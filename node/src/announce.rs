use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use hex::ToHex;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};

use crate::models::FileManifest;
use crate::tracker_dto::FileAnnounceRequest;

pub fn generate_manifest<P: AsRef<Path>>(path: P, block_size: u64) -> io::Result<FileManifest> {
    let Some(file_name) = path.as_ref().file_name() else {
        return Err(io::Error::new(io::ErrorKind::InvalidFilename, "Invalid filename"));
    };
    let file_name = file_name.to_string_lossy().to_string();

    let mut file = File::open(path)?;
    let file_size = file.metadata()?.size();
    let block_count = file_size.div_ceil(block_size);

    let mut block_hashes: Vec<String> = vec![];
    block_hashes.reserve(block_count as usize);

    let mut block_buf: Vec<u8> = vec![0u8; block_size as usize];

    file.seek(SeekFrom::Start(0))?;
    for idx in 0..block_count {
        if idx % 16 == 0 {
            log::debug!("Manifest block hash {} generated", idx)
        }
        let read_size= file.read(&mut block_buf)?;

        block_hashes.push(Sha256::digest(&block_buf[0..read_size]).encode_hex());
    }

    log::debug!("Generating file hash");
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

#[derive(Clone, Debug)]
pub enum AnnounceFileResponse {
    Ok,
    Conflict,
    BadRequest,
    Unknown
}

pub async fn announce_file(tracker_base_url: &str, manifest: FileManifest) -> anyhow::Result<AnnounceFileResponse> {

    let payload = FileAnnounceRequest {
        file_name: manifest.file_name,
        file_hash: manifest.file_hash,
        file_size: manifest.file_size,
        block_size: manifest.block_size,
        block_hashes: manifest.block_hashes,
    };

    let url = format!("{}/api/v2/file_announce", tracker_base_url);
    // println!("Requesting {}", url.green());
    
    let client = reqwest::Client::new();
    let response = client.post(url)
        .json(&payload)
        .send().await?;

    let response = match response.status() {
        StatusCode::OK => AnnounceFileResponse::Ok,
        StatusCode::CONFLICT => AnnounceFileResponse::Conflict,
        StatusCode::BAD_REQUEST => AnnounceFileResponse::BadRequest,
        _ => AnnounceFileResponse::Unknown
    };

    Ok(response)

}