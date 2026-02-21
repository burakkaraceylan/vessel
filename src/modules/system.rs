use async_trait::async_trait;

use crate::module::{FromModuleCommand, Module};
use commands::SystemCommand;

pub mod commands;
pub mod events;
pub mod keyboard;
pub mod window;

pub struct SystemModule;

#[async_trait]
impl Module for SystemModule {
    async fn new(_config: toml::Table) -> anyhow::Result<Self> {
        Ok(SystemModule)
    }

    fn name(&self) -> &'static str {
        "system"
    }

    async fn run(&self, ctx: crate::module::ModuleContext) -> anyhow::Result<()> {
        let mut window_module = window::WindowModule::new(ctx.event_tx.clone());
        let mut rx = ctx.rx;

        let window_fut = window_module.run();
        tokio::pin!(window_fut);

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => break,
                _ = &mut window_fut => break,
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    match SystemCommand::from_command(&cmd.action, &cmd.params) {
                        Ok(SystemCommand::SendKeys(chord)) => {
                            if let Err(e) = keyboard::send_keys(&chord) {
                                tracing::error!("send_keys failed: {e}");
                            }
                        }
                        Ok(SystemCommand::SpawnExe { exe, args }) => {
                            if let Err(e) = tokio::process::Command::new(&exe).args(&args).spawn() {
                                tracing::error!("spawn_exe failed for '{exe}': {e}");
                            }
                        }
                        Ok(SystemCommand::OpenUri(uri)) => {
                            if let Err(e) = tokio::process::Command::new("cmd")
                                .args(["/c", "start", "", &uri])
                                .spawn()
                            {
                                tracing::error!("open_uri failed for '{uri}': {e}");
                            }
                        }
                        Err(e) => tracing::warn!("unknown system command: {e}"),
                    }
                }
            }
        }

        Ok(())
    }
}
