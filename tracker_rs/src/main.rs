mod models;
mod dto;
mod context;

use std::sync::Arc;

use clap::Parser;

use axum::{
    Json, Router, extract::{Query, State}, http::{StatusCode, response}, routing::{get, post}
};
use serde::{Deserialize, Serialize};
use tokio::{fs::OpenOptions, sync::{Notify, mpsc}};

use crate::{context::Context, dto::{FileAnnounceRequest, FileBrief, FileDetailRequest, FileListResponse}, models::FileManifest};

#[derive(Parser, Debug)]
#[command(name = "resource-node")]
#[command(about = "User node for the resource distribution system")]
struct Cli {
    /// Listen endpoint
    #[arg(long = "endpoint", default_value = "0.0.0.0:3000")]
    endpoint: String,

    /// Path for file list store
    #[arg(long = "file-store", default_value = "file_store.json")]
    store_path: String,

    /// Do not store file to 
    #[arg(long = "no-store", default_value_t = false)]
    no_store: bool,


    /// Node expiry time in seconds
    #[arg(long = "ttl", default_value_t = 60)]
    ttl: u64
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Clip resolve
    let cli = Cli::parse();

    let env = env_logger::Env::new().default_filter_or("info");
    env_logger::init_from_env(env);

    let context = Context::new();

    // Load states
    if !cli.no_store {
        if tokio::fs::try_exists(&cli.store_path).await.is_ok_and(|val| val == true) {
            let json_str= tokio::fs::read_to_string(&cli.store_path).await.expect("Failed to read store file content");
            let files : Vec<FileManifest> = serde_json::from_str(&json_str).expect("Failed to parse store file content");
            {
                let mut file_list = context.file_list.write().await;
                let mut file_list_by_hash = context.file_list_by_hash.write().await;
                let mut file_list_by_name = context.file_list_by_name.write().await;

                for file in files {
                    let file = Arc::new(file);
                    file_list.push(file.clone());
                    if file_list_by_hash.insert(file.file_hash.clone(), file.clone()).is_some() {
                        log::warn!("File storage have two file with same hash {}", &file.file_hash)
                    }
                    file_list_by_name.entry(file.file_name.clone()).or_insert(vec![]).push(file.clone());
                    log::debug!("File {} (Hash {}) appended", file.file_name, file.file_hash);
                }
            }
        }
        // TODO: Generate a task here
    }

    let app= Router::new()
        .route("/api/v2/file_list", get(file_list))
        .route("/api/v2/query", get(file_query))
        .route("/api/v2/file_announce", post(file_announce))
        .with_state(Arc::new(context));

    log::info!("Started to listen on {}", &cli.endpoint);
    
    let listener = tokio::net::TcpListener::bind(&cli.endpoint).await.unwrap();
    axum::serve(listener, app).await?;

    Ok(())
}

async fn file_announce(
    State(state): State<Arc<Context>>,
    Json(payload): Json<FileAnnounceRequest>,
) -> (StatusCode, ()) {

    if !payload.is_valid() {
        return (StatusCode::BAD_REQUEST, ());
    }

    let file : Arc<FileManifest> = Arc::new(payload.into());

    // Check hash, if hash is the same, reject the announce
    {
        let mut by_hash = state.file_list_by_hash.write().await;
        if by_hash.contains_key(&file.file_hash) {
            return (StatusCode::CONFLICT, ());
        }
        by_hash.insert(file.file_hash.clone(), file.clone());
    }
    state.file_list_by_name.write().await.entry(file.file_name.clone()).or_insert(vec![]).push(file.clone());
    state.file_list.write().await.push(file.clone());
    state.notify_save.notify_waiters();

    return (StatusCode::OK, ());
}

async fn file_list(
    State(state): State<Arc<Context>>,
) -> (StatusCode, Json<FileListResponse>) {
    
    let brief_list: Vec<FileBrief> = state.file_list.read().await.iter().map(
        |file| FileBrief::from(file.as_ref())
    ).collect();

    (StatusCode::OK, Json(brief_list))
}

async fn file_query(
    State(state): State<Arc<Context>>,
    Query(payload): Query<FileDetailRequest>
) -> (StatusCode, Json<FileListResponse>) {
    
    if let Some(file) = state.file_list_by_hash.read().await.get(&payload.file_id) {
        return (StatusCode::OK, Json(vec![ FileBrief::from(file.as_ref()) ]));
    }
    if let Some(files) = state.file_list_by_name.read().await.get(&payload.file_id) {
        let list : Vec<FileBrief> = files.iter().map(|file| FileBrief::from(file.as_ref())).collect();
        return (StatusCode::OK, Json(list));
    }

    (StatusCode::OK, Json(vec![]))
}