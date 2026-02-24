mod api;
mod config;
mod dashboard;
mod module;
mod module_manager;
mod modules;
mod protocol;
mod vessel;
mod wasm;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::module::Module;
use crate::module_manager::ModuleManager;
use crate::modules::{discord, media};
use crate::vessel::{AppState, build_router};
use crate::wasm::WasmModule;

fn load_wasm_modules(manager: &mut ModuleManager, config: &config::Config) {
    let modules_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("vessel")
        .join("modules");

    let Ok(entries) = std::fs::read_dir(&modules_dir) else {
        return; // No modules directory yet — fine
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("module.wasm").exists() {
            continue;
        }
        let dir_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let module_config = config.modules
            .get(dir_name)
            .cloned()
            .unwrap_or_default();
        match WasmModule::load(path.clone(), module_config) {
            Ok(module) => {
                info!(name = module.name(), "WASM module loaded");
                manager.register_module(Box::new(module));
            }
            Err(e) => {
                error!(path = %path.display(), "failed to load WASM module: {e:#}");
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let token = CancellationToken::new();
    let config = config::Config::load()?;

    let dashboard_store = Arc::new(dashboard::DashboardStore::new());
    dashboard_store.load_dashboards()?;

    let mut module_manager = ModuleManager::new();

    match config.modules.get("discord") {
        Some(discord_config) => {
            match discord::DiscordModule::new(discord_config.to_owned()).await {
                Ok(m) => { module_manager.register_module(Box::new(m)); }
                Err(e) => { error!("discord module failed to initialize: {e:#}"); }
            }
        }
        None => warn!("discord module config missing, skipping"),
    }

    match media::MediaModule::new(toml::Table::new()).await {
        Ok(m) => { module_manager.register_module(Box::new(m)); }
        Err(e) => { error!("media module failed to initialize: {e:#}"); }
    }

    match modules::system::SystemModule::new(toml::Table::new()).await {
        Ok(m) => { module_manager.register_module(Box::new(m)); }
        Err(e) => { error!("system module failed to initialize: {e:#}"); }
    }

    load_wasm_modules(&mut module_manager, &config);

    module_manager.run_all(token.clone()).await?;

    let assets = module_manager.assets.clone();
    let state = Arc::new(AppState {
        module_manager,
        assets,
        dashboard_store,
        cancel_token: token.clone(),
    });

    let cancel_token = token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Ctrl+C received, shutting down");
        cancel_token.cancel();
    });

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config.host, config.port)).await?;
    info!(host = %config.host, port = config.port, "server listening");
    axum::serve(listener, build_router(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(token.cancelled_owned())
        .await?;

    Ok(())
}
