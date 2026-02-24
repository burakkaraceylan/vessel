wit_bindgen::generate!({
    world: "vessel-module",
    path: "wit",
});

use exports::vessel::host::guest::Guest;
use vessel::host::host::*;
use vessel::host::types::Event;

struct HomeAssistant;

// ── Module-level state ──────────────────────────────────────────────────────
use std::sync::atomic::{AtomicU32, Ordering};

static WS_HANDLE: AtomicU32 = AtomicU32::new(0);
static MSG_ID: AtomicU32 = AtomicU32::new(1);
/// ID of the in-flight get_states request, so we can match the result.
static GET_STATES_ID: AtomicU32 = AtomicU32::new(0);

fn next_id() -> u32 {
    MSG_ID.fetch_add(1, Ordering::Relaxed)
}

/// Emit a single HA entity state as a stateful Vessel event.
/// event name = entity_id, cache key = entity_id
/// data = { "state": "...", "attributes": { ... } }
fn emit_entity_state(state: &serde_json::Value) -> Result<(), String> {
    let entity_id = match state.get("entity_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_owned(),
        None => return Ok(()),
    };
    let data = serde_json::json!({
        "state": state.get("state").cloned().unwrap_or(serde_json::Value::Null),
        "attributes": state.get("attributes").cloned().unwrap_or(serde_json::json!({})),
    });
    let event = Event {
        module: "home-assistant".to_owned(),
        name: entity_id.clone(),
        version: 1,
        data: data.to_string(),
        timestamp: 0,
    };
    emit_stateful(&event, &entity_id)
}

impl Guest for HomeAssistant {
    fn on_load() -> Result<(), String> {
        log("info", "Home Assistant: loading");

        let ha_url = config_get("url")
            .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_owned());

        let _ = subscribe("system.window.*");

        let handle = websocket_connect(&ha_url)?;
        WS_HANDLE.store(handle, Ordering::Relaxed);

        Ok(())
    }

    fn on_unload() -> Result<(), String> {
        log("info", "Home Assistant: unloading");
        Ok(())
    }

    fn on_event(_event: Event) -> Result<(), String> {
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
                Ok(r#"{"queued":true}"#.to_owned())
            }
            _ => Err(format!("unknown action: {}", action)),
        }
    }

    fn on_timer(_handle: u32) -> Result<(), String> {
        log("info", "Home Assistant: attempting reconnect");
        let ha_url = config_get("url")
            .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_owned());
        if let Ok(new_handle) = websocket_connect(&ha_url) {
            WS_HANDLE.store(new_handle, Ordering::Relaxed);
        }
        Ok(())
    }

    fn on_websocket_message(_handle: u32, message: String) -> Result<(), String> {
        let msg: serde_json::Value = match serde_json::from_str(&message) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        match msg.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "auth_required" => {
                let token = config_get("token").unwrap_or_default();
                let auth = serde_json::json!({ "type": "auth", "access_token": token });
                websocket_send(WS_HANDLE.load(Ordering::Relaxed), &auth.to_string())?;
            }

            "auth_ok" => {
                log("info", "Home Assistant: authenticated");

                // Fetch all current entity states for the snapshot cache.
                let states_id = next_id();
                GET_STATES_ID.store(states_id, Ordering::Relaxed);
                let get_states = serde_json::json!({
                    "id": states_id,
                    "type": "get_states"
                });
                websocket_send(WS_HANDLE.load(Ordering::Relaxed), &get_states.to_string())?;

                // Subscribe to ongoing state changes.
                let sub_id = next_id();
                let sub = serde_json::json!({
                    "id": sub_id,
                    "type": "subscribe_events",
                    "event_type": "state_changed"
                });
                websocket_send(WS_HANDLE.load(Ordering::Relaxed), &sub.to_string())?;
            }

            "auth_invalid" => {
                log("error", "Home Assistant: authentication failed — check 'token' under [modules.home-assistant] in config.toml");
            }

            "result" => {
                let msg_id = msg.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if msg_id != GET_STATES_ID.load(Ordering::Relaxed) {
                    return Ok(());
                }
                if msg.get("success").and_then(|v| v.as_bool()) != Some(true) {
                    log("warn", "Home Assistant: get_states failed");
                    return Ok(());
                }
                let states = match msg.get("result").and_then(|v| v.as_array()) {
                    Some(s) => s.clone(),
                    None => return Ok(()),
                };
                for state in &states {
                    let _ = emit_entity_state(state);
                }
            }

            "event" => {
                if let Some(event_data) = msg.get("event") {
                    if event_data.get("event_type").and_then(|t| t.as_str())
                        == Some("state_changed")
                    {
                        if let Some(new_state) = event_data
                            .get("data")
                            .and_then(|d| d.get("new_state"))
                        {
                            let _ = emit_entity_state(new_state);
                        }
                    }
                }
            }

            _ => {}
        }

        Ok(())
    }
}

export!(HomeAssistant);
