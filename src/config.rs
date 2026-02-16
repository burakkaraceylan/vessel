use anyhow::Context;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub port: u16,
    pub host: String,
    pub modules: HashMap<String, toml::Table>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_str =
            std::fs::read_to_string("config.toml").context("failed to read conf.toml")?;
        let config: Config = toml::from_str(&config_str).context("failed to parse config.toml")?;
        Ok(config)
    }
}
