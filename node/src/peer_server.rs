use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde::Deserialize;
use tokio::{net::TcpListener, sync::watch};

use crate::local_files::read_block_by_hash;

#[derive(Clone)]
pub struct PeerServerConfig {
    pub host: String,
    pub port: u16,
    pub share_dir: PathBuf,
    pub block_size: usize,
}

#[derive(Clone)]
struct PeerServerState {
    share_dir: PathBuf,
    block_size: usize,
}

#[derive(Deserialize)]
struct BlockQuery {
    file_hash: String,
    block_index: usize,
}

pub async fn run(config: PeerServerConfig, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    if *shutdown.borrow() {
        return Ok(());
    }

    let address: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .with_context(|| {
            format!(
                "invalid peer listen address: {}:{}",
                config.host, config.port
            )
        })?;
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind peer server on {address}"))?;
    if *shutdown.borrow() {
        return Ok(());
    }

    let state = Arc::new(PeerServerState {
        share_dir: config.share_dir,
        block_size: config.block_size,
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/v1/blocks", get(get_block))
        .with_state(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            while !*shutdown.borrow() {
                if shutdown.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .context("peer server stopped with an error")
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn get_block(
    State(state): State<Arc<PeerServerState>>,
    Query(query): Query<BlockQuery>,
) -> Response<Body> {
    let share_dir = state.share_dir.clone();
    let block_size = state.block_size;
    let file_hash = query.file_hash;
    let block_index = query.block_index;

    let result = tokio::task::spawn_blocking(move || {
        read_block_by_hash(&share_dir, block_size, &file_hash, block_index)
    })
    .await;

    match result {
        Ok(Ok(Some(block))) => block_response(block),
        Ok(Ok(None)) => text_response(StatusCode::NOT_FOUND, "block not found"),
        Ok(Err(error)) => text_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        Err(error) => text_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

fn block_response(block: crate::local_files::LocalBlockData) -> Response<Body> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        "x-file-hash",
        HeaderValue::from_str(&block.file_hash).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        "x-block-index",
        HeaderValue::from_str(&block.block_index.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        "x-block-hash",
        HeaderValue::from_str(&block.block_hash).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        "x-block-size",
        HeaderValue::from_str(&block.size.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("")),
    );

    (StatusCode::OK, headers, block.bytes).into_response()
}

fn text_response(status: StatusCode, message: &str) -> Response<Body> {
    (status, message.to_string()).into_response()
}
