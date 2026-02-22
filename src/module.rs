use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait Module: Send + Sync {
    async fn new(config: toml::Table) -> anyhow::Result<Self, anyhow::Error>
    where
        Self: Sized;
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: ModuleContext) -> anyhow::Result<(), anyhow::Error>;
}

pub struct ModuleContext {
    pub cancel_token: CancellationToken,
    pub rx: mpsc::Receiver<ModuleCommand>,
    pub event_tx: EventPublisher,
    pub assets: Arc<DashMap<String, (Vec<u8>, String)>>,
}

impl ModuleContext {
    pub fn new(
        cancel_token: CancellationToken,
        rx: mpsc::Receiver<ModuleCommand>,
        event_tx: EventPublisher,
        assets: Arc<DashMap<String, (Vec<u8>, String)>>,
    ) -> Self {
        ModuleContext {
            cancel_token,
            rx,
            event_tx,
            assets,
        }
    }
}

pub struct ModuleCommand {
    pub target: String,
    pub action: String,
    pub params: serde_json::Value,
}

pub trait FromModuleCommand: Sized {
    fn from_command(
        action: &str,
        params: &serde_json::Value,
    ) -> anyhow::Result<Self, anyhow::Error>;
}

pub trait IntoModuleEvent {
    fn into_event(self) -> ModuleEvent;
}

#[derive(Clone)]
pub enum ModuleEvent {
    /// Persisted in the state cache. `cache_key` determines the cache slot —
    /// events with the same key are mutually exclusive and overwrite each other.
    /// Use a shared key for events that represent alternative states of the same
    /// thing (e.g. TrackChanged and PlaybackStopped both map to "media/now_playing").
    Stateful {
        source: &'static str,
        event: String,
        data: serde_json::Value,
        cache_key: &'static str,
    },
    /// Not persisted. Use for point-in-time notifications that have no lasting state.
    Transient {
        source: &'static str,
        event: String,
        data: serde_json::Value,
    },
}

impl ModuleEvent {
    pub fn source(&self) -> &'static str {
        match self {
            Self::Stateful  { source, .. } | Self::Transient { source, .. } => source,
        }
    }

    pub fn event_name(&self) -> &str {
        match self {
            Self::Stateful  { event, .. } | Self::Transient { event, .. } => event,
        }
    }

    pub fn data(&self) -> &serde_json::Value {
        match self {
            Self::Stateful  { data, .. } | Self::Transient { data, .. } => data,
        }
    }
}

#[derive(Clone)]
pub struct EventPublisher {
    tx: broadcast::Sender<ModuleEvent>,
    cache: Arc<DashMap<String, ModuleEvent>>,
}

impl EventPublisher {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(32);
        Self {
            tx,
            cache: Arc::new(DashMap::new()),
        }
    }

    pub fn send(&self, event: ModuleEvent) {
        if let ModuleEvent::Stateful { cache_key, .. } = &event {
            self.cache.insert(cache_key.to_string(), event.clone());
        }
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ModuleEvent> {
        self.tx.subscribe()
    }

    pub fn snapshot(&self) -> Vec<ModuleEvent> {
        self.cache.iter().map(|e| e.value().clone()).collect()
    }
}
