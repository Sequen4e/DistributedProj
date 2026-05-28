use anyhow::{Context, Result, bail};
use reqwest::Client;

use crate::tracker_dto::{
    FilesResponse, OfflineRequest, OfflineResponse, QueryResponse, UpdateNodeRequest,
    UpdateNodeResponse,
};

#[derive(Clone)]
pub struct TrackerClient {
    base_url: String,
    client: Client,
}

impl TrackerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }

    pub async fn update_node(&self, payload: &UpdateNodeRequest) -> Result<UpdateNodeResponse> {
        self.client
            .post(self.url("/api/v1/update"))
            .json(payload)
            .send()
            .await
            .context("failed to send update request to tracker")?
            .error_for_status()
            .context("tracker rejected update request")?
            .json()
            .await
            .context("failed to decode tracker update response")
    }

    pub async fn mark_offline(&self, payload: &OfflineRequest) -> Result<OfflineResponse> {
        self.client
            .post(self.url("/api/v1/offline"))
            .json(payload)
            .send()
            .await
            .context("failed to send offline request to tracker")?
            .error_for_status()
            .context("tracker rejected offline request")?
            .json()
            .await
            .context("failed to decode tracker offline response")
    }

    pub async fn list_files(&self) -> Result<FilesResponse> {
        self.client
            .get(self.url("/api/v1/files"))
            .send()
            .await
            .context("failed to request file list from tracker")?
            .error_for_status()
            .context("tracker rejected file list request")?
            .json()
            .await
            .context("failed to decode tracker file list response")
    }

    pub async fn query_by_hash(&self, file_hash: &str) -> Result<QueryResponse> {
        self.query("file_hash", file_hash).await
    }

    pub async fn query_by_name(&self, file_name: &str) -> Result<QueryResponse> {
        self.query("file_name", file_name).await
    }

    async fn query(&self, key: &str, value: &str) -> Result<QueryResponse> {
        if value.trim().is_empty() {
            bail!("query value must not be empty");
        }

        self.client
            .get(self.url("/api/v1/query"))
            .query(&[(key, value)])
            .send()
            .await
            .with_context(|| format!("failed to query tracker by {key}"))?
            .error_for_status()
            .with_context(|| format!("tracker rejected query by {key}"))?
            .json()
            .await
            .context("failed to decode tracker query response")
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}
