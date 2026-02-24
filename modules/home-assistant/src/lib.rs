wit_bindgen::generate!({
    world: "vessel-module",
    path: "wit",
});

use exports::vessel::host::guest::Guest;
use vessel::host::host::*;
use vessel::host::types::Event;

struct HomeAssistant;

impl Guest for HomeAssistant {
    fn on_load() -> Result<(), String> {
        log("info", "Home Assistant: loading");
        Ok(())
    }

    fn on_unload() -> Result<(), String> {
        log("info", "Home Assistant: unloading");
        Ok(())
    }

    fn on_event(_event: Event) -> Result<(), String> {
        Ok(())
    }

    fn on_command(_action: String, _params: String) -> Result<String, String> {
        Ok("{}".to_string())
    }

    fn on_timer(_handle: u32) -> Result<(), String> {
        Ok(())
    }

    fn on_websocket_message(_handle: u32, _message: String) -> Result<(), String> {
        Ok(())
    }
}

export!(HomeAssistant);
