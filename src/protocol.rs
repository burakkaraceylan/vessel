use crate::module::{ModuleCommand, ModuleEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Companion → Vessel
#[derive(Deserialize)]
pub struct IncomingMessage {
    pub module: String,
    pub action: String,
    #[serde(default)]
    pub params: Value,
}

// Vessel → Companion
#[derive(Serialize)]
pub struct OutgoingMessage {
    pub module: &'static str,
    pub event: String,
    pub data: Value,
}

impl From<ModuleEvent> for OutgoingMessage {
    fn from(event: ModuleEvent) -> Self {
        OutgoingMessage {
            module: event.source,
            event: event.event,
            data: event.data,
        }
    }
}
