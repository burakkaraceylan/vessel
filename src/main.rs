mod config;
mod module;
mod module_manager;
mod modules;
mod protocol;
mod vessel;

use std::sync::Arc;

use crate::module::Module;
use crate::module_manager::ModuleManager;
use crate::modules::discord;
use crate::vessel::{AppState, build_router};
use anyhow::Context;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = CancellationToken::new();
    let config = config::Config::load()?;

    let mut module_manager = ModuleManager::new();
    let discord_config = config
        .modules
        .get("discord")
        .context("Discord module config missing")?
        .to_owned();
    let discord_module = discord::DiscordModule::new(discord_config).await?;
    module_manager.register_module(Box::new(discord_module));
    module_manager.run_all(token.clone()).await?;

    let state = Arc::new(AppState {
        module_manager,
        assets: Arc::new(DashMap::new()),
        cancel_token: token.clone(),
    });

    let cancel_token = token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        println!("Ctrl+C received, shutting down...");
        cancel_token.cancel();
    });

    let listener = tokio::net::TcpListener::bind(format!("{}:8001", config.host)).await?;
    axum::serve(listener, build_router(state)).await?;

    Ok(())
}
