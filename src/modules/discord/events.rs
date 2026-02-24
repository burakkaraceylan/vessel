use crate::module::{IntoModuleEvent, ModuleEvent};
use crate::modules::discord::voice;
use serde_json::Value;

pub enum DiscordEvent {
    VoiceSettingsUpdate(voice::VoiceSettings),
    SelectedVoiceChannel(Option<Value>),
    VoiceChannelJoined(Value),
    VoiceChannelLeft,
    SpeakingStart { user_id: String },
    SpeakingStop { user_id: String },
    // Fired by Discord whenever the local user switches voice channels (or leaves).
    VoiceChannelSelect { channel_id: Option<String> },
}

impl IntoModuleEvent for DiscordEvent {
    fn into_event(self) -> ModuleEvent {
        match self {
            DiscordEvent::VoiceSettingsUpdate(settings) => ModuleEvent::Stateful {
                source: "discord",
                event: "voice_settings_update".to_string(),
                data: serde_json::to_value(settings).unwrap_or_default(),
                cache_key: "discord/voice_settings_update".to_string(),
            },
            DiscordEvent::SelectedVoiceChannel(channel) => ModuleEvent::Stateful {
                source: "discord",
                event: "selected_voice_channel".to_string(),
                data: channel.unwrap_or(Value::Null),
                cache_key: "discord/selected_voice_channel".to_string(),
            },
            // Transition events — canonical state is SelectedVoiceChannel.
            DiscordEvent::VoiceChannelJoined(data) => ModuleEvent::Transient {
                source: "discord",
                event: "voice_channel_joined".to_string(),
                data,
            },
            DiscordEvent::VoiceChannelLeft => ModuleEvent::Transient {
                source: "discord",
                event: "voice_channel_left".to_string(),
                data: Value::Null,
            },
            // Consumed internally to re-subscribe speaking; never forwarded to clients.
            DiscordEvent::VoiceChannelSelect { channel_id } => ModuleEvent::Transient {
                source: "discord",
                event: "voice_channel_select".to_string(),
                data: match channel_id {
                    Some(id) => serde_json::json!({ "channel_id": id }),
                    None => Value::Null,
                },
            },
            // Raw speaking events are consumed internally; never forwarded to clients.
            DiscordEvent::SpeakingStart { user_id } => ModuleEvent::Transient {
                source: "discord",
                event: "speaking_start".to_string(),
                data: serde_json::json!({ "user_id": user_id }),
            },
            DiscordEvent::SpeakingStop { user_id } => ModuleEvent::Transient {
                source: "discord",
                event: "speaking_stop".to_string(),
                data: serde_json::json!({ "user_id": user_id }),
            },
        }
    }
}
