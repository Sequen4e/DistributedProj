use std::sync::Arc;
use std::path::Path;

use bytesize::ByteSize;
use colored::Colorize;
use dashmap::DashMap;
use thousands::Separable;
use tokio::task::JoinSet;
use tokio::sync::{Mutex, RwLock, broadcast};

use crate::tracker_dto::AnnounceFileResponse;
use crate::tui;
use crate::tracker_client::TrackerClient;
use crate::random_id::generate_node_id;
use crate::peer_server::run_peer_server;
use crate::download::{BlockStatus, DownloadContext, download};
use crate::announce::{generate_file_hash, generate_manifest};

pub async fn list_file_command(client: TrackerClient) {

    env_logger::init();

    let files = match client.list_file().await {
        Ok(x) => x,
        Err(e) => {
            log::error!("{} {:#}", "Failed to request file list".red(), e);
            return;
        }
    };
    

    println!();
    for file in files.iter() {
        println!("{}:", file.file_name.bright_green().bold());
        println!("  {} {}", "Hash:".yellow(), file.file_hash);
        if file.file_size >= 1024 {
            println!("  {} {} ({} bytes)", "Size:".yellow(), ByteSize::b(file.file_size).display().iec(), file.file_size.separate_with_commas());
        } else {
            println!("  {} {}", "Size:".yellow(), file.file_size)
        }
        println!("  {} {} ({} blocks)", "Block Size:".yellow(), ByteSize::b(file.block_size).display().iec(), file.file_size.div_ceil(file.block_size));
        println!();
    }

    println!("Total {} files on server", files.len().to_string().green());
}

pub async fn query_file_command(client: TrackerClient, file_id: String) {

    env_logger::init();
    
    let files = match client.query_file(&file_id).await {
        Ok(x) => x,
        Err(e) => {
            log::error!("{} {:#}", "Failed to request file list".red(), e);
            return;
        }
    };

    println!();
    for file in files.iter() {
        println!("{}:", file.file_name.bright_green().bold());
        println!("  {} {}", "Hash:".yellow(), file.file_hash);
        if file.file_size >= 1024 {
            println!("  {} {} ({} bytes)", "Size:".yellow(), ByteSize::b(file.file_size).display().iec(), file.file_size.separate_with_commas());
        } else {
            println!("  {} {}", "Size:".yellow(), file.file_size)
        }
        println!("  {} {} ({} blocks)", "Block Size:".yellow(), ByteSize::b(file.block_size).display().iec(), file.file_size.div_ceil(file.block_size));
        println!();
    }

    println!("Total {} files in query result", files.len().to_string().green());
}

pub async fn announce_file_command(client: TrackerClient, file_path: String, block_size: u64) {

    env_logger::init();

    let manifest = match generate_manifest(file_path, block_size, Some(indicatif::ProgressBar::no_length())) {
        Ok(x) => x,
        Err(e) => {
            log::error!("{} {:#}", "Failed to generate file manifest:".red(), e);
            return;
        }
    };

    println!("Announcing file {} on tracker...", manifest.file_name.green().bold());

    let result = match client.announce_file(&manifest).await {
        Ok(x) => x,
        Err(e) => {
            log::error!("{} {:#}", "Failed to announce manifest:".red(), e);
            return;
        }
    };

    match result {
        AnnounceFileResponse::Ok => {
            println!("Tracker: Successfully announced file {} on tracker {}", manifest.file_name.green().bold(), client.base_url.yellow())
        },
        AnnounceFileResponse::Conflict => {
            println!("Tracker: File {} already announced on tracker {}", manifest.file_name.green().bold(), client.base_url.yellow())
        },
        AnnounceFileResponse::BadRequest => {
            println!("Tracker: Broken file manifest")
        },
        AnnounceFileResponse::Unknown => {
            println!("Tracker: Unknown error from tracker")
        },
    }
}

pub async fn download_command(client: TrackerClient, file_id: String, save_path: String, listen_port: Option<u16>, node_name: Option<String>) {

    // env_logger::init();
    tui_logger::init_logger(log::LevelFilter::Trace).expect("Failed to initialize TUI logger");
    tui_logger::set_default_level(log::LevelFilter::Info);

    let save_path = Path::new(&save_path);
    if !save_path.is_dir() && !save_path.parent().is_some_and(|path| path.is_dir()) {
        println!("{} File save directory does not exist", "Error:".bright_red().bold());
        return;
    }

    let manifest_list = client.query_file(&file_id).await.expect("Failed to query file list");

    if manifest_list.len() == 0 {
        println!("{} File not found on tracker", "Error:".bright_red().bold());
        return;
    }

    // Select manifest
    let manifest = if manifest_list.len() >= 2 {
        for (idx, file) in manifest_list.iter().enumerate() {
            println!("[{}] {}:", idx + 1, file.file_name.bright_green().bold());
            println!("  {} {}", "Hash:".yellow(), file.file_hash);
            if file.file_size >= 1024 {
                println!("  {} {} ({} bytes)", "Size:".yellow(), ByteSize::b(file.file_size).display().iec(), file.file_size.separate_with_commas());
            } else {
                println!("  {} {}", "Size:".yellow(), file.file_size)
            }
            println!("  {} {} ({} blocks)", "Block Size:".yellow(), ByteSize::b(file.block_size).display().iec(), file.file_size.div_ceil(file.block_size));
            println!();
        }
        loop {
            print!("Select a file index to download: ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).expect("Failed to readline");
            let idx: i64 = match input.trim().parse() {
                Ok(num) => num,
                Err(_) => {
                    println!("Please enter a valid number.");
                    continue;
                } 
            };
            match manifest_list.get((idx - 1) as usize) {
                Some(x) => break x.clone(),
                None => {
                    println!("Index out of range.");
                    continue;
                }
            }
        }
    } else {
        manifest_list[0].clone()
    };

    let file_path = if save_path.is_dir() {
        save_path.join(&manifest.file_name)
    } else {
        save_path.to_path_buf()
    };

    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(file_path).await.expect("Failed to open file");

    file.set_len(manifest.file_size).await.expect("Failed to allocate space for file");

    let block_count = manifest.file_size.div_ceil(manifest.block_size);
    let blocks = vec![BlockStatus::Pending; block_count as usize];

    let (tx, _rx) = broadcast::channel(1);

    let context = DownloadContext {
        node_id: node_name.unwrap_or_else(|| generate_node_id(listen_port.unwrap_or(0))),
        broadcast: tx,
        peer_port: RwLock::new(listen_port.unwrap_or(0)),
        tracker_client: client,
        manifest,
        file: Mutex::new(file),
        blocks: RwLock::new(blocks),
        peers: RwLock::new(vec![]),
        transmitted: DashMap::new(),
        block_remote_peers_count: RwLock::new(vec![]),
    };
    let context = Arc::new(context);

    let mut set = JoinSet::new();

    {
        let context = context.clone();
        set.spawn(async {
            tui::init_tui(context).await;
        });
    }
    {
        let context = context.clone();
        set.spawn(async {
            run_peer_server(context).await;
        });
    }
    {
        let context = context.clone();
        set.spawn(async {
            download(context).await;
        });
    }

    set.join_all().await;

}

pub async fn seed_command(client: TrackerClient, file_path: String, listen_port: Option<u16>, node_name: Option<String>) {

    // env_logger::init();
    tui_logger::init_logger(log::LevelFilter::Trace).expect("Failed to initialize TUI logger");
    tui_logger::set_default_level(log::LevelFilter::Info);

    println!("Checking local file hash...");
    let file_hash = generate_file_hash(&file_path).expect("Failed to generate file hash");
    let file_list = client.query_file(&file_hash).await.expect("Failed to query file on tracker");
    if file_list.len() < 1 {
        println!("{} File not found on tracker", "Error:".bright_red().bold());
        return;
    } else if file_list.len() > 1 {
        println!("{} 2 file with same hash on server", "Error:".bright_red().bold());
        return;
    }
    let remote_manifest = file_list[0].clone();

    println!("Remote manifest retrived. Calculating local manifest...");
    let local_manifest = generate_manifest(&file_path, remote_manifest.block_size, Some(indicatif::ProgressBar::no_length())).expect("Failed to generate file manifest");

    //// TODO Integrity check
    if remote_manifest.block_hashes != local_manifest.block_hashes {
        println!("{} Local file block hashes is not consistent with remote manifest. Maybe file is incorrect or broken?", "Error:".bright_red().bold());
        return;
    }

    println!("Opening local file...");
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .open(&file_path).await.expect("Failed to open file");

    let block_count = remote_manifest.file_size.div_ceil(remote_manifest.block_size);
    let blocks = vec![BlockStatus::Complete; block_count as usize];

    let (tx, _rx) = broadcast::channel(1);

    let context = DownloadContext {
        node_id: node_name.unwrap_or_else(|| generate_node_id(listen_port.unwrap_or(0))),
        broadcast: tx,
        peer_port: RwLock::new(listen_port.unwrap_or(0)),
        tracker_client: client,
        manifest: remote_manifest,
        file: Mutex::new(file),
        blocks: RwLock::new(blocks),
        peers: RwLock::new(vec![]),
        transmitted: DashMap::new(),
        block_remote_peers_count: RwLock::new(vec![]),
    };
    let context = Arc::new(context);

    let mut set = JoinSet::new();

    {
        let context = context.clone();
        set.spawn(async {
            tui::init_tui(context).await;
        });
    }
    {
        let context = context.clone();
        set.spawn(async {
            run_peer_server(context).await;
        });
    }
    {
        let context = context.clone();
        set.spawn(async {
            download(context).await;
        });
    }

    set.join_all().await;
}