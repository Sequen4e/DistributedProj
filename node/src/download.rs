use ratatui::widgets::Block;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::fs::File;

use crate::signal::BroadcastSignal;
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
    pub broadcast: broadcast::Sender<BroadcastSignal>,
    pub peer_port: RwLock<u16>,
    pub tracker_client: TrackerClient,
    pub manifest: FileManifest,
    pub file: Mutex<File>,
    pub blocks: RwLock<Vec <BlockStatus> >
}