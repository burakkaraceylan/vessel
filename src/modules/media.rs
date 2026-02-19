pub mod commands;
pub mod events;
pub mod smtc;

use crate::module::{FromModuleCommand, IntoModuleEvent, Module, ModuleContext};
use async_trait::async_trait;
use commands::MediaCommand;
use smtc::{SmtcCommand, SmtcModule};

pub struct MediaModule;

#[async_trait]
impl Module for MediaModule {
    async fn new(_config: toml::Table) -> anyhow::Result<Self> {
        Ok(MediaModule)
    }

    fn name(&self) -> &'static str {
        "media"
    }

    async fn run(&self, mut ctx: ModuleContext) -> anyhow::Result<()> {
        let mut smtc = SmtcModule::new(ctx.cancel_token.clone(), ctx.assets.clone()).await?;

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => break,

                Some(cmd) = ctx.rx.recv() => {
                    handle_command(cmd, &smtc).await;
                }

                outbound = smtc.event_rx.recv() => {
                    let Some(outbound) = outbound else { break };
                    let _ = ctx.event_tx.send(events::MediaEvent::from(outbound).into_event());
                }
            }
        }

        Ok(())
    }
}

async fn handle_command(cmd: crate::module::ModuleCommand, smtc: &SmtcModule) {
    let media_cmd = match MediaCommand::from_command(&cmd.action, &cmd.params) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Unknown media command '{}': {e}", cmd.action);
            return;
        }
    };

    let smtc_cmd = match media_cmd {
        MediaCommand::Play => SmtcCommand::Play,
        MediaCommand::Pause => SmtcCommand::Pause,
        MediaCommand::TogglePlayPause => SmtcCommand::TogglePlayPause,
        MediaCommand::Stop => SmtcCommand::Stop,
        MediaCommand::Next => SmtcCommand::Next,
        MediaCommand::Previous => SmtcCommand::Previous,
        MediaCommand::SetVolume(_) | MediaCommand::GetStatus => return,
    };

    let _ = smtc.command_tx.send(smtc_cmd).await;
}


