use std::io::SeekFrom;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::routing::get;
use axum::Router;
use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::download::{BlockStatus, DownloadContext};


pub async fn run_peer_server(context: Arc<DownloadContext>) {

    let app = Router::new()
        .route("/api/v2/blocks/{id}", get(block_request))
        .with_state(context.clone());

    let port = *context.peer_port.read().await;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("Failed to initiate peer server");
    let graceful_context = context.clone();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move { 
            let mut rx = graceful_context.broadcast.subscribe();
            tokio::select! { _ = rx.recv() => {}};
        })
        .await.expect("Failed to initialize axum server");

}

async fn block_request(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<DownloadContext>>,
    Path((block_index,)): Path<(u64,)>
) -> (StatusCode, Vec<u8>) {
    log::info!("{} Requested block {}", addr, block_index);
    if block_index >= state.manifest.file_size.div_ceil(state.manifest.block_size) {
        return (StatusCode::BAD_REQUEST, vec![]);
    }
    if !state.blocks.read().await.get(block_index as usize).is_some_and(|status| *status == BlockStatus::Complete) {
        return (StatusCode::NOT_FOUND, vec![]);
    }

    let mut block_buf = vec![0u8; state.manifest.block_size as usize];
    {
        let mut file = state.file.lock().await;
        if let Err(e) = file.seek(SeekFrom::Start(state.manifest.block_size * block_index)).await {
            log::error!("Failed to seek on file: {:#}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, vec![])
        }
        let actual_size = match file.read(&mut block_buf).await {
            Ok(x) => x,
            Err(e) => { 
                log::error!("Failed to read on file: {:#}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, vec![])
            }
        };
        block_buf.truncate(actual_size);
    }

    (StatusCode::OK, block_buf)
}