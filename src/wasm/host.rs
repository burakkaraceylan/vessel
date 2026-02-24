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
    pub config: std::collections::HashMap<String, String>,
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
        module: String,
        name: String,
        version: u32,
        _params: String,
    ) -> Result<String, String> {
        // Capability check runs even though routing is not yet implemented — this
        // ensures the enforcement path is exercised and denials are returned correctly.
        if let Err(e) = self.capability.check_call(&module, &name, version) {
            return Err(e.to_string());
        }
        Err("driver call routing not yet implemented".into())
    }

    async fn send_http_request(
        &mut self,
        req: vessel::host::types::HttpRequest,
    ) -> Result<vessel::host::types::HttpResponse, String> {
        if let Err(e) = self.capability.check_network_http() {
            return Err(e.to_string());
        }

        let client = reqwest::Client::new();
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| e.to_string())?;

        let mut builder = client.request(method, &req.url);
        for (key, value) in &req.headers {
            builder = builder.header(key.as_str(), value.as_str());
        }
        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        match builder.send().await {
            Ok(response) => {
                let status = response.status().as_u16() as u32;
                let headers: Vec<(String, String)> = response
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = response.text().await.unwrap_or_default();
                Ok(vessel::host::types::HttpResponse { status, headers, body })
            }
            Err(e) => Err(e.to_string()),
        }
    }

    async fn websocket_connect(&mut self, url: String) -> Result<u32, String> {
        if let Err(e) = self.capability.check_network_websocket() {
            return Err(e.to_string());
        }

        use futures_util::StreamExt;
        use tokio_tungstenite::connect_async;

        let handle = self.next_handle;
        self.next_handle += 1;

        let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::channel::<String>(32);
        let inbound_tx = self.ws_tx.clone();
        let module_id = self.module_id.clone();

        tokio::spawn(async move {
            let ws_stream = match connect_async(&url).await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    eprintln!("[{}] WS connect failed: {}", module_id, e);
                    return;
                }
            };
            let (mut write, mut read) = ws_stream.split();

            loop {
                tokio::select! {
                    Some(msg) = outbound_rx.recv() => {
                        use tokio_tungstenite::tungstenite::Message;
                        use futures_util::SinkExt;
                        let _ = write.send(Message::text(msg)).await;
                    }
                    Some(Ok(msg)) = read.next() => {
                        use tokio_tungstenite::tungstenite::Message;
                        if let Message::Text(text) = msg {
                            let _ = inbound_tx.send((handle, text.as_str().to_owned())).await;
                        }
                    }
                    else => break,
                }
            }
        });

        self.ws_handles.insert(handle, outbound_tx);
        Ok(handle)
    }

    async fn websocket_send(&mut self, handle: u32, message: String) -> Result<(), String> {
        match self.ws_handles.get(&handle) {
            Some(tx) => tx.send(message).await.map_err(|e| e.to_string()),
            None => Err(format!("unknown websocket handle {}", handle)),
        }
    }

    async fn websocket_close(&mut self, handle: u32) -> Result<(), String> {
        self.ws_handles.remove(&handle);
        Ok(())
    }

    async fn config_get(&mut self, key: String) -> Option<String> {
        self.config.get(&key).cloned()
    }

    async fn storage_get(&mut self, key: String) -> Option<String> {
        if self.capability.check_storage().is_err() {
            return None;
        }
        let sanitized = sanitize_key(&key);
        if sanitized.is_empty() {
            return None;
        }
        let path = self.storage_dir.join(sanitized);
        tokio::fs::read_to_string(path).await.ok()
    }

    async fn storage_set(&mut self, key: String, value: String) -> Result<(), String> {
        if let Err(e) = self.capability.check_storage() {
            return Err(e.to_string());
        }
        let sanitized = sanitize_key(&key);
        if sanitized.is_empty() {
            return Err("storage key must not be empty".into());
        }
        let path = self.storage_dir.join(sanitized);
        tokio::fs::write(path, value).await.map_err(|e| e.to_string())
    }

    async fn storage_delete(&mut self, key: String) -> Result<(), String> {
        if let Err(e) = self.capability.check_storage() {
            return Err(e.to_string());
        }
        let sanitized = sanitize_key(&key);
        if sanitized.is_empty() {
            return Err("storage key must not be empty".into());
        }
        let path = self.storage_dir.join(sanitized);
        let _ = tokio::fs::remove_file(path).await; // Ignore not-found
        Ok(())
    }

    async fn set_timeout(&mut self, ms: u64) -> u32 {
        if self.capability.check_timers().is_err() {
            // Returns 0; guest should treat 0 as an invalid handle (timers not permitted).
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
            // Returns 0; guest should treat 0 as an invalid handle (timers not permitted).
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
