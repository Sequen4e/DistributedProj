use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::models::{FileManifest, PeerFileInfo};

/// The request body of announce new file
#[derive(Serialize, Deserialize, Debug)]
pub struct FileAnnounceRequest {
    pub file_name: String,
    pub file_hash: String,
    pub file_size: u64,
    pub block_size: u64,
    // Hash for each block
    pub block_hashes: Vec<String>
}
impl Into<FileManifest> for FileAnnounceRequest {
    fn into(self) -> FileManifest {
        FileManifest {
            file_name: self.file_name,
            file_hash: self.file_hash,
            file_size: self.file_size,
            block_size: self.block_size,
            block_hashes: self.block_hashes,
        }
    }
}
impl FileAnnounceRequest {
    pub fn is_valid(&self) -> bool {
        let correct_block_count = self.file_size.div_ceil(self.block_size);
        return correct_block_count == self.block_hashes.len() as u64;
    }
}

#[derive(Clone, Debug)]
pub enum AnnounceFileResponse {
    Ok,
    Conflict,
    BadRequest,
    Unknown
}
impl From<StatusCode> for AnnounceFileResponse {
    fn from(value: StatusCode) -> Self {
        match value {
            StatusCode::OK => AnnounceFileResponse::Ok,
            StatusCode::CONFLICT => AnnounceFileResponse::Conflict,
            StatusCode::BAD_REQUEST => AnnounceFileResponse::BadRequest,
            _ => AnnounceFileResponse::Unknown
        }
    }
}

/// Brief object used in file list
#[derive(Serialize, Deserialize, Debug)]
pub struct FileBrief {
    pub file_name: String,
    pub file_hash: String,
    pub file_size: u64,
    pub block_size: u64,
}
impl From<&FileManifest> for FileBrief {
    fn from(file: &FileManifest) -> Self {
        FileBrief {
            file_name: file.file_name.clone(),
            file_hash: file.file_hash.clone(),
            file_size: file.file_size,
            block_size: file.block_size,
        }
    }
}
pub type FileListResponse = Vec<FileBrief>;

pub type FileDetailResponse = Vec<FileManifest>;

// Peer updates
#[derive(Serialize, Deserialize, Debug)]
pub struct PeerUpdateRequest {
    pub file_hash: String,
    pub peer_id: String,
    /// If not specified, use HTTP client ID
    pub peer_host: Option<String>,
    pub peer_port: u16,
    /// A 0-1 string
    pub blocks: String
}
#[derive(Clone, Debug)]
pub enum PeerUpdateResponse {
    Ok,
    FileNotFound,
    Unknown
}
impl From<StatusCode> for PeerUpdateResponse {
    fn from(value: StatusCode) -> Self {
        match value {
            StatusCode::OK => PeerUpdateResponse::Ok,
            StatusCode::NOT_FOUND => PeerUpdateResponse::FileNotFound,
            _ => PeerUpdateResponse::Unknown
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PeerListRequest {
    pub file_hash: String,
}
pub type PeerListResponse = Vec<PeerFileInfo>;
