use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::module::FromModuleCommand;

pub enum SystemCommand {
    SendKeys(String),
    SpawnExe { exe: String, args: Vec<String> },
    OpenUri(String),
}

impl FromModuleCommand for SystemCommand {
    fn from_command(action: &str, params: &Value) -> Result<Self> {
        match action {
            "send_keys" => {
                let keys = params["keys"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing string param 'keys'"))?
                    .to_string();
                Ok(SystemCommand::SendKeys(keys))
            }
            "spawn_exe" => {
                let exe = params["exe"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing string param 'exe'"))?
                    .to_string();
                let args = params["args"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(SystemCommand::SpawnExe { exe, args })
            }
            "open_uri" => {
                let uri = params["uri"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing string param 'uri'"))?
                    .to_string();
                Ok(SystemCommand::OpenUri(uri))
            }
            _ => Err(anyhow!("unknown system command '{}'", action)),
        }
    }
}
