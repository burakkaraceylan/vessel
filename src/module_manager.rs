use crate::module::{Module, ModuleCommand, ModuleContext, ModuleEvent};
use anyhow::Context;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

pub struct ModuleManager {
    senders: HashMap<&'static str, mpsc::Sender<ModuleCommand>>,
    modules: HashMap<&'static str, (Box<dyn Module>, mpsc::Receiver<ModuleCommand>)>,
    event_tx: broadcast::Sender<ModuleEvent>,
}

impl ModuleManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(32);
        ModuleManager {
            event_tx,
            senders: HashMap::new(),
            modules: HashMap::new(),
        }
    }

    pub fn register_module(&mut self, module: Box<dyn Module>) {
        let (tx, rx) = mpsc::channel(32);
        let name = module.name();
        self.senders.insert(name, tx);
        self.modules.insert(name, (module, rx));
    }

    pub async fn send_command(
        &self,
        command: ModuleCommand,
    ) -> Result<(), mpsc::error::SendError<ModuleCommand>> {
        if let Some(tx) = self.senders.get(command.target.as_str()) {
            tx.send(command).await
        } else {
            eprintln!("Module '{}' not found", command.target);
            Ok(())
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ModuleEvent> {
        self.event_tx.subscribe()
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
            let ctx = ModuleContext::new(cancel_token.clone(), rx, self.event_tx.clone());
            tokio::spawn(async move {
                if let Err(e) = module.run(ctx).await {
                    eprintln!("Module error: {}", e);
                }
            });
        }
        Ok(())
    }
}
