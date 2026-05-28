use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use local_files::{DEFAULT_BLOCK_SIZE, default_share_dir};
use node_shell::{NodeConfig, run};

mod local_files;
mod node_shell;
mod tracker_dto;

#[derive(Parser)]
#[command(name = "resource-node")]
#[command(about = "User node for the resource distribution system")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8000")]
    tracker: String,

    #[arg(long, default_value_os_t = default_share_dir())]
    path: PathBuf,

    #[arg(long, default_value_t = DEFAULT_BLOCK_SIZE)]
    block_size: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // TODO: start HTTP server in background thread
    run(NodeConfig {
        tracker: cli.tracker,
        share_dir: cli.path,
        block_size: cli.block_size,
    })
}
