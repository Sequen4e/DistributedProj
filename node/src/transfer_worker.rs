use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, watch};

use crate::{
    local_files::scan_share_dir,
    node_runtime::RuntimeCommand,
    node_shell::NodeConfig,
    tracker_client::TrackerClient,
    tracker_dto::{FileLocation, PeerLocation, offline_request, update_request_from_index},
};

pub async fn run(
    config: NodeConfig,
    mut command_rx: mpsc::UnboundedReceiver<RuntimeCommand>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    println!("Transfer worker is ready.");
    let worker = TransferWorker::new(config);

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                if let RuntimeCommand::Shutdown = command {
                    break;
                }
                worker.handle(command).await;
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }

    println!("Transfer worker stopped.");
    Ok(())
}

struct TransferWorker {
    config: NodeConfig,
    tracker: TrackerClient,
    http: Client,
}

impl TransferWorker {
    fn new(config: NodeConfig) -> Self {
        Self {
            tracker: TrackerClient::new(config.tracker.clone()),
            http: Client::new(),
            config,
        }
    }

    async fn handle(&self, command: RuntimeCommand) {
        let result = match command {
            RuntimeCommand::PublishLocalFiles => self.publish_local_files().await,
            RuntimeCommand::ListTrackerFiles => self.list_tracker_files().await,
            RuntimeCommand::Download { target } => self.download(&target).await,
            RuntimeCommand::MarkOffline => self.mark_offline().await,
            RuntimeCommand::Shutdown => Ok(()),
        };

        if let Err(error) = result {
            eprintln!("Command failed: {error:#}");
        }
    }

    async fn publish_local_files(&self) -> Result<()> {
        let index = scan_share_dir(&self.config.share_dir, self.config.block_size)?;
        let payload = update_request_from_index(
            &self.config.node_id,
            &self.config.peer_host,
            self.config.peer_port,
            &index,
        );
        let response = self.tracker.update_node(&payload).await?;
        println!(
            "Published {} local resources. Tracker knows {} files. Expires at {}.",
            response.registered_resources, response.known_files, response.expires_at
        );
        Ok(())
    }

    async fn list_tracker_files(&self) -> Result<()> {
        let response = self.tracker.list_files().await?;
        if response.files.is_empty() {
            println!("Tracker has no files.");
            return Ok(());
        }

        println!("file_name | file_hash | peers | blocks");
        for file in response.files {
            println!(
                "{} | {} | {} | {}",
                file.file_name,
                file.file_hash,
                file.peer_count,
                file.available_blocks.len()
            );
        }
        Ok(())
    }

    async fn mark_offline(&self) -> Result<()> {
        let payload = offline_request(&self.config.node_id);
        let response = self.tracker.mark_offline(&payload).await?;
        println!(
            "Tracker offline update accepted. removed={}",
            response.removed
        );
        Ok(())
    }

    async fn download(&self, target: &str) -> Result<()> {
        let location = self.resolve_download_target(target).await?;
        let blocks = self.download_blocks(&location).await?;
        let bytes = join_blocks(blocks)?;

        let actual_hash = sha256_hex(&bytes);
        if actual_hash != location.file_hash {
            bail!(
                "downloaded file hash mismatch: expected {}, got {}",
                location.file_hash,
                actual_hash
            );
        }

        let output_path = safe_output_path(&self.config.share_dir, &location.file_name)?;
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&output_path, bytes)
            .await
            .with_context(|| {
                format!("failed to write downloaded file: {}", output_path.display())
            })?;
        println!(
            "Downloaded {} to {}",
            location.file_name,
            output_path.display()
        );

        self.publish_local_files().await?;
        Ok(())
    }

    async fn resolve_download_target(&self, target: &str) -> Result<FileLocation> {
        let by_hash = self.tracker.query_by_hash(target).await?;
        if let Some(location) = by_hash.locations.into_iter().next() {
            return Ok(location);
        }

        let by_name = self.tracker.query_by_name(target).await?;
        match by_name.locations.len() {
            0 => bail!("tracker has no file matching {target}"),
            1 => Ok(by_name.locations.into_iter().next().expect("one location")),
            count => bail!(
                "tracker returned {count} files named {target}; use file_hash to disambiguate"
            ),
        }
    }

    async fn download_blocks(&self, location: &FileLocation) -> Result<Vec<(usize, Vec<u8>)>> {
        if location.available_blocks.is_empty() {
            bail!(
                "tracker returned no available blocks for {}",
                location.file_name
            );
        }

        let location = Arc::new(location.clone());
        let mut handles = Vec::new();
        for block_index in location.available_blocks.iter().copied() {
            let Some(peer) = select_peer_for_block(&location, block_index, &self.config.node_id)
            else {
                bail!("no peer can provide block {block_index}");
            };
            let expected_hash = peer
                .blocks
                .iter()
                .find(|block| block.index == block_index)
                .map(|block| block.hash.clone());
            let http = self.http.clone();
            let file_hash = location.file_hash.clone();
            handles.push(tokio::spawn(async move {
                fetch_block(http, peer, file_hash, block_index, expected_hash).await
            }));
        }

        let mut blocks = Vec::new();
        for handle in handles {
            blocks.push(handle.await.context("download task failed to join")??);
        }
        Ok(blocks)
    }
}

fn select_peer_for_block(
    location: &FileLocation,
    block_index: usize,
    self_node_id: &str,
) -> Option<PeerLocation> {
    let can_provide =
        |peer: &&PeerLocation| peer.blocks.iter().any(|block| block.index == block_index);
    location
        .peers
        .iter()
        .filter(can_provide)
        .find(|peer| peer.node_id != self_node_id)
        .or_else(|| location.peers.iter().filter(can_provide).next())
        .cloned()
}

async fn fetch_block(
    http: Client,
    peer: PeerLocation,
    file_hash: String,
    block_index: usize,
    expected_hash: Option<String>,
) -> Result<(usize, Vec<u8>)> {
    let url = format!("http://{}:{}/api/v1/blocks", peer.host, peer.port);
    let bytes = http
        .get(url)
        .query(&[
            ("file_hash", file_hash.as_str()),
            ("block_index", &block_index.to_string()),
        ])
        .send()
        .await
        .with_context(|| {
            format!(
                "failed to request block {block_index} from {}",
                peer.node_id
            )
        })?
        .error_for_status()
        .with_context(|| format!("peer {} rejected block {block_index}", peer.node_id))?
        .bytes()
        .await
        .with_context(|| format!("failed to read block {block_index} from {}", peer.node_id))?
        .to_vec();

    if let Some(expected_hash) = expected_hash {
        let actual_hash = sha256_hex(&bytes);
        if actual_hash != expected_hash {
            bail!(
                "block {block_index} hash mismatch from {}: expected {}, got {}",
                peer.node_id,
                expected_hash,
                actual_hash
            );
        }
    }

    Ok((block_index, bytes))
}

fn join_blocks(mut blocks: Vec<(usize, Vec<u8>)>) -> Result<Vec<u8>> {
    blocks.sort_by_key(|(index, _)| *index);
    for (expected, (actual, _)) in blocks.iter().enumerate() {
        if *actual != expected {
            bail!("downloaded blocks are not contiguous at index {expected}");
        }
    }

    Ok(blocks
        .into_iter()
        .flat_map(|(_, bytes)| bytes)
        .collect::<Vec<_>>())
}

fn safe_output_path(root: &Path, file_name: &str) -> Result<PathBuf> {
    let mut path = PathBuf::from(root);
    for component in Path::new(file_name).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => bail!("tracker returned unsafe file name: {file_name}"),
        }
    }
    Ok(path)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
