use anyhow::{Context, Result};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::{Duration, sleep},
};

use crate::{
    node_shell::{self, NodeConfig},
    peer_server::{self, PeerServerConfig},
    transfer_worker,
};

pub type CommandDone = oneshot::Sender<()>;

pub enum RuntimeCommand {
    Shutdown,
    PublishLocalFiles { done: CommandDone },
    ListTrackerFiles { done: CommandDone },
    Download { target: String, done: CommandDone },
    MarkOffline { done: CommandDone },
}

pub async fn run(config: NodeConfig) -> Result<()> {
    let (command_tx, mut command_rx) = mpsc::unbounded_channel();
    let (worker_tx, worker_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let peer_config = PeerServerConfig {
        host: config.peer_host.clone(),
        port: config.peer_port,
        share_dir: config.share_dir.clone(),
        block_size: config.block_size,
    };

    let mut peer_handle = tokio::spawn(peer_server::run(peer_config, shutdown_rx.clone()));
    let mut transfer_handle =
        tokio::spawn(transfer_worker::run(config.clone(), worker_rx, shutdown_rx));
    let mut shell_handle = tokio::spawn(node_shell::run(config, command_tx));

    let completed_task = tokio::select! {
        task = command_loop(&mut command_rx, &worker_tx, &shutdown_tx) => {
            task?;
            "command"
        }
        result = &mut peer_handle => {
            request_shutdown(&shutdown_tx);
            result.context("peer server task failed to join")??;
            "peer server"
        }
        result = &mut transfer_handle => {
            request_shutdown(&shutdown_tx);
            result.context("transfer worker task failed to join")??;
            "transfer worker"
        }
        result = &mut shell_handle => {
            request_shutdown(&shutdown_tx);
            result.context("CLI task failed to join")??;
            "CLI"
        }
    };

    if completed_task != "peer server" {
        await_task(peer_handle, "peer server").await?;
    }
    if completed_task != "transfer worker" {
        await_task(transfer_handle, "transfer worker").await?;
    }
    if completed_task != "CLI" {
        await_task(shell_handle, "CLI").await?;
    }
    Ok(())
}

async fn command_loop(
    command_rx: &mut mpsc::UnboundedReceiver<RuntimeCommand>,
    worker_tx: &mpsc::UnboundedSender<RuntimeCommand>,
    shutdown_tx: &watch::Sender<bool>,
) -> Result<()> {
    while let Some(command) = command_rx.recv().await {
        match command {
            RuntimeCommand::Shutdown => {
                request_shutdown(shutdown_tx);
                break;
            }
            command => {
                if worker_tx.send(command).is_err() {
                    request_shutdown(shutdown_tx);
                    break;
                }
            }
        }
    }
    request_shutdown(shutdown_tx);
    Ok(())
}

fn request_shutdown(shutdown_tx: &watch::Sender<bool>) {
    let _ = shutdown_tx.send(true);
}

async fn await_task(
    mut handle: tokio::task::JoinHandle<Result<()>>,
    task_name: &'static str,
) -> Result<()> {
    tokio::select! {
        result = &mut handle => {
            result.with_context(|| format!("{task_name} task failed to join"))??;
        }
        _ = sleep(Duration::from_secs(2)) => {
            handle.abort();
        }
    }
    Ok(())
}
