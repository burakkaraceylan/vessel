use crate::module::{EventPublisher, ModuleEvent};
use crate::wasm::capability::CapabilityValidator;
use glob::Pattern;
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
    /// Pre-compiled glob patterns from `subscribe()` calls — avoids recompiling on every event.
    pub subscriptions: Vec<Pattern>,
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
        // Pattern is valid (capability check uses Pattern::new internally), so unwrap is safe.
        let compiled = Pattern::new(&pattern).map_err(|e| e.to_string())?;
        self.subscriptions.push(compiled);
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

    async fn storage_get(&mut self, key: String) -> Option<String> {
        if self.capability.check_storage().is_err() {
            return None;
        }
        let path = self.storage_dir.join(sanitize_key(&key));
        std::fs::read_to_string(path).ok()
    }

    async fn storage_set(&mut self, key: String, value: String) -> Result<(), String> {
        if let Err(e) = self.capability.check_storage() {
            return Err(e.to_string());
        }
        let path = self.storage_dir.join(sanitize_key(&key));
        std::fs::write(path, value).map_err(|e| e.to_string())
    }

    async fn storage_delete(&mut self, key: String) -> Result<(), String> {
        if let Err(e) = self.capability.check_storage() {
            return Err(e.to_string());
        }
        let path = self.storage_dir.join(sanitize_key(&key));
        let _ = std::fs::remove_file(path); // Ignore not-found
        Ok(())
    }

    async fn set_timeout(&mut self, ms: u64) -> u32 {
        if self.capability.check_timers().is_err() {
            return 0;
        }
        let handle = self.next_handle;
        self.next_handle += 1;
        let tx = self.timer_tx.clone();
        let join = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
            let _ = tx.send(handle).await;
        });
        self.timer_handles.insert(handle, join);
        handle
    }

    async fn set_interval(&mut self, ms: u64) -> u32 {
        if self.capability.check_timers().is_err() {
            return 0;
        }
        let handle = self.next_handle;
        self.next_handle += 1;
        let tx = self.timer_tx.clone();
        let join = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(ms));
            interval.tick().await; // skip the immediate first tick
            loop {
                interval.tick().await;
                if tx.send(handle).await.is_err() {
                    break;
                }
            }
        });
        self.timer_handles.insert(handle, join);
        handle
    }

    async fn clear_timer(&mut self, handle: u32) {
        if let Some(join) = self.timer_handles.remove(&handle) {
            join.abort();
        }
    }

    async fn log(&mut self, level: String, message: String) {
        println!("[{}] [{}] {}", level.to_uppercase(), self.module_id, message);
    }
}

/// Converts a storage key to a safe filename — replaces non-alphanumeric chars with underscores.
fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}
