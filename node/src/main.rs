use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use local_files::{DEFAULT_BLOCK_SIZE, default_share_dir};
use node_runtime::run;
use node_shell::NodeConfig;

mod local_files;
mod node_runtime;
mod node_shell;
mod peer_server;
mod tracker_dto;
mod transfer_worker;

#[derive(Parser)]
#[command(name = "resource-node")]
#[command(about = "User node for the resource distribution system")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8000")]
    tracker: String,

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 9001)]
    port: u16,

    #[arg(long, default_value_os_t = default_share_dir())]
    path: PathBuf,

    #[arg(long, default_value_t = DEFAULT_BLOCK_SIZE)]
    block_size: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    run(NodeConfig {
        tracker: cli.tracker,
        peer_host: cli.host,
        peer_port: cli.port,
        share_dir: cli.path,
        block_size: cli.block_size,
    })
    .await
}
