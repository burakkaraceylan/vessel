use crate::module::{IntoModuleEvent, ModuleEvent};
use crate::modules::discord::events::DiscordEvent;
use crate::modules::discord::ipc::DiscordIpc;
use crate::modules::discord::oauth;
use crate::modules::discord::token_cache;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

/// Voice settings as returned by GET_VOICE_SETTINGS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSettings {
    pub mute: bool,
    pub deaf: bool,
    pub input: Option<AudioDevice>,
    pub output: Option<AudioDevice>,
    pub mode: Option<VoiceMode>,
    pub automatic_gain_control: Option<bool>,
    pub echo_cancellation: Option<bool>,
    pub noise_suppression: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub device_id: Option<String>,
    pub volume: Option<f64>,
    pub available_devices: Option<Vec<DeviceInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceMode {
    #[serde(rename = "type")]
    pub mode_type: Option<String>, // "VOICE_ACTIVITY" or "PUSH_TO_TALK"
    pub auto_threshold: Option<bool>,
    pub threshold: Option<f64>,
    pub delay: Option<f64>,
}

/// High-level controller for Discord voice via local RPC.
///
/// # Setup (one-time)
/// 1. Go to https://discord.com/developers/applications
/// 2. Create a new application
/// 3. Copy the Application ID (= client_id) and Client Secret
/// 4. Under OAuth2 → Redirects, add: https://localhost
///
/// # Scopes needed
/// - `rpc` — basic RPC access
/// - `rpc.voice.read` — read voice settings & state
/// - `rpc.voice.write` — modify voice settings (mute/unmute/etc)
pub struct DiscordVoiceController {
    ipc: DiscordIpc,
    authenticated: bool,
}

impl DiscordVoiceController {
    /// Connect to Discord's named pipe and perform the handshake.
    pub async fn connect(client_id: &str) -> Result<Self> {
        let mut ipc = DiscordIpc::connect().await?;
        ipc.handshake(client_id).await?;

        Ok(Self {
            ipc,
            authenticated: false,
        })
    }

    /// Step 1 of auth: Send AUTHORIZE, which pops up Discord's consent dialog.
    /// Returns the authorization code to exchange for a token.
    pub async fn authorize(&mut self, client_id: &str) -> Result<String> {
        let resp = self
            .ipc
            .command(
                "AUTHORIZE",
                serde_json::json!({
                    "client_id": client_id,
                    "scopes": ["rpc", "rpc.voice.read", "rpc.voice.write"],
                }),
            )
            .await?;

        let code = resp["data"]["code"]
            .as_str()
            .context("No authorization code in AUTHORIZE response")?
            .to_string();

        info!("Got auth code from user consent");
        Ok(code)
    }

    /// Step 2 of auth: Exchange the code for a token, then AUTHENTICATE.
    pub async fn authenticate(
        &mut self,
        client_id: &str,
        client_secret: &str,
        code: &str,
    ) -> Result<()> {
        // Exchange code for access token via Discord's HTTP API
        let token_resp = oauth::exchange_code(client_id, client_secret, code).await?;

        // Send AUTHENTICATE over IPC with the access token
        let resp = self
            .ipc
            .command(
                "AUTHENTICATE",
                serde_json::json!({
                    "access_token": token_resp.access_token,
                }),
            )
            .await?;

        info!(
            "Authenticated as: {}",
            resp["data"]["user"]["username"]
                .as_str()
                .unwrap_or("unknown")
        );

        self.authenticated = true;

        // Cache the token for subsequent runs
        if let Err(e) = token_cache::save(&token_resp) {
            warn!("Failed to cache token: {}", e);
        }

        Ok(())
    }

    /// Try to AUTHENTICATE with an access token over IPC.
    /// Returns Ok(true) on success, Ok(false) if Discord rejected the token.
    async fn try_authenticate(&mut self, access_token: &str) -> Result<bool> {
        match self
            .ipc
            .command(
                "AUTHENTICATE",
                serde_json::json!({ "access_token": access_token }),
            )
            .await
        {
            Ok(resp) => {
                info!(
                    "Authenticated as: {}",
                    resp["data"]["user"]["username"]
                        .as_str()
                        .unwrap_or("unknown")
                );
                self.authenticated = true;
                Ok(true)
            }
            Err(e) => {
                warn!("AUTHENTICATE with cached token failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Full auth flow in one call.
    ///
    /// Tries, in order:
    /// 1. Cached access token (no popup)
    /// 2. Refresh token if cached token expired (no popup)
    /// 3. Full AUTHORIZE flow (popup)
    pub async fn connect_and_auth(client_id: &str, client_secret: &str) -> Result<Self> {
        let mut ctrl = Self::connect(client_id).await?;

        // Try cached token
        if let Ok(Some(cached)) = token_cache::load() {
            if !cached.is_expired() {
                info!("Trying cached access token...");
                if ctrl.try_authenticate(&cached.access_token).await? {
                    return Ok(ctrl);
                }
                // Token was rejected — clear and continue
                let _ = token_cache::clear();
            } else if let Some(ref refresh) = cached.refresh_token {
                info!("Cached token expired, attempting refresh...");
                match oauth::refresh_token(client_id, client_secret, refresh).await {
                    Ok(token_resp) => {
                        if ctrl.try_authenticate(&token_resp.access_token).await? {
                            let _ = token_cache::save(&token_resp);
                            return Ok(ctrl);
                        }
                    }
                    Err(e) => {
                        warn!("Token refresh failed: {}", e);
                    }
                }
                let _ = token_cache::clear();
            } else {
                // Expired with no refresh token — clear
                let _ = token_cache::clear();
            }
        }

        // Fall back to full AUTHORIZE flow (shows popup)
        info!("Starting full authorization flow (consent dialog)...");
        let code = ctrl.authorize(client_id).await?;
        ctrl.authenticate(client_id, client_secret, &code).await?;

        Ok(ctrl)
    }

    // ─── Voice Controls ────────────────────────────────────────

    /// Get current voice settings (mute, deaf, devices, mode, etc.)
    pub async fn get_voice_settings(&mut self) -> Result<VoiceSettings> {
        let resp = self
            .ipc
            .command("GET_VOICE_SETTINGS", serde_json::json!({}))
            .await?;
        let data = &resp["data"];
        let settings: VoiceSettings =
            serde_json::from_value(data.clone()).context("Failed to parse voice settings")?;
        Ok(settings)
    }

    /// Set voice settings. Only the fields you pass will be modified.
    /// Note: Discord only allows one app to control voice settings at a time.
    /// Your app "locks" voice settings while connected.
    async fn set_voice_settings(&mut self, args: Value) -> Result<VoiceSettings> {
        let resp = self.ipc.command("SET_VOICE_SETTINGS", args).await?;
        let data = &resp["data"];
        let settings: VoiceSettings =
            serde_json::from_value(data.clone()).context("Failed to parse voice settings")?;
        Ok(settings)
    }

    /// Toggle microphone mute
    pub async fn set_mute(&mut self, mute: bool) -> Result<VoiceSettings> {
        info!("Setting mute: {}", mute);
        self.set_voice_settings(serde_json::json!({ "mute": mute }))
            .await
    }

    /// Toggle deafen (mutes both mic AND audio output)
    pub async fn set_deaf(&mut self, deaf: bool) -> Result<VoiceSettings> {
        info!("Setting deaf: {}", deaf);
        self.set_voice_settings(serde_json::json!({ "deaf": deaf }))
            .await
    }

    /// Set input (microphone) volume. Range: 0.0 - 100.0
    pub async fn set_input_volume(&mut self, volume: f64) -> Result<VoiceSettings> {
        info!("Setting input volume: {}", volume);
        self.set_voice_settings(serde_json::json!({
            "input": { "volume": volume.clamp(0.0, 100.0) }
        }))
        .await
    }

    /// Set output (speaker) volume. Range: 0.0 - 200.0
    pub async fn set_output_volume(&mut self, volume: f64) -> Result<VoiceSettings> {
        info!("Setting output volume: {}", volume);
        self.set_voice_settings(serde_json::json!({
            "output": { "volume": volume.clamp(0.0, 200.0) }
        }))
        .await
    }

    /// Set voice mode to Voice Activity Detection
    pub async fn set_voice_activity(&mut self) -> Result<VoiceSettings> {
        info!("Setting mode: voice activity");
        self.set_voice_settings(serde_json::json!({
            "mode": { "type": "VOICE_ACTIVITY", "auto_threshold": true }
        }))
        .await
    }

    /// Set voice mode to Push-to-Talk
    pub async fn set_push_to_talk(&mut self) -> Result<VoiceSettings> {
        info!("Setting mode: push to talk");
        self.set_voice_settings(serde_json::json!({
            "mode": { "type": "PUSH_TO_TALK" }
        }))
        .await
    }

    /// Set input device by device ID
    pub async fn set_input_device(&mut self, device_id: &str) -> Result<VoiceSettings> {
        info!("Setting input device: {}", device_id);
        self.set_voice_settings(serde_json::json!({
            "input": { "device_id": device_id }
        }))
        .await
    }

    /// Set output device by device ID
    pub async fn set_output_device(&mut self, device_id: &str) -> Result<VoiceSettings> {
        info!("Setting output device: {}", device_id);
        self.set_voice_settings(serde_json::json!({
            "output": { "device_id": device_id }
        }))
        .await
    }

    // ─── Event Subscriptions ───────────────────────────────────

    /// Subscribe to voice settings changes.
    /// After subscribing, you'll receive VOICE_SETTINGS_UPDATE events
    /// when calling recv_event().
    pub async fn subscribe_voice_settings(&mut self) -> Result<()> {
        self.ipc
            .subscribe("VOICE_SETTINGS_UPDATE", serde_json::json!({}))
            .await?;
        info!("Subscribed to VOICE_SETTINGS_UPDATE");
        Ok(())
    }

    /// Subscribe to voice connection status changes.
    pub async fn subscribe_voice_connection_status(&mut self) -> Result<()> {
        self.ipc
            .subscribe("VOICE_CONNECTION_STATUS", serde_json::json!({}))
            .await?;
        info!("Subscribed to VOICE_CONNECTION_STATUS");
        Ok(())
    }

    /// Subscribe to speaking start/stop events for a voice channel.
    pub async fn subscribe_speaking(&mut self, channel_id: &str) -> Result<()> {
        self.ipc
            .subscribe(
                "SPEAKING_START",
                serde_json::json!({ "channel_id": channel_id }),
            )
            .await?;
        self.ipc
            .subscribe(
                "SPEAKING_STOP",
                serde_json::json!({ "channel_id": channel_id }),
            )
            .await?;
        info!("Subscribed to SPEAKING events for channel {}", channel_id);
        Ok(())
    }

    /// Read the next event/response from Discord (blocking).
    /// Use this in a loop after subscribing to events.
    pub async fn recv_event(&mut self) -> Result<ModuleEvent> {
        let (_opcode, data) = self.ipc.recv().await?;

        let evt = data["evt"]
            .as_str()
            .ok_or_else(|| anyhow!("missing evt field"))?;
        match evt {
            "VOICE_SETTINGS_UPDATE" => {
                let settings: VoiceSettings = serde_json::from_value(data["data"].clone())
                    .context("Failed to parse VOICE_SETTINGS_UPDATE data")?;
                Ok(DiscordEvent::VoiceSettingsUpdate(settings).into_event())
            }
            "SPEAKING_START" => {
                let user_id = data["data"]["user_id"]
                    .as_str()
                    .context("missing user_id in SPEAKING_START")?
                    .to_string();
                Ok(DiscordEvent::SpeakingStart { user_id }.into_event())
            }
            "SPEAKING_STOP" => {
                let user_id = data["data"]["user_id"]
                    .as_str()
                    .context("missing user_id in SPEAKING_STOP")?
                    .to_string();
                Ok(DiscordEvent::SpeakingStop { user_id }.into_event())
            }
            other => Err(anyhow!("unknown discord event: {}", other)),
        }
    }

    // ─── Channel Info ──────────────────────────────────────────

    /// Get the currently selected voice channel
    pub async fn get_selected_voice_channel(&mut self) -> Result<Option<Value>> {
        let resp = self
            .ipc
            .command("GET_SELECTED_VOICE_CHANNEL", serde_json::json!({}))
            .await?;
        let data = &resp["data"];
        if data.is_null() {
            Ok(None)
        } else {
            Ok(Some(data.clone()))
        }
    }

    /// Join a voice channel by ID
    pub async fn select_voice_channel(&mut self, channel_id: &str, force: bool) -> Result<Value> {
        let resp = self
            .ipc
            .command(
                "SELECT_VOICE_CHANNEL",
                serde_json::json!({
                    "channel_id": channel_id,
                    "force": force,
                }),
            )
            .await?;
        Ok(resp["data"].clone())
    }

    /// Leave the current voice channel
    pub async fn leave_voice_channel(&mut self) -> Result<()> {
        self.ipc
            .command(
                "SELECT_VOICE_CHANNEL",
                serde_json::json!({ "channel_id": null }),
            )
            .await?;
        Ok(())
    }
}
