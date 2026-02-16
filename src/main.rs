mod config;
mod module;
mod module_manager;
mod modules;
mod protocol;
mod vessel;

use crate::module::Module;
use crate::modules::discord;
use crate::vessel::Vessel;
use anyhow::Context;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = CancellationToken::new();
    let config = config::Config::load()?;
    let mut vessel = Vessel::new(&config).await?;
    let discord_config = config
        .modules
        .get("discord")
        .context("Discord module config missing")?
        .to_owned();
    let discord_module = discord::DiscordModule::new(discord_config).await?;
    vessel.module_manager.register_module(discord_module);

    let cancel_token = token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        println!("Ctrl+C received, shutting down...");
        cancel_token.cancel();
    });

    vessel.run(token).await?;

    Ok(())
}
