use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;

use crate::tracker_dto::{AnnounceFileResponse, FileAnnounceRequest, FileDetailResponse, FileListResponse, PeerListResponse, PeerUpdateRequest, PeerUpdateResponse};
use crate::models::FileManifest;
#[derive(Clone, Debug)]
pub struct TrackerClient {
    pub base_url: String,
    client: Client,
}

impl TrackerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build().expect("Failed to build tracker client"),
        }
    }

    pub async fn list_file(&self) -> Result<FileListResponse> {
        self.client
            .get(self.url("/api/v2/file_list"))
            .send()
            .await
            .context("Failed to send list file request")?
            .json()
            .await
            .context("Failed to parse tracker JSON payload")
    }

    pub async fn query_file(&self, file_id: &str) -> Result<FileDetailResponse> {
        self.client
            .get(self.url(&format!("/api/v2/query?file_id={}", file_id)))
            .send()
            .await
            .context("Failed to send query file request")?
            .json()
            .await
            .context("Failed to parse tracker JSON payload")
    }

    pub async fn announce_file(&self, manifest: &FileManifest) -> anyhow::Result<AnnounceFileResponse> {
        
        let payload = FileAnnounceRequest {
            file_name: manifest.file_name.clone(),
            file_hash: manifest.file_hash.clone(),
            file_size: manifest.file_size,
            block_size: manifest.block_size,
            block_hashes: manifest.block_hashes.clone(),
        };

        let response = self.client
            .post(self.url("/api/v2/file_announce"))
            .json(&payload)
            .send()
            .await
            .context("Failed to send announce file request")?;

        Ok(AnnounceFileResponse::from(response.status()))

    }

    pub async fn update_peer(&self, req: PeerUpdateRequest) -> anyhow::Result<PeerUpdateResponse> {

        let response = self.client
            .post(self.url("/api/v2/update"))
            .json(&req)
            .send()
            .await
            .context("Failed to send peer update")?;

        Ok(PeerUpdateResponse::from(response.status()))
    }

    pub async fn peer_list(&self, file_hash: &str) -> anyhow::Result<PeerListResponse> {
        self.client
            .get(self.url(&format!("/api/v2/peer_list?file_hash={}", file_hash)))
            .send()
            .await
            .context("Failed to request peer list")?
            .json()
            .await
            .context("Failed to parse tracker JSON payload")
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}
