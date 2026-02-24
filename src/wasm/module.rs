use crate::module::{Module, ModuleContext};
use crate::wasm::capability::CapabilityValidator;
use crate::wasm::host::{HostData, VesselModule};
use crate::wasm::manifest::{load_manifest, ModuleManifest};
use anyhow::Context;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use wasmtime::component::Component;
use wasmtime::{Config, Engine, Store};

pub struct WasmModule {
    manifest: ModuleManifest,
    wasm_path: PathBuf,
    engine: Engine,
    /// Leaked once at load time — reused by both `name()` and `run()`.
    name_static: &'static str,
    config: std::collections::HashMap<String, String>,
}

impl WasmModule {
    pub fn load(module_dir: PathBuf, config: toml::Table) -> anyhow::Result<Self> {
        let manifest = load_manifest(&module_dir)?;
        let wasm_path = module_dir.join("module.wasm");
        let name_static: &'static str = Box::leak(manifest.id.clone().into_boxed_str());

        let mut wasmtime_config = Config::new();
        wasmtime_config.async_support(true);
        wasmtime_config.wasm_component_model(true);
        let engine = Engine::new(&wasmtime_config)?;

        let config_map = toml_to_string_map(&config);

        Ok(WasmModule { manifest, wasm_path, engine, name_static, config: config_map })
    }
}

#[async_trait]
impl Module for WasmModule {
    async fn new(_config: toml::Table) -> anyhow::Result<Self> {
        anyhow::bail!("Use WasmModule::load() to construct WASM modules")
    }

    fn name(&self) -> &'static str {
        self.name_static
    }

    async fn run(&self, ctx: ModuleContext) -> anyhow::Result<()> {
        let capability = Arc::new(CapabilityValidator::from_permissions(&self.manifest.permissions));

        // Channels for timer and websocket callbacks back into the run loop
        let (timer_tx, mut timer_rx) = mpsc::channel::<u32>(32);
        let (ws_tx, mut ws_rx) = mpsc::channel::<(u32, String)>(32);

        // Reuse the &'static str computed once at load time.
        let module_id_static: &'static str = self.name_static;

        // Storage directory: %APPDATA%/Local/vessel/modules/<id>/storage/
        let storage_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("vessel")
            .join("modules")
            .join(&self.manifest.id)
            .join("storage");
        std::fs::create_dir_all(&storage_dir)?;

        let host_data = HostData {
            module_id: self.manifest.id.clone(),
            module_id_static,
            capability: capability.clone(),
            event_publisher: ctx.event_tx.clone(),
            timer_tx,
            ws_tx,
            subscriptions: Vec::new(),
            storage_dir,
            config: self.config.clone(),
            timer_handles: std::collections::HashMap::new(),
            ws_handles: std::collections::HashMap::new(),
            next_handle: 1,
        };

        // ── Instantiate the WASM component ────────────────────────────────
        let wasm_bytes = std::fs::read(&self.wasm_path)
            .with_context(|| format!("reading {}", self.wasm_path.display()))?;
        let component = Component::from_binary(&self.engine, &wasm_bytes)?;

        let mut linker = wasmtime::component::Linker::new(&self.engine);
        VesselModule::add_to_linker::<HostData, wasmtime::component::HasSelf<HostData>>(&mut linker, |x| x)?;

        let mut store = Store::new(&self.engine, host_data);
        let bindings = VesselModule::instantiate_async(&mut store, &component, &linker).await?;

        // ── on_load ───────────────────────────────────────────────────────
        match bindings.vessel_host_guest().call_on_load(&mut store)? {
            Ok(()) => {}
            Err(msg) => {
                eprintln!("[{}] on_load failed: {}", self.manifest.id, msg);
                return Ok(());
            }
        }

        // ── Subscribe to the event bus ────────────────────────────────────
        let mut event_rx = ctx.event_tx.subscribe();
        let mut command_rx = ctx.rx;

        // ── Main dispatch loop ─────────────────────────────────────────────
        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => break,

                Some(cmd) = command_rx.recv() => {
                    let params_json = serde_json::to_string(&cmd.params).unwrap_or_default();
                    match bindings.vessel_host_guest()
                        .call_on_command(&mut store, &cmd.action, &params_json)
                    {
                        Ok(Ok(_response)) => {}
                        Ok(Err(e)) => eprintln!("[{}] on_command error: {}", self.manifest.id, e),
                        Err(e) => eprintln!("[{}] on_command trap: {}", self.manifest.id, e),
                    }
                }

                Ok(event) = event_rx.recv() => {
                    let event_key = format!("{}.{}", event.source(), event.event_name());
                    let matches = store.data().subscriptions.iter()
                        .any(|pat| pat.matches(&event_key));
                    if matches {
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let wit_event = crate::wasm::host::vessel::host::types::Event {
                            module: event.source().to_string(),
                            name: event.event_name().to_string(),
                            version: 1,
                            data: serde_json::to_string(event.data()).unwrap_or_default(),
                            timestamp: ts,
                        };
                        let _ = bindings.vessel_host_guest()
                            .call_on_event(&mut store, &wit_event);
                    }
                }

                Some(handle) = timer_rx.recv() => {
                    let _ = bindings.vessel_host_guest()
                        .call_on_timer(&mut store, handle);
                }

                Some((handle, message)) = ws_rx.recv() => {
                    let _ = bindings.vessel_host_guest()
                        .call_on_websocket_message(&mut store, handle, &message);
                }
            }
        }

        // ── on_unload ──────────────────────────────────────────────────────
        let _ = bindings.vessel_host_guest().call_on_unload(&mut store);
        Ok(())
    }
}

fn toml_to_string_map(table: &toml::Table) -> std::collections::HashMap<String, String> {
    table.iter().map(|(k, v)| {
        let s = match v {
            toml::Value::String(s) => s.clone(),
            toml::Value::Integer(i) => i.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::Boolean(b) => b.to_string(),
            other => other.to_string(),
        };
        (k.clone(), s)
    }).collect()
}
