use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::fs::File;
use tokio::task::yield_now;
use tokio::{signal, time};

use crate::models::PeerFileInfo;
use crate::signal::BroadcastSignal;
use crate::tracker_dto::{PeerUpdateRequest, PeerUpdateResponse};
use crate::{models::FileManifest, tracker_client::TrackerClient};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum BlockStatus {
    Complete,
    Ongoing,
    Pending,
    Delayed
}


#[derive(Debug)]
pub struct DownloadContext {
    pub node_id: String,
    pub broadcast: broadcast::Sender<BroadcastSignal>,
    pub peer_port: RwLock<u16>,
    pub tracker_client: TrackerClient,
    pub manifest: FileManifest,
    pub file: Mutex<File>,
    pub blocks: RwLock<Vec <BlockStatus> >,
    pub peers: RwLock<Vec <PeerFileInfo> >
}
impl DownloadContext {
    pub async fn to_update_payload(&self) -> PeerUpdateRequest {
        let zero_one_str: String = self.blocks.read().await.iter()
            .map(|status| match status {
                BlockStatus::Complete => '1',
                _ => '0'
            })
            .collect();
        PeerUpdateRequest {
            file_hash: self.manifest.file_hash.clone(),
            peer_id: self.node_id.clone(),
            peer_host: None,
            peer_port: *self.peer_port.read().await,
            blocks: zero_one_str,
        }
    }
}

pub async fn update_peer_info_task(context: Arc<DownloadContext>, seconds: u64) {
    let mut interval = time::interval(Duration::from_secs(seconds));

    loop {
        interval.tick().await;
        log::info!(target: "peer_update", "Updateting peer info");

        match context.tracker_client.update_peer(context.to_update_payload().await).await {
            Ok(PeerUpdateResponse::Ok) => {},
            Ok(PeerUpdateResponse::FileNotFound) => {
                log::warn!(target: "peer_update", "Tracker report failed update: File is not present on server")
            }
            Ok(PeerUpdateResponse::Unknown) => {
                log::warn!(target: "peer_update", "Tracker report failed update: Unknown error")
            }
            Err(e) => {
                log::warn!(target: "peer_update", "Failed to update peer info to tracker: {:#}", e)
            }
        }

        let peers = match context.tracker_client.peer_list(&context.manifest.file_hash).await {
            Ok(x) => x,
            Err(e) => {
                log::warn!(target: "peer_update", "Failed to get peer list from tracker: {:#}", e);
                continue;
            }
        };

        log::info!(target: "peer_update", "\n{:#?}", peers);

        let mut guard = context.peers.write().await;
        let _ = std::mem::replace(&mut *guard, peers);
    }
}

pub async fn download(context: Arc<DownloadContext>) {

    log::info!(target: "download", "Downloading task started");

    let mut rx = context.broadcast.subscribe();

    let update_task_context = context.clone();
    let update_task = tokio::spawn(async move {
        update_peer_info_task(update_task_context, 20).await
    });

    tokio::select! {
        _ = signal::ctrl_c() => { update_task.abort(); },
        _ = rx.recv() => { update_task.abort(); },
    };
}