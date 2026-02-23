use crate::module::{EventPublisher, ModuleEvent};
use crate::wasm::capability::CapabilityValidator;
use std::sync::Arc;
use tokio::sync::mpsc;

wasmtime::component::bindgen!({
    world: "vessel-module",
    path: "wit/vessel-host.wit",
    imports: { default: async },
});

pub struct HostData {
    pub module_id: String,
    /// Pre-leaked `&'static str` version of `module_id` for use in `ModuleEvent::source`.
    /// Computed once at construction — avoids leaking a new allocation on every `emit()` call.
    pub module_id_static: &'static str,
    pub capability: Arc<CapabilityValidator>,
    pub event_publisher: EventPublisher,
    pub timer_tx: mpsc::Sender<u32>,
    pub ws_tx: mpsc::Sender<(u32, String)>,
    pub subscriptions: Vec<String>,
    pub storage_dir: std::path::PathBuf,
    pub timer_handles: std::collections::HashMap<u32, tokio::task::JoinHandle<()>>,
    pub ws_handles: std::collections::HashMap<u32, tokio::sync::mpsc::Sender<String>>,
    pub next_handle: u32,
}

// `types::Host` is an empty marker trait — HostData must implement it so
// that `VesselModule::add_to_linker` can satisfy both interface bounds.
impl vessel::host::types::Host for HostData {}

impl vessel::host::host::Host for HostData {
    async fn subscribe(&mut self, pattern: String) -> Result<(), String> {
        if let Err(e) = self.capability.check_subscribe(&pattern) {
            return Err(e.to_string());
        }
        self.subscriptions.push(pattern);
        Ok(())
    }

    async fn emit(&mut self, event: vessel::host::types::Event) -> Result<(), String> {
        let data: serde_json::Value = serde_json::from_str(&event.data)
            .unwrap_or(serde_json::Value::Null);
        self.event_publisher.send(ModuleEvent::Transient {
            source: self.module_id_static,
            event: event.name,
            data,
        });
        Ok(())
    }

    async fn call(
        &mut self,
        _module: String,
        _name: String,
        _version: u32,
        _params: String,
    ) -> Result<String, String> {
        Err("not yet implemented".into())
    }

    async fn send_http_request(
        &mut self,
        _req: vessel::host::types::HttpRequest,
    ) -> Result<vessel::host::types::HttpResponse, String> {
        Err("not yet implemented".into())
    }

    async fn websocket_connect(&mut self, _url: String) -> Result<u32, String> {
        Err("not yet implemented".into())
    }

    async fn websocket_send(&mut self, _handle: u32, _message: String) -> Result<(), String> {
        Err("not yet implemented".into())
    }

    async fn websocket_close(&mut self, _handle: u32) -> Result<(), String> {
        Err("not yet implemented".into())
    }

    async fn storage_get(&mut self, _key: String) -> Option<String> {
        None
    }

    async fn storage_set(&mut self, _key: String, _value: String) -> Result<(), String> {
        Err("not yet implemented".into())
    }

    async fn storage_delete(&mut self, _key: String) -> Result<(), String> {
        Err("not yet implemented".into())
    }

    async fn set_timeout(&mut self, _ms: u64) -> u32 {
        0
    }

    async fn set_interval(&mut self, _ms: u64) -> u32 {
        0
    }

    async fn clear_timer(&mut self, _handle: u32) {
    }

    async fn log(&mut self, level: String, message: String) {
        println!("[{}] [{}] {}", level.to_uppercase(), self.module_id, message);
    }
}
