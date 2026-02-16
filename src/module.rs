use async_trait::async_trait;
use tokio::sync::mpsc;
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
    pub event_tx: mpsc::Sender<ModuleEvent>,
}

impl ModuleContext {
    pub fn new(
        cancel_token: CancellationToken,
        rx: mpsc::Receiver<ModuleCommand>,
        event_tx: mpsc::Sender<ModuleEvent>,
    ) -> Self {
        ModuleContext {
            cancel_token,
            rx,
            event_tx,
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

pub struct ModuleEvent {
    pub source: &'static str,
    pub event: String,
    pub data: serde_json::Value,
}
