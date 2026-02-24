wit_bindgen::generate!({
    world: "vessel-module",
    path: "wit",
});

use exports::vessel::host::guest::Guest;
use vessel::host::host::*;
use vessel::host::types::Event;

struct HomeAssistant;

// ── Module-level state ──────────────────────────────────────────────────────
// WASM is single-threaded, but the Rust compiler still requires statics to be
// Sync. AtomicU32 satisfies Sync and is the natural fit for u32 counters/handles.

use std::sync::atomic::{AtomicU32, Ordering};

static WS_HANDLE: AtomicU32 = AtomicU32::new(0);
static MSG_ID: AtomicU32 = AtomicU32::new(1);

fn next_id() -> u32 {
    MSG_ID.fetch_add(1, Ordering::Relaxed)
}

impl Guest for HomeAssistant {
    fn on_load() -> Result<(), String> {
        log("info", "Home Assistant: loading");

        let ha_url = storage_get("url")
            .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_string());

        // Subscribe to system window events (optional — could pause when idle)
        let _ = subscribe("system.window.*");

        // Connect — authentication happens in on_websocket_message when auth_required arrives
        let handle = websocket_connect(&ha_url)?;
        WS_HANDLE.store(handle, Ordering::Relaxed);

        Ok(())
    }

    fn on_unload() -> Result<(), String> {
        log("info", "Home Assistant: unloading");
        Ok(())
    }

    fn on_event(_event: Event) -> Result<(), String> {
        // React to system window events if needed (e.g. pause polling when idle)
        Ok(())
    }

    fn on_command(action: String, params: String) -> Result<String, String> {
        let params: serde_json::Value = serde_json::from_str(&params)
            .unwrap_or(serde_json::Value::Null);

        match action.as_str() {
            "call_service" => {
                let id = next_id();
                let msg = serde_json::json!({
                    "id": id,
                    "type": "call_service",
                    "domain": params.get("domain").and_then(|v| v.as_str()).unwrap_or(""),
                    "service": params.get("service").and_then(|v| v.as_str()).unwrap_or(""),
                    "service_data": params.get("service_data").cloned().unwrap_or(serde_json::json!({})),
                });
                websocket_send(WS_HANDLE.load(Ordering::Relaxed), &msg.to_string())?;
                Ok(r#"{"queued":true}"#.to_string())
            }
            _ => Err(format!("unknown action: {}", action)),
        }
    }

    fn on_timer(_handle: u32) -> Result<(), String> {
        // Reconnect timer
        log("info", "Home Assistant: attempting reconnect");
        let ha_url = storage_get("url")
            .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_string());
        if let Ok(new_handle) = websocket_connect(&ha_url) {
            WS_HANDLE.store(new_handle, Ordering::Relaxed);
        }
        Ok(())
    }

    fn on_websocket_message(_handle: u32, message: String) -> Result<(), String> {
        // Parse the HA message type
        let msg: serde_json::Value = match serde_json::from_str(&message) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        match msg.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "auth_required" => {
                let token = storage_get("token").unwrap_or_default();
                let auth = serde_json::json!({ "type": "auth", "access_token": token });
                websocket_send(WS_HANDLE.load(Ordering::Relaxed), &auth.to_string())?;
            }

            "auth_ok" => {
                log("info", "Home Assistant: authenticated");
                let id = next_id();
                let sub = serde_json::json!({
                    "id": id,
                    "type": "subscribe_events",
                    "event_type": "state_changed"
                });
                websocket_send(WS_HANDLE.load(Ordering::Relaxed), &sub.to_string())?;
            }

            "auth_invalid" => {
                log("error", "Home Assistant: authentication failed — set token via storage");
            }

            "event" => {
                if let Some(event_data) = msg.get("event") {
                    if event_data.get("event_type").and_then(|t| t.as_str())
                        == Some("state_changed")
                    {
                        let data = event_data
                            .get("data")
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        let vessel_event = Event {
                            module: "home-assistant".to_string(),
                            name: "state_changed".to_string(),
                            version: 1,
                            data: data.to_string(),
                            timestamp: 0,
                        };
                        let _ = emit(&vessel_event);
                    }
                }
            }

            _ => {} // ignore result, ping, pong, etc.
        }

        Ok(())
    }
}

export!(HomeAssistant);
