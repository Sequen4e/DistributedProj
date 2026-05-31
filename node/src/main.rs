mod models;
mod tracker_dto;
mod tracker_client;
mod announce;
mod cli;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::{cli::{announce_file_command, list_file_command, query_file_command}, tracker_client::TrackerClient};

#[derive(Parser, Debug)]
#[command(name = "resource-node")]
#[command(about = "User node for the resource distribution system")]
struct Cli {
    #[command(subcommand)]
    pub command: Commands
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "List all files on the tracker")]
    List (TrackerArg),

    #[command(about = "Show the manifest of a file on the tracker")]
    Check {
        #[command(flatten)]
        tracker: TrackerArg,

        /// File-name or file-hash
        file_id: String
    },

    #[command(about = "Announce a local file to the tracker")]
    Announce {
        #[command(flatten)]
        tracker: TrackerArg,

        /// File block size
        #[arg(short = 'b', long = "block-size", default_value_t = 65536)]
        block_size: u64,

        /// Path to local file to be announced
        file_path: String
    },

    #[command(about = "Start downloading a file")]
    Download {
        #[command(flatten)]
        tracker: TrackerArg,

        /// Optional port hint. If not specified, a random port will be chosen.
        #[arg(short = 'p', long = "port")]
        port: Option<u16>,

        /// File-name or file-hash
        file_id: String,

        /// Path to save path
        save_path: String,
    },
    
    #[command(about = "Start seeding a local file")]
    Seed {
        #[command(flatten)]
        tracker: TrackerArg,

        /// Optional port hint. If not specified, a random port will be chosen.
        #[arg(short = 'p', long = "port")]
        port: Option<u16>,

        /// Path to local file to be seeded
        file_path: String,
    }
}

#[derive(Args, Debug)]
struct TrackerArg {
    /// Tracker base URL
    #[arg(short = 't', long = "tracker", required = true)]
    base_url: String
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // print!("{:#?}", cli);
    match cli.command {
        Commands::List(tracker) => list_file_command(TrackerClient::new(tracker.base_url)).await,
        Commands::Check { tracker, file_id } => query_file_command(TrackerClient::new(tracker.base_url), file_id).await,
        Commands::Announce { tracker, block_size, file_path } => announce_file_command(TrackerClient::new(tracker.base_url), file_path, block_size).await,
        Commands::Download { tracker, port, file_id, save_path } => todo!(),
        Commands::Seed { tracker, port, file_path } => todo!(),
    };

    Ok(())
}
