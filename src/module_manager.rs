use crate::module::{EventPublisher, Module, ModuleCommand, ModuleContext, ModuleEvent};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, info_span, warn, Instrument};

pub struct ModuleManager {
    senders: HashMap<&'static str, mpsc::Sender<ModuleCommand>>,
    modules: HashMap<&'static str, (Box<dyn Module>, mpsc::Receiver<ModuleCommand>)>,
    event_publisher: EventPublisher,
    pub assets: Arc<DashMap<String, (Vec<u8>, String)>>,
}

impl ModuleManager {
    pub fn new() -> Self {
        ModuleManager {
            event_publisher: EventPublisher::new(),
            senders: HashMap::new(),
            modules: HashMap::new(),
            assets: Arc::new(DashMap::new()),
        }
    }

    pub fn register_module(&mut self, module: Box<dyn Module>) {
        let (tx, rx) = mpsc::channel(32);
        let name = module.name();
        self.senders.insert(name, tx);
        self.modules.insert(name, (module, rx));
        info!(name, "module registered");
    }

    pub async fn send_command(
        &self,
        command: ModuleCommand,
    ) -> Result<(), mpsc::error::SendError<ModuleCommand>> {
        if let Some(tx) = self.senders.get(command.target.as_str()) {
            tx.send(command).await
        } else {
            warn!(name = %command.target, "module not found");
            Ok(())
        }
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ModuleEvent> {
        self.event_publisher.subscribe()
    }

    pub fn snapshot(&self) -> Vec<ModuleEvent> {
        self.event_publisher.snapshot()
    }

    pub async fn route_command(
        &self,
        module: &str,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<()> {
        let command: ModuleCommand = ModuleCommand {
            target: module.to_owned(),
            action,
            params,
        };
        self.send_command(command).await?;
        Ok(())
    }

    pub async fn run_all(&mut self, cancel_token: CancellationToken) -> anyhow::Result<()> {
        for (_, (module, rx)) in self.modules.drain() {
            let name = module.name();
            let ctx = ModuleContext::new(
                cancel_token.clone(),
                rx,
                self.event_publisher.clone(),
                self.assets.clone(),
            );
            tokio::spawn(
                async move {
                    if let Err(e) = module.run(ctx).await {
                        error!("module error: {e:#}");
                    }
                }
                .instrument(info_span!("module", name)),
            );
        }
        Ok(())
    }
}
