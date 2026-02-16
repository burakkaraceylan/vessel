use crate::config;
use crate::module::ModuleEvent;
use crate::module_manager::ModuleManager;
use crate::protocol::{IncomingMessage, OutgoingMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

pub struct Vessel {
    tcp_listener: TcpListener,
    ws_listener: TcpListener,
    pub module_manager: ModuleManager,
}

impl Vessel {
    pub async fn new(config: &config::Config) -> Result<Self, Box<dyn std::error::Error>> {
        let tcp_listener = TcpListener::bind(format!("{}:8000", config.host)).await?;
        let ws_listener = TcpListener::bind(format!("{}:8001", config.host)).await?;
        let module_manager = ModuleManager::new();
        Ok(Vessel {
            tcp_listener,
            ws_listener,
            module_manager,
        })
    }

    pub async fn run(mut self, token: CancellationToken) -> Result<(), Box<dyn std::error::Error>> {
        let mut event_rx = self
            .module_manager
            .take_event()
            .expect("event_rx already taken");

        self.module_manager.run_all(token.clone()).await?;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    println!("Vessel is shutting down.");
                    break;
                }
                result = self.tcp_listener.accept() => {
                    let (socket, addr) = result?;
                    println!("Companion connected: {:?}", addr);

                    if let Err(e) = handle_connection(
                        socket,
                        &self.module_manager,
                        &mut event_rx,
                        token.clone(),
                    ).await {
                        eprintln!("Connection error: {}", e);
                    }

                    println!("Companion disconnected, waiting for reconnect...");
                }
                result = self.ws_listener.accept() => {
                    let (socket, addr) = result?;
                    println!("Web client connected: {:?}", addr);
                    if let Err(e) = handle_websocket(
                        socket,
                        &self.module_manager,
                        &mut event_rx,
                        token.clone(),
                    ).await {
                        eprintln!("WebSocket error: {}", e);
                    }
                    println!("Web client disconnected, waiting for reconnect...");
                }
            }
        }
        Ok(())
    }
}

async fn handle_websocket(
    socket: TcpStream,
    module_manager: &ModuleManager,
    event_rx: &mut mpsc::Receiver<ModuleEvent>,
    cancel_token: CancellationToken,
) -> anyhow::Result<()> {
    let ws_stream = accept_async(socket).await?;
    let (mut write, mut read) = ws_stream.split();

    println!("WebSocket connection established");
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                break;
            }

            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        for line in text.lines() {
                            if line.is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<IncomingMessage>(line) {
                                Ok(msg) => {
                                    if let Err(e) = module_manager.route_command(
                                        &msg.module, msg.action, msg.params,
                                    ).await {
                                        eprintln!("Route error: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Invalid JSON: {}", e);
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        eprintln!("WebSocket read error: {}", e);
                        break;
                    }
                    _ => {} // ping/pong/binary - ignore
                }
            }

            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        let msg = OutgoingMessage::from(event);
                        let json = serde_json::to_string(&msg)?;
                        write.send(Message::Text(json.into())).await?;
                    }
                    None => break,
                }
            }
        }
    }

    Ok(())
}

async fn handle_connection(
    socket: TcpStream,
    module_manager: &ModuleManager,
    event_rx: &mut mpsc::Receiver<ModuleEvent>,
    cancel_token: CancellationToken,
) -> anyhow::Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut lines = BufReader::new(reader).lines();

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                break;
            }

            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        match serde_json::from_str::<IncomingMessage>(&line) {
                            Ok(msg) => {
                                if let Err(e) = module_manager.route_command(
                                    &msg.module, msg.action, msg.params,
                                ).await {
                                    eprintln!("Route error: {}", e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Invalid JSON: {}", e);
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        eprintln!("Read error: {}", e);
                        break;
                    }
                }
            }

            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        let msg = OutgoingMessage::from(event);
                        let mut json = serde_json::to_string(&msg)?;
                        json.push('\n');
                        writer.write_all(json.as_bytes()).await?;
                    }
                    None => break,
                }
            }
        }
    }

    Ok(())
}
