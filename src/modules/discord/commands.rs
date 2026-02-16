use crate::module::FromModuleCommand;
use anyhow::{anyhow, Result};
use serde_json::Value;

pub enum DiscordCommand {
    SetMute(bool),
    SetDeaf(bool),
    SetInputVolume(f64),
    SetOutputVolume(f64),
    SetVoiceActivity,
    SetPushToTalk,
    SetInputDevice(String),
    SetOutputDevice(String),
    GetVoiceSettings,
    GetSelectedVoiceChannel,
    SelectVoiceChannel { channel_id: String, force: bool },
    LeaveVoiceChannel,
}

impl FromModuleCommand for DiscordCommand {
    fn from_command(action: &str, params: &Value) -> Result<Self> {
        match action {
            "set_mute" => {
                let mute = params["mute"]
                    .as_bool()
                    .ok_or_else(|| anyhow!("missing bool param 'mute'"))?;
                Ok(DiscordCommand::SetMute(mute))
            }
            "set_deaf" => {
                let deaf = params["deaf"]
                    .as_bool()
                    .ok_or_else(|| anyhow!("missing bool param 'deaf'"))?;
                Ok(DiscordCommand::SetDeaf(deaf))
            }
            "set_input_volume" => {
                let volume = params["volume"]
                    .as_f64()
                    .ok_or_else(|| anyhow!("missing f64 param 'volume'"))?;
                Ok(DiscordCommand::SetInputVolume(volume))
            }
            "set_output_volume" => {
                let volume = params["volume"]
                    .as_f64()
                    .ok_or_else(|| anyhow!("missing f64 param 'volume'"))?;
                Ok(DiscordCommand::SetOutputVolume(volume))
            }
            "set_voice_activity" => Ok(DiscordCommand::SetVoiceActivity),
            "set_push_to_talk" => Ok(DiscordCommand::SetPushToTalk),
            "set_input_device" => {
                let device_id = params["device_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing string param 'device_id'"))?
                    .to_string();
                Ok(DiscordCommand::SetInputDevice(device_id))
            }
            "set_output_device" => {
                let device_id = params["device_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing string param 'device_id'"))?
                    .to_string();
                Ok(DiscordCommand::SetOutputDevice(device_id))
            }
            "get_voice_settings" => Ok(DiscordCommand::GetVoiceSettings),
            "get_selected_voice_channel" => Ok(DiscordCommand::GetSelectedVoiceChannel),
            "select_voice_channel" => {
                let channel_id = params["channel_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing string param 'channel_id'"))?
                    .to_string();
                let force = params["force"].as_bool().unwrap_or(false);
                Ok(DiscordCommand::SelectVoiceChannel { channel_id, force })
            }
            "leave_voice_channel" => Ok(DiscordCommand::LeaveVoiceChannel),
            _ => Err(anyhow!("unknown discord action: {}", action)),
        }
    }
}
