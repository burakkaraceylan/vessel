use crate::wasm::manifest::Permissions;
use glob::Pattern;
use std::collections::HashSet;

#[derive(Debug)]
pub enum CapabilityError {
    Denied(String),
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilityError::Denied(msg) => write!(f, "capability denied: {}", msg),
        }
    }
}

pub struct CapabilityValidator {
    subscribe_patterns: Vec<Pattern>,
    allowed_calls: HashSet<String>,
    pub network_http: bool,
    pub network_websocket: bool,
    pub network_tcp: bool,
    pub storage: bool,
    pub timers: bool,
}

impl CapabilityValidator {
    pub fn from_permissions(perms: &Permissions) -> Self {
        let subscribe_patterns = perms
            .subscribe
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        // Allowed calls stored as "module.name@version" e.g. "discord.voice.set_mute@1"
        let allowed_calls = perms.call.iter().cloned().collect();

        CapabilityValidator {
            subscribe_patterns,
            allowed_calls,
            network_http: perms.network.http,
            network_websocket: perms.network.websocket,
            network_tcp: perms.network.tcp,
            storage: perms.storage,
            timers: perms.timers,
        }
    }

    pub fn check_subscribe(&self, pattern: &str) -> Result<(), CapabilityError> {
        let allowed = self.subscribe_patterns.iter().any(|p| p.matches(pattern));
        if !allowed {
            return Err(CapabilityError::Denied(format!(
                "subscribe '{}' not declared in manifest",
                pattern
            )));
        }
        Ok(())
    }

    pub fn check_call(&self, module: &str, name: &str, version: u32) -> Result<(), CapabilityError> {
        let key = format!("{}.{}@{}", module, name, version);
        if !self.allowed_calls.contains(&key) {
            return Err(CapabilityError::Denied(format!(
                "call '{}.{}@{}' not declared in manifest",
                module, name, version
            )));
        }
        Ok(())
    }

    pub fn check_network_http(&self) -> Result<(), CapabilityError> {
        if !self.network_http {
            return Err(CapabilityError::Denied("network.http not declared".into()));
        }
        Ok(())
    }

    pub fn check_network_websocket(&self) -> Result<(), CapabilityError> {
        if !self.network_websocket {
            return Err(CapabilityError::Denied("network.websocket not declared".into()));
        }
        Ok(())
    }

    pub fn check_storage(&self) -> Result<(), CapabilityError> {
        if !self.storage {
            return Err(CapabilityError::Denied("storage not declared".into()));
        }
        Ok(())
    }

    pub fn check_timers(&self) -> Result<(), CapabilityError> {
        if !self.timers {
            return Err(CapabilityError::Denied("timers not declared".into()));
        }
        Ok(())
    }
}
