use crate::module::FromModuleCommand;
use anyhow::{Result, anyhow};
use serde_json::Value;

pub enum MediaCommand {
    Play,
    Pause,
    TogglePlayPause,
    Stop,
    Next,
    Previous,
    SetVolume(f64),
    GetStatus,
}

impl FromModuleCommand for MediaCommand {
    fn from_command(action: &str, params: &Value) -> Result<Self> {
        match action {
            "play" => Ok(MediaCommand::Play),
            "pause" => Ok(MediaCommand::Pause),
            "toggle_play_pause" => Ok(MediaCommand::TogglePlayPause),
            "stop" => Ok(MediaCommand::Stop),
            "next" => Ok(MediaCommand::Next),
            "previous" => Ok(MediaCommand::Previous),
            "set_volume" => {
                let volume = params["volume"]
                    .as_f64()
                    .ok_or_else(|| anyhow!("missing f64 param 'volume'"))?;
                Ok(MediaCommand::SetVolume(volume))
            }
            "get_status" => Ok(MediaCommand::GetStatus),
            _ => Err(anyhow!("unknown command action '{}'", action)),
        }
    }
}
