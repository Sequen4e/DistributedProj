use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::fs::File;
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
    pub peers: RwLock<Vec <PeerFileInfo> >,
    pub transmitted: DashMap<String, u64>,
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

    pub async fn block_remote_peers_count(&self) -> Vec<u64> {
        let block_count = self.manifest.file_size.div_ceil(self.manifest.block_size);
        let bitmap : Vec<String> = self.peers.read().await.iter().map(|x| x.blocks.clone()).collect();
        let mut peer_counts = vec![0; block_count as usize];
        for peer_block in bitmap {
            for index in 0..block_count as usize {
                if peer_block.chars().nth(index).is_some_and(|c| c == '1') {
                    peer_counts[index] += 1;
                }
            }
        }
        peer_counts
    }

    pub async fn remote_peers_with_block(&self, block: u64) -> Vec<String> {
        self.peers.read().await.iter()
            .filter(|x| x.peer_id != self.node_id && x.blocks.chars().nth(block as usize).is_some_and(|c| c == '1'))
            .map(|info| info.peer_id.clone())
            .collect()
    }

}

pub struct DownloadBlock {
    pub peer_id: String,
    pub peer_host: String,
    pub peer_port: u16,
    pub block_index: u64,
}

pub async fn update_peer_info_task(context: Arc<DownloadContext>, seconds: u64) {
    let mut interval = time::interval(Duration::from_secs(seconds));

    loop {
        interval.tick().await;

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

        let peers: Vec<PeerFileInfo> = peers.into_iter().filter(|x| {
            let pass = x.blocks.len() as u64 == (context.manifest.file_size.div_ceil(context.manifest.block_size)) &&
            x.blocks.chars().all(|c| c == '0' || c == '1');
            if !pass {
                log::warn!(target: "peer_update", "Invalid peer info inspesctd in peer list (peer_id{})", x.peer_id);
            }
            pass
        }).collect();

        log::info!(target: "peer_update", "Currently {} peers online", peers.len());

        let mut guard = context.peers.write().await;
        let _ = std::mem::replace(&mut *guard, peers);
    }
}

pub async fn download(context: Arc<DownloadContext>) {

    log::info!(target: "download", "Downloading task started");

    let mut rx = context.broadcast.subscribe();

    // Create update task
    let update_task_context = context.clone();
    let update_task = tokio::spawn(async move {
        update_peer_info_task(update_task_context, 20).await
    });

    

    tokio::select! {
        _ = signal::ctrl_c() => { update_task.abort(); },
        _ = rx.recv() => { update_task.abort(); },
    };
}