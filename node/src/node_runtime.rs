use anyhow::{Context, Result};
use tokio::{
    sync::{broadcast, mpsc, oneshot, watch},
    time::{Duration, sleep},
};

use crate::{
    node_shell::NodeConfig,
    peer_server::{self, PeerServerConfig},
    signal::BroadcastSignal,
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

    let (broadcast_tx, _broadcast_rx) = broadcast::channel::<BroadcastSignal>(32);

    let peer_config = PeerServerConfig {
        host: config.peer_host.clone(),
        port: config.peer_port,
        share_dir: config.share_dir.clone(),
        block_size: config.block_size,
    };

    let mut peer_handle = tokio::spawn(peer_server::run(
        peer_config,
        shutdown_rx.clone(),
    ));

    let mut transfer_handle = tokio::spawn(transfer_worker::run(
        config.clone(),
        worker_rx,
        shutdown_rx.clone(),
        broadcast_tx.clone(),
    ));

    let mut tui_handle = tokio::spawn(crate::tui::init_tui(
        config.clone(),
        broadcast_tx.clone(),
        command_tx.clone(),
    ));

    let mut tui_rx = broadcast_tx.subscribe();
    let command_tx_clone = command_tx.clone();
    tokio::spawn(async move {
        while let Ok(signal) = tui_rx.recv().await {
            if matches!(signal, BroadcastSignal::Shutdown) {
                let _ = command_tx_clone.send(RuntimeCommand::Shutdown);
                break;
            }
        }
    });

    let completed_task = tokio::select! {
        task = command_loop(&mut command_rx, &worker_tx, &shutdown_tx) => {
            task?;
            "command"
        }
        res = &mut peer_handle => {
            request_shutdown(&shutdown_tx);
            res.context("peer server task panicked")??;
            "peer server"
        }
        res = &mut transfer_handle => {
            request_shutdown(&shutdown_tx);
            res.context("transfer worker task panicked")??;
            "transfer worker"
        }
        res = &mut tui_handle => {
            request_shutdown(&shutdown_tx);
            res.context("TUI task panicked")?;
            "TUI"
        }
    };

    // 清理其他后台任务，移除了旧的 shell_handle
    if completed_task != "peer server" {
        await_task(peer_handle, "peer server").await?;
    }
    if completed_task != "transfer worker" {
        await_task(transfer_handle, "transfer worker").await?;
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

async fn await_task<T>(
    mut handle: tokio::task::JoinHandle<T>,
    task_name: &'static str,
) -> Result<()> {
    tokio::select! {
        res = &mut handle => {
            res.with_context(|| format!("{task_name} task failed to join"))?;
        }
        _ = sleep(Duration::from_secs(2)) => {
            handle.abort();
        }
    }
    Ok(())
}