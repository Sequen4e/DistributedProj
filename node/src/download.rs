use std::io::SeekFrom;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use hex::ToHex;
use rand::seq::IteratorRandom;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::fs::File;
use tokio::task::JoinSet;
use tokio::{signal, time};

use crate::models::PeerFileInfo;
use crate::signal::BroadcastSignal;
use crate::tracker_dto::{PeerUpdateRequest, PeerUpdateResponse};
use crate::{models::FileManifest, tracker_client::TrackerClient};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum BlockStatus {
    Complete,
    Downloading,
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
    pub block_remote_peers_count: RwLock< Vec <u64> >,
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

    pub async fn calculate_block_remote_peers_count(&self) -> Vec<u64> {
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

    pub async fn remote_peers_with_block(&self, block: u64) -> Vec<PeerFileInfo> {
        self.peers.read().await.iter()
            .filter(|x| x.peer_id != self.node_id && x.blocks.chars().nth(block as usize).is_some_and(|c| c == '1'))
            .map(|info| info.clone())
            .collect()
    }

}

#[derive(Clone, Debug)]
pub struct DownloadTask {
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
            pass && x.peer_id != context.node_id
        }).collect();
        log::info!(target: "peer_update", "Currently {} peers online", peers.len());

        {
            let mut guard = context.peers.write().await;
            let _ = std::mem::replace(&mut *guard, peers);
        }
        {
            let mut guard = context.block_remote_peers_count.write().await;
            let _ = std::mem::replace(&mut *guard, context.calculate_block_remote_peers_count().await);
        }
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
    // Create forced interval
    let period = std::time::Duration::from_secs_f32(1.0);
    let mut interval = tokio::time::interval(period);

    let mut set = JoinSet::new();

    loop {

        while set.len() < 8 {
            if let Some(index) = decide_next_block(context.clone()).await {
                if let Some(peer) = decide_peer(context.clone(), index).await {
                    context.blocks.write().await[index as usize] = BlockStatus::Downloading;

                    let ctx = context.clone();
                    let task_block = DownloadTask {
                        peer_id: peer.peer_id,
                        peer_host: peer.peer_host,
                        peer_port: peer.peer_port,
                        block_index: index,
                    };

                    set.spawn(async move {
                        let result = download_worker(ctx, task_block.clone()).await;
                        (task_block, result)
                    });
                } else {
                    context.blocks.write().await[index as usize] = BlockStatus::Delayed;
                }
            }
            break;
        }

        tokio::select! {
            _ = interval.tick() => {},

            Some(res) = set.join_next(), if !set.is_empty() => {
                match res {
                    Ok((task, Ok(()))) => {
                        // log::info!("Block {} downloaded written to disk", task.block_index);
                        context.blocks.write().await[task.block_index as usize] = BlockStatus::Complete;
                    }
                    Ok((task, Err(e))) => {
                        log::warn!("Block {} download failed: {:#}", task.block_index, e);
                        context.blocks.write().await[task.block_index as usize] = BlockStatus::Pending;
                    }
                    Err(e) => {
                        log::error!("Worker task panicked! {}", e);
                    }
                }
            }

            _ = signal::ctrl_c() => { update_task.abort(); break; },
            _ = rx.recv() => { update_task.abort(); break;},
        }
    }
}

#[derive(Debug)]
pub enum DownloadError {
    NotFound,
    RemotePeerError,
    BadResponseLength{
        expected: u64,
        actual: u64,
    },
    BadResponseHash{
        received: String,
        manifest: String,
    },
    Other (anyhow::Error)
}
impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::NotFound => write!(f, "Block not found on remote peer"),
            DownloadError::RemotePeerError => write!(f, "Remote peer reported unknown error"),
            DownloadError::BadResponseLength {expected, actual, .. } => write!(f, "Bad remote peer response length, expected {expected}, actual {actual}"),
            DownloadError::BadResponseHash {received, manifest, .. } => write!(f, "Bad remote peer response, manifest hash {manifest}, received hash {received}"),
            DownloadError::Other(err) => write!(f, "Other error: {:#}", err),
        }
    }
}

impl<E> From<E> for DownloadError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        DownloadError::Other(err.into())
    }
}


pub async fn download_worker(context: Arc<DownloadContext>, task: DownloadTask) -> Result<(), DownloadError> {

    let url = format!("http://{}:{}/api/v2/blocks/{}", task.peer_host, task.peer_port, task.block_index);
    log::info!(target:"download_worker", "Polling node {}\n  ({})", task.peer_id, url);

    let response = reqwest::get(url).await?;
    match response.status() {
        StatusCode::OK => { }
        StatusCode::NOT_FOUND => { return Err(DownloadError::NotFound) }
        _ => { return Err(DownloadError::RemotePeerError) }
    };

    let recv_buf = response.bytes().await?;

    let correct_block_count = context.manifest.file_size.div_ceil(context.manifest.block_size);
    let correct_recv_size = if correct_block_count == task.block_index + 1 {
        context.manifest.file_size - (task.block_index * context.manifest.block_size)
    } else {
        context.manifest.block_size
    };
    if recv_buf.len() as u64 != correct_recv_size {
        return Err(DownloadError::BadResponseLength { expected: correct_recv_size, actual: recv_buf.len() as u64 })
    }

    let expected_hash = context.manifest.block_hashes.get(task.block_index as usize);
    let received_hash: String = Sha256::digest(&recv_buf).encode_hex();
    if !expected_hash.is_some_and(|hash| *hash == received_hash) {
        return Err(DownloadError::BadResponseHash { received: received_hash, manifest: expected_hash.cloned().unwrap_or("Block hash not found in manifest".to_string()) })
    }

    // File IO
    let mut file = context.file.lock().await;
    file.seek(SeekFrom::Start(context.manifest.block_size * task.block_index)).await?;
    file.write(&recv_buf).await?;

    Ok(())
}

pub async fn decide_next_block(context: Arc<DownloadContext>) -> Option<u64> {
    let blocks = context.blocks.read().await;
    // (Index and count)
    let mut pending_blocks: Vec<(u64, u64)> = context.block_remote_peers_count.read().await.iter().enumerate()
        .filter(|(idx, count)| blocks.get(*idx).is_some_and(|status| **count != 0 && *status == BlockStatus::Pending ))
        .map(|(idx, count)| (idx as u64, *count) )
        .collect();
    pending_blocks.sort_by(|(ai, ac), (bi, bc)| {
        match ac.cmp(bc) {
            std::cmp::Ordering::Equal => ai.cmp(bi) ,
            _x => _x
        }
    });

    pending_blocks.first().map(|(idx, _)| *idx)
}

pub async fn decide_peer(context: Arc<DownloadContext>, block_index: u64) -> Option<PeerFileInfo> {
    let possible_peers = context.remote_peers_with_block(block_index).await;

    let possible_peers : Vec<(PeerFileInfo, u64)> = possible_peers.into_iter()
        .map(|info| {
            let instance = context.transmitted.entry(info.peer_id.clone()).or_insert(0);
            (info, *instance)
        })
        .collect();

    let min_count = possible_peers.iter().map(|(_, count)| *count).min()?;

    possible_peers.into_iter()
        .filter(|(_, count)| *count == min_count)
        .choose(&mut rand::rng())
        .map(|(node, _)| node)
}