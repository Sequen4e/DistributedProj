use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "resource-node")]
#[command(about = "User node for the resource distribution system")]
struct Cli {
    #[command(subcommand)]
    command: Commands
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
    /// Tracker endpoint
    #[arg(short = 't', long = "tracker", required = true)]
    tracker: String
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    print!("{:#?}", cli);

    Ok(())
}
