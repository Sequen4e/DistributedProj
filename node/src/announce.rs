use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::fs::Metadata;
use std::path::Path;

use hex::ToHex;
use sha2::{Digest, Sha256};

use crate::models::FileManifest;

pub fn generate_file_hash<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let file = File::open(path)?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    io::copy(&mut reader, &mut hasher)?;

    let file_hash = hasher.finalize().encode_hex();

    Ok(file_hash)
}

pub fn generate_manifest<P: AsRef<Path>>(path: P, block_size: u64, pb: Option<indicatif::ProgressBar>) -> io::Result<FileManifest> {
    let Some(file_name) = path.as_ref().file_name() else {
        return Err(io::Error::new(io::ErrorKind::InvalidFilename, "Invalid filename"));
    };
    let file_name = file_name.to_string_lossy().to_string();

    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();
    let block_count = file_size.div_ceil(block_size);

    let mut block_hashes: Vec<String> = vec![];
    block_hashes.reserve(block_count as usize);

    println!("Generating block hashes...");
    let mut block_buf: Vec<u8> = vec![0u8; block_size as usize];

    let pb = pb.unwrap_or_else(indicatif::ProgressBar::hidden);
    pb.set_length(block_count);

    file.seek(SeekFrom::Start(0))?;
    for _ in 0..block_count {
        let read_size= file.read(&mut block_buf)?;
        block_hashes.push(Sha256::digest(&block_buf[0..read_size]).encode_hex());
        pb.inc(1);
    }
    pb.finish_with_message("Done generating block hashes");

    log::debug!("Generating file hash..");
    file.seek(SeekFrom::Start(0))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    io::copy(&mut reader, &mut hasher)?;

    let file_hash = hasher.finalize().encode_hex();


    Ok(FileManifest {
        file_name,
        file_hash,
        file_size,
        block_size,
        block_hashes,
    })
}