use bytesize::ByteSize;
use colored::Colorize;
use thousands::Separable;

use crate::{announce::generate_manifest, tracker_client::TrackerClient};

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
        crate::announce::AnnounceFileResponse::Ok => {
            println!("Tracker: Successfully announced file {} on tracker {}", manifest.file_name.green().bold(), client.base_url.yellow())
        },
        crate::announce::AnnounceFileResponse::Conflict => {
            println!("Tracker: File {} already announced on tracker {}", manifest.file_name.green().bold(), client.base_url.yellow())
        },
        crate::announce::AnnounceFileResponse::BadRequest => {
            println!("Tracker: Broken file manifest")
        },
        crate::announce::AnnounceFileResponse::Unknown => {
            println!("Tracker: Unknown error from tracker")
        },
    }
}