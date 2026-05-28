use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use local_files::{DEFAULT_BLOCK_SIZE, default_share_dir, scan_share_dir};

mod local_files;

#[derive(Parser)]
#[command(name = "resource-node")]
#[command(about = "User node for the resource distribution system")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Scan {
        #[arg(long, default_value_os_t = default_share_dir())]
        path: PathBuf,

        #[arg(long, default_value_t = DEFAULT_BLOCK_SIZE)]
        block_size: usize,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command.unwrap_or_else(default_command) {
        Command::Scan { path, block_size } => {
            let index = scan_share_dir(&path, block_size)?;
            println!("{}", serde_json::to_string_pretty(&index)?);
        }
    }

    Ok(())
}

fn default_command() -> Command {
    Command::Scan {
        path: default_share_dir(),
        block_size: DEFAULT_BLOCK_SIZE,
    }
}
