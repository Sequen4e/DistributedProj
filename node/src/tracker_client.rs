use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};

use crate::{announce::AnnounceFileResponse, models::FileManifest, tracker_dto::{FileAnnounceRequest, FileDetailResponse, FileListResponse}};

#[derive(Clone)]
pub struct TrackerClient {
    pub base_url: String,
    client: Client,
}

impl TrackerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
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
            .get(self.url(&format!("/api/v2/file_list?file_id={}", file_id)))
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

        let response = match response.status() {
            StatusCode::OK => AnnounceFileResponse::Ok,
            StatusCode::CONFLICT => AnnounceFileResponse::Conflict,
            StatusCode::BAD_REQUEST => AnnounceFileResponse::BadRequest,
            _ => AnnounceFileResponse::Unknown
        };

        Ok(response)

    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}
