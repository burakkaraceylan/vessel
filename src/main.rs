mod api;
mod config;
mod dashboard;
mod module;
mod module_manager;
mod modules;
mod protocol;
mod vessel;
mod wasm;

use std::sync::Arc;

use crate::module::Module;
use crate::module_manager::ModuleManager;
use crate::modules::{discord, media};
use crate::vessel::{AppState, build_router};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = CancellationToken::new();
    let config = config::Config::load()?;

    let dashboard_store = Arc::new(dashboard::DashboardStore::new());
    dashboard_store.load_dashboards()?;

    let mut module_manager = ModuleManager::new();

    match config.modules.get("discord") {
        Some(discord_config) => {
            match discord::DiscordModule::new(discord_config.to_owned()).await {
                Ok(m) => { module_manager.register_module(Box::new(m)); }
                Err(e) => { eprintln!("[vessel] discord module failed to initialize: {e:#}"); }
            }
        }
        None => eprintln!("[vessel] discord module config missing, skipping"),
    }

    match media::MediaModule::new(toml::Table::new()).await {
        Ok(m) => { module_manager.register_module(Box::new(m)); }
        Err(e) => { eprintln!("[vessel] media module failed to initialize: {e:#}"); }
    }

    match modules::system::SystemModule::new(toml::Table::new()).await {
        Ok(m) => { module_manager.register_module(Box::new(m)); }
        Err(e) => { eprintln!("[vessel] system module failed to initialize: {e:#}"); }
    }

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
        println!("Ctrl+C received, shutting down...");
        cancel_token.cancel();
    });

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config.host, config.port)).await?;
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(token.cancelled_owned())
        .await?;

    Ok(())
}
