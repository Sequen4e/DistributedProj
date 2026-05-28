#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::local_files::{BlockResource, FileResource, ResourceIndex};

pub type Metadata = BTreeMap<String, Value>;

#[derive(Debug, Clone, Serialize)]
pub struct UpdateNodeRequest {
    pub node_id: String,
    pub host: String,
    pub port: u16,
    pub resources: Vec<TrackerResource>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfflineRequest {
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerResource {
    pub file_hash: String,
    pub file_name: String,
    pub file_size: u64,
    pub block_size: usize,
    pub blocks: Vec<TrackerBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerBlock {
    pub index: usize,
    pub hash: String,
    pub size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateNodeResponse {
    pub status: String,
    pub node_id: String,
    pub registered_resources: usize,
    pub known_files: usize,
    pub expires_at: String,
    pub server_time: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilesResponse {
    pub files: Vec<FileSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileSummary {
    pub file_hash: String,
    pub file_name: String,
    pub file_size: Option<u64>,
    pub block_size: Option<usize>,
    pub peer_count: usize,
    pub available_blocks: Vec<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryResponse {
    pub locations: Vec<FileLocation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileLocation {
    pub file_hash: String,
    pub file_name: String,
    pub file_size: Option<u64>,
    pub block_size: Option<usize>,
    pub peers: Vec<PeerLocation>,
    pub available_blocks: Vec<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerLocation {
    pub node_id: String,
    pub host: String,
    pub port: u16,
    pub last_seen: String,
    pub expires_at: String,
    pub blocks: Vec<TrackerBlock>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodesResponse {
    pub nodes: Vec<NodeSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeSummary {
    pub node_id: String,
    pub host: String,
    pub port: u16,
    pub last_seen: String,
    pub expires_at: String,
    pub resource_count: usize,
    pub resources: Vec<TrackerResource>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub message: String,
    pub node_ttl_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfflineResponse {
    pub status: String,
    pub removed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorResponse {
    pub status: String,
    pub error: String,
}

pub fn update_request_from_index(
    node_id: impl Into<String>,
    host: impl Into<String>,
    port: u16,
    index: &ResourceIndex,
) -> UpdateNodeRequest {
    UpdateNodeRequest {
        node_id: node_id.into(),
        host: host.into(),
        port,
        resources: index.resources.iter().map(TrackerResource::from).collect(),
        metadata: default_node_metadata(),
    }
}

pub fn offline_request(node_id: impl Into<String>) -> OfflineRequest {
    OfflineRequest {
        node_id: node_id.into(),
    }
}

fn default_node_metadata() -> Metadata {
    BTreeMap::from([(
        "client".to_string(),
        Value::String("rust-resource-node".to_string()),
    )])
}

impl From<&FileResource> for TrackerResource {
    fn from(resource: &FileResource) -> Self {
        Self {
            file_hash: resource.file_hash.clone(),
            file_name: resource.file_name.clone(),
            file_size: resource.file_size,
            block_size: resource.block_size,
            blocks: resource.blocks.iter().map(TrackerBlock::from).collect(),
        }
    }
}

impl From<&BlockResource> for TrackerBlock {
    fn from(block: &BlockResource) -> Self {
        Self {
            index: block.index,
            hash: block.hash.clone(),
            size: block.size,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::local_files::{BlockResource, FileResource, ResourceIndex};

    #[test]
    fn converts_local_index_to_tracker_update_payload() {
        let index = ResourceIndex {
            resources: vec![FileResource {
                file_hash: "hash-x".to_string(),
                file_name: "x.txt".to_string(),
                file_size: 20,
                block_size: 10,
                blocks: vec![BlockResource {
                    index: 0,
                    hash: "block-hash-0".to_string(),
                    size: 10,
                }],
            }],
        };

        let payload = update_request_from_index("node-a", "127.0.0.1", 9001, &index);
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(
            value,
            json!({
                "node_id": "node-a",
                "host": "127.0.0.1",
                "port": 9001,
                "resources": [{
                    "file_hash": "hash-x",
                    "file_name": "x.txt",
                    "file_size": 20,
                    "block_size": 10,
                    "blocks": [{
                        "index": 0,
                        "hash": "block-hash-0",
                        "size": 10
                    }]
                }],
                "metadata": {
                    "client": "rust-resource-node"
                }
            })
        );
    }
}
