use crate::module::ModuleEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Client → Vessel
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncomingMessage {
    /// Fire-and-forget command routed to a module.
    Call {
        request_id: String,
        module: String,
        name: String,
        #[serde(default = "default_version")]
        version: u32,
        #[serde(default)]
        params: Value,
    },
    /// Ask to receive future events matching this module+name.
    Subscribe {
        module: String,
        name: String,
    },
}

fn default_version() -> u32 { 1 }

/// Vessel → Client
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutgoingMessage {
    Event {
        module: &'static str,
        name: String,
        version: u32,
        data: Value,
        timestamp: u64,
    },
    Response {
        request_id: String,
        success: bool,
        data: Value,
    },
}

impl From<ModuleEvent> for OutgoingMessage {
    fn from(event: ModuleEvent) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        OutgoingMessage::Event {
            module: event.source(),
            name: event.event_name().to_owned(),
            version: 1,
            data: event.data().clone(),
            timestamp,
        }
    }
}
