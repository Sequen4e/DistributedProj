use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileManifest {
    pub file_name: String,
    pub file_hash: String,
    pub file_size: u64,
    pub block_size: u64,
    // Hash for each block
    pub block_hashes: Vec<String>
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerFileInfo {
    pub file_hash: String,
    pub peer_id: String,
    pub peer_host: String,
    pub peer_port: u16,
    /// A 0-1 string
    pub blocks: String,
    pub last_seen: DateTime<Utc>
}

