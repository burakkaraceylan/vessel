use crate::module::{IntoModuleEvent, ModuleEvent};
use crate::modules::discord::voice;
use serde_json::Value;

pub enum DiscordEvent {
    VoiceSettingsUpdate(voice::VoiceSettings),
    SelectedVoiceChannel(Option<Value>),
    VoiceChannelJoined(Value),
    VoiceChannelLeft,
}

impl IntoModuleEvent for DiscordEvent {
    fn into_event(self) -> ModuleEvent {
        match self {
            DiscordEvent::VoiceSettingsUpdate(settings) => ModuleEvent {
                source: "discord",
                event: "voice_settings_update".to_string(),
                data: serde_json::to_value(settings).unwrap_or_default(),
            },
            DiscordEvent::SelectedVoiceChannel(channel) => ModuleEvent {
                source: "discord",
                event: "selected_voice_channel".to_string(),
                data: channel.unwrap_or(Value::Null),
            },
            DiscordEvent::VoiceChannelJoined(data) => ModuleEvent {
                source: "discord",
                event: "voice_channel_joined".to_string(),
                data,
            },
            DiscordEvent::VoiceChannelLeft => ModuleEvent {
                source: "discord",
                event: "voice_channel_left".to_string(),
                data: Value::Null,
            },
        }
    }
}
