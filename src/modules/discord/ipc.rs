use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::io::{Read, Write};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tracing::{debug, info, warn};

/// Discord IPC opcodes
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl TryFrom<u32> for OpCode {
    type Error = anyhow::Error;
    fn try_from(value: u32) -> Result<Self> {
        match value {
            0 => Ok(Self::Handshake),
            1 => Ok(Self::Frame),
            2 => Ok(Self::Close),
            3 => Ok(Self::Ping),
            4 => Ok(Self::Pong),
            _ => Err(anyhow!("Unknown opcode: {}", value)),
        }
    }
}

/// Low-level IPC connection to Discord's named pipe.
///
/// Protocol: Each frame is [opcode: u32 LE][length: u32 LE][json payload: bytes]
/// The pipe MUST receive the full frame in a single write.
pub struct DiscordIpc {
    pipe: NamedPipeClient,
}

impl DiscordIpc {
    /// Try connecting to discord-ipc-0 through discord-ipc-9
    pub async fn connect() -> Result<Self> {
        for i in 0..10 {
            let path = format!(r"\\.\pipe\discord-ipc-{}", i);
            debug!("Trying pipe: {}", path);

            match ClientOptions::new().read(true).write(true).open(&path) {
                Ok(pipe) => {
                    info!("Connected to {}", path);
                    return Ok(Self { pipe });
                }
                Err(e) => {
                    debug!("Pipe {} unavailable: {}", path, e);
                    continue;
                }
            }
        }
        Err(anyhow!("Could not connect to Discord. Is it running?"))
    }

    /// Send a frame: writes [opcode][length][payload] as a single buffer.
    pub async fn send(&mut self, opcode: OpCode, data: &Value) -> Result<()> {
        let payload = serde_json::to_string(data)?;
        let payload_bytes = payload.as_bytes();
        let len = payload_bytes.len() as u32;

        // Must write as a single buffer — Discord will break otherwise
        let mut buf = Vec::with_capacity(8 + payload_bytes.len());
        buf.extend_from_slice(&(opcode as u32).to_le_bytes());
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(payload_bytes);

        self.pipe
            .write_all(&buf)
            .await
            .context("Failed to write to Discord pipe")?;
        self.pipe.flush().await?;

        debug!("Sent opcode {:?}, {} bytes", opcode, len);
        Ok(())
    }

    /// Read a frame: returns (opcode, parsed JSON)
    pub async fn recv(&mut self) -> Result<(OpCode, Value)> {
        // Read 8-byte header
        let mut header = [0u8; 8];
        self.pipe
            .read_exact(&mut header)
            .await
            .context("Failed to read header from Discord pipe")?;

        let opcode_raw = u32::from_le_bytes(header[0..4].try_into()?);
        let length = u32::from_le_bytes(header[4..8].try_into()?) as usize;

        let opcode = OpCode::try_from(opcode_raw)?;

        // Read JSON payload
        let mut payload = vec![0u8; length];
        self.pipe
            .read_exact(&mut payload)
            .await
            .context("Failed to read payload from Discord pipe")?;

        let data: Value =
            serde_json::from_slice(&payload).context("Failed to parse JSON from Discord")?;

        debug!("Recv opcode {:?}, {} bytes", opcode, length);
        Ok((opcode, data))
    }

    /// Send handshake: opcode 0 with version and client_id
    pub async fn handshake(&mut self, client_id: &str) -> Result<Value> {
        let payload = serde_json::json!({
            "v": 1,
            "client_id": client_id,
        });
        self.send(OpCode::Handshake, &payload).await?;

        let (opcode, data) = self.recv().await?;
        match opcode {
            OpCode::Frame => {
                // Should be DISPATCH/READY
                info!("Handshake successful");
                debug!("READY: {}", serde_json::to_string_pretty(&data)?);
                Ok(data)
            }
            OpCode::Close => {
                let code = data.get("code").and_then(|v| v.as_u64()).unwrap_or(0);
                let msg = data
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                Err(anyhow!("Discord rejected handshake: {} ({})", msg, code))
            }
            _ => Err(anyhow!("Unexpected opcode during handshake: {:?}", opcode)),
        }
    }

    /// Send a SUBSCRIBE frame with `evt` at the top level (as Discord requires).
    pub async fn subscribe(&mut self, evt: &str, args: Value) -> Result<Value> {
        let nonce = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "cmd": "SUBSCRIBE",
            "evt": evt,
            "args": args,
            "nonce": nonce,
        });
        self.send(OpCode::Frame, &payload).await?;

        loop {
            let (opcode, data) = self.recv().await?;
            match opcode {
                OpCode::Frame => {
                    let resp_nonce = data.get("nonce").and_then(|v| v.as_str());
                    if resp_nonce == Some(&nonce) {
                        if let Some(evt) = data.get("evt").and_then(|v| v.as_str()) {
                            if evt == "ERROR" {
                                let err_data = &data["data"];
                                return Err(anyhow!(
                                    "Discord RPC error {}: {}",
                                    err_data.get("code").and_then(|v| v.as_u64()).unwrap_or(0),
                                    err_data
                                        .get("message")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                ));
                            }
                        }
                        return Ok(data);
                    } else {
                        debug!(
                            "Received event while waiting for response: {}",
                            data.get("evt")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        );
                    }
                }
                OpCode::Close => {
                    return Err(anyhow!("Connection closed by Discord"));
                }
                OpCode::Ping => {
                    self.send(OpCode::Pong, &data).await?;
                }
                _ => {
                    warn!("Unexpected opcode: {:?}", opcode);
                }
            }
        }
    }

    /// Send a command frame and receive the response
    pub async fn command(&mut self, cmd: &str, args: Value) -> Result<Value> {
        let nonce = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "cmd": cmd,
            "args": args,
            "nonce": nonce,
        });
        self.send(OpCode::Frame, &payload).await?;

        // Read responses until we get one matching our nonce
        // (events can arrive between request and response)
        loop {
            let (opcode, data) = self.recv().await?;
            match opcode {
                OpCode::Frame => {
                    let resp_nonce = data.get("nonce").and_then(|v| v.as_str());
                    if resp_nonce == Some(&nonce) {
                        // Check for error
                        if let Some(evt) = data.get("evt").and_then(|v| v.as_str()) {
                            if evt == "ERROR" {
                                let err_data = &data["data"];
                                return Err(anyhow!(
                                    "Discord RPC error {}: {}",
                                    err_data.get("code").and_then(|v| v.as_u64()).unwrap_or(0),
                                    err_data
                                        .get("message")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                ));
                            }
                        }
                        return Ok(data);
                    } else {
                        // This is an event or response to a different command
                        debug!(
                            "Received event while waiting for response: {}",
                            data.get("evt")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        );
                    }
                }
                OpCode::Close => {
                    return Err(anyhow!("Connection closed by Discord"));
                }
                OpCode::Ping => {
                    // Respond with pong
                    self.send(OpCode::Pong, &data).await?;
                }
                _ => {
                    warn!("Unexpected opcode: {:?}", opcode);
                }
            }
        }
    }
}
