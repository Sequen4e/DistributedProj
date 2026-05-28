use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;

use crate::local_files::scan_share_dir;

pub struct NodeConfig {
    pub tracker: String,
    pub share_dir: PathBuf,
    pub block_size: usize,
}

pub fn run(config: NodeConfig) -> Result<()> {
    ensure_share_dir(&config.share_dir)?;
    print_banner(&config);
    print_help_hint();

    let mut input = String::new();
    loop {
        print!("node> ");
        io::stdout().flush()?;

        input.clear();
        if io::stdin().read_line(&mut input)? == 0 {
            println!();
            break;
        }

        let command = input.trim();
        if command.is_empty() {
            continue;
        }

        if handle_command(command, &config)? {
            break;
        }
        println!();
    }

    Ok(())
}

fn ensure_share_dir(share_dir: &PathBuf) -> Result<()> {
    if !share_dir.exists() {
        std::fs::create_dir_all(share_dir)?;
    }
    Ok(())
}

fn print_banner(config: &NodeConfig) {
    println!("Resource node is running.");
    println!("Tracker: {}", config.tracker);
    println!("Local files: {}", config.share_dir.display());
}

fn print_help_hint() {
    println!("Type `help` to list commands.");
}

fn handle_command(command: &str, config: &NodeConfig) -> Result<bool> {
    let mut parts = command.split_whitespace();
    match parts.next() {
        Some("scan") => scan_summary(config)?,
        Some("scanCplt") => scan_complete(config)?,
        Some("help") => print_help(),
        Some("exit") => {
            println!("Stopping resource node.");
            return Ok(true);
        }
        Some(unknown) => {
            println!("Unknown command: {unknown}");
            print_help_hint();
        }
        None => {}
    }

    Ok(false)
}

fn scan_summary(config: &NodeConfig) -> Result<()> {
    let index = scan_share_dir(&config.share_dir, config.block_size)?;
    if index.resources.is_empty() {
        println!("No local files found.");
        return Ok(());
    }

    println!("file_name | block_count");
    for resource in index.resources {
        println!("{} | {}", resource.file_name, resource.blocks.len());
    }

    Ok(())
}

fn scan_complete(config: &NodeConfig) -> Result<()> {
    let index = scan_share_dir(&config.share_dir, config.block_size)?;
    println!("{}", serde_json::to_string_pretty(&index)?);
    Ok(())
}

fn print_help() {
    println!("Available commands:");
    println!("  scan      Scan local files and print file names with block counts");
    println!("  scanCplt  Scan local files and print complete resource index JSON");
    println!("  help      List available commands");
    println!("  exit      Stop this node process");
}
