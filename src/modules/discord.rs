pub mod commands;
pub mod events;
pub mod ipc;
pub mod oauth;
pub mod token_cache;
pub mod voice;

use crate::module::FromModuleCommand;
use crate::module::IntoModuleEvent;
use crate::module::Module;
use crate::module::ModuleContext;
use crate::module::ModuleEvent;
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use commands::DiscordCommand;
use events::DiscordEvent;
use std::collections::HashSet;
use tokio::sync::Mutex;
use tracing::{info, warn};

pub struct DiscordModule {
    pub voice_controller: Mutex<voice::DiscordVoiceController>,
    speaking_users: Mutex<HashSet<String>>,
    client_id: String,
    client_secret: String,
}

impl DiscordModule {
    async fn handle_command(&self, cmd: DiscordCommand) -> Result<ModuleEvent> {
        let mut vc = self.voice_controller.lock().await;
        let event = match cmd {
            DiscordCommand::SetMute(mute) => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_mute(mute).await?)
            }
            DiscordCommand::SetDeaf(deaf) => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_deaf(deaf).await?)
            }
            DiscordCommand::SetInputVolume(vol) => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_input_volume(vol).await?)
            }
            DiscordCommand::SetOutputVolume(vol) => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_output_volume(vol).await?)
            }
            DiscordCommand::SetVoiceActivity => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_voice_activity().await?)
            }
            DiscordCommand::SetPushToTalk => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_push_to_talk().await?)
            }
            DiscordCommand::SetInputDevice(id) => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_input_device(&id).await?)
            }
            DiscordCommand::SetOutputDevice(id) => {
                DiscordEvent::VoiceSettingsUpdate(vc.set_output_device(&id).await?)
            }
            DiscordCommand::GetVoiceSettings => {
                DiscordEvent::VoiceSettingsUpdate(vc.get_voice_settings().await?)
            }
            DiscordCommand::GetSelectedVoiceChannel => {
                DiscordEvent::SelectedVoiceChannel(vc.get_selected_voice_channel().await?)
            }
            DiscordCommand::SelectVoiceChannel { channel_id, force } => {
                DiscordEvent::VoiceChannelJoined(vc.select_voice_channel(&channel_id, force).await?)
            }
            DiscordCommand::LeaveVoiceChannel => {
                vc.leave_voice_channel().await?;
                DiscordEvent::VoiceChannelLeft
            }
        };
        Ok(event.into_event())
    }
}

#[async_trait]
impl Module for DiscordModule {
    async fn new(config: toml::Table) -> Result<Self, anyhow::Error> {
        let client_id = config
            .get("client_id")
            .context("client_id missing from config")?
            .as_str()
            .context("client_id is not a string")?;
        let client_secret = config
            .get("client_secret")
            .context("client_secret missing from config")?
            .as_str()
            .context("client_secret is not a string")?;
        let voice_controller =
            voice::DiscordVoiceController::connect_and_auth(client_id, client_secret)
                .await
                .context("Failed to connect and authenticate with Discord voice controller")?;
        Ok(DiscordModule {
            voice_controller: Mutex::new(voice_controller),
            speaking_users: Mutex::new(HashSet::new()),
            client_id: client_id.to_owned(),
            client_secret: client_secret.to_owned(),
        })
    }

    fn name(&self) -> &'static str {
        "discord"
    }

    async fn run(&self, mut ctx: ModuleContext) -> Result<(), anyhow::Error> {
        self.voice_controller
            .lock()
            .await
            .subscribe_voice_settings()
            .await?;

        // Fetch and emit initial state so the cache is populated before any client connects.
        match self.voice_controller.lock().await.get_voice_settings().await {
            Ok(settings) => { let _ = ctx.event_tx.send(DiscordEvent::VoiceSettingsUpdate(settings).into_event()); }
            Err(e) => warn!("Failed to fetch initial voice settings: {}", e),
        }

        let maybe_channel = self
            .voice_controller
            .lock()
            .await
            .get_selected_voice_channel()
            .await;

        // Emit the current channel (or null) so clients know the initial channel state.
        let _ = ctx.event_tx.send(DiscordEvent::SelectedVoiceChannel(
            maybe_channel.as_ref().ok().and_then(|c| c.clone()),
        ).into_event());

        // If we're already in a channel when the module starts, subscribe to speaking now.
        if let Ok(Some(channel)) = maybe_channel {
            if let Some(channel_id) = channel["id"].as_str().map(|s| s.to_string()) {
                if let Err(e) = self
                    .voice_controller
                    .lock()
                    .await
                    .subscribe_speaking(&channel_id)
                    .await
                {
                    warn!("Failed to subscribe to speaking for channel {}: {}", channel_id, e);
                }
            }
        }

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    info!("Discord module shutting down");
                    break;
                }

                Some(cmd) = ctx.rx.recv() => {
                    match DiscordCommand::from_command(&cmd.action, &cmd.params) {
                        Ok(discord_cmd) => {
                            match self.handle_command(discord_cmd).await {
                                Ok(event) => {
                                    // When joining a channel, subscribe to speaking events.
                                    if event.event_name() == "voice_channel_joined" {
                                        if let Some(channel_id) = event.data()["id"].as_str().map(|s| s.to_string()) {
                                            if let Err(e) = self.voice_controller.lock().await.subscribe_speaking(&channel_id).await {
                                                warn!("Failed to subscribe to speaking: {}", e);
                                            }
                                        }
                                    }
                                    let _ = ctx.event_tx.send(event);
                                }
                                Err(e) => {
                                    warn!("Discord command '{}' failed: {}", cmd.action, e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Invalid discord command '{}': {}", cmd.action, e);
                        }
                    }
                }

                result = async { self.voice_controller.lock().await.recv_event().await } => {
                    match result {
                        Ok(event) => {
                            match event.event_name() {
                                "speaking_start" => {
                                    if let Some(user_id) = event.data()["user_id"].as_str() {
                                        let mut users = self.speaking_users.lock().await;
                                        users.insert(user_id.to_string());
                                        let active = !users.is_empty();
                                        let _ = ctx.event_tx.send(ModuleEvent::Stateful {
                                            source: "discord",
                                            event: "speaking".to_string(),
                                            data: serde_json::json!({ "active": active }),
                                            cache_key: "discord/speaking",
                                        });
                                    }
                                }
                                "speaking_stop" => {
                                    if let Some(user_id) = event.data()["user_id"].as_str() {
                                        let mut users = self.speaking_users.lock().await;
                                        users.remove(user_id);
                                        let active = !users.is_empty();
                                        let _ = ctx.event_tx.send(ModuleEvent::Stateful {
                                            source: "discord",
                                            event: "speaking".to_string(),
                                            data: serde_json::json!({ "active": active }),
                                            cache_key: "discord/speaking",
                                        });
                                    }
                                }
                                _ => {
                                    let _ = ctx.event_tx.send(event);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Discord event recv error: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
