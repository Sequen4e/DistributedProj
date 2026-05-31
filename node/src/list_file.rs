use crate::tracker_dto::{FileDetailResponse, FileListResponse};

pub async fn list_file(tracker_base_url: &str) -> anyhow::Result<FileListResponse> {
    let url = format!("{}/api/v2/file_list", tracker_base_url);
    // println!("Requesting {}", url.green());
    
    let payload: FileListResponse = reqwest::get(url).await?.json().await?;

    Ok(payload)
}

pub async fn query_file(tracker_base_url: &str, file_id: String) -> anyhow::Result<FileDetailResponse> {
    let url = format!("{}/api/v2/query?file_id={}", tracker_base_url, file_id);
    // println!("Requesting {}", url.green());
    
    let payload: FileDetailResponse = reqwest::get(url).await?.json().await?;

    Ok(payload)
}