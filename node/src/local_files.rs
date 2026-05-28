use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const DEFAULT_BLOCK_SIZE: usize = 512;

#[derive(Serialize)]
pub struct ResourceIndex {
    pub resources: Vec<FileResource>,
}

#[derive(Serialize)]
pub struct FileResource {
    pub file_hash: String,
    pub file_name: String,
    pub file_size: u64,
    pub block_size: usize,
    pub blocks: Vec<BlockResource>,
}

#[derive(Serialize)]
pub struct BlockResource {
    pub index: usize,
    pub hash: String,
    pub size: usize,
}

pub fn default_share_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("file")
}

pub fn scan_share_dir(root: &Path, block_size: usize) -> Result<ResourceIndex> {
    if block_size == 0 {
        bail!("block_size must be greater than 0");
    }
    if !root.exists() {
        bail!("share directory does not exist: {}", root.display());
    }
    if !root.is_dir() {
        bail!("share path is not a directory: {}", root.display());
    }

    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve share directory: {}", root.display()))?;
    let mut files = collect_files(&root)?;
    files.sort();

    let resources = files
        .iter()
        .map(|path| index_file(&root, path, block_size))
        .collect::<Result<Vec<_>>>()?;

    Ok(ResourceIndex { resources })
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_inner(root, &mut files)?;
    Ok(files)
}

fn collect_files_inner(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read directory: {}", dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry under: {}", dir.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to read metadata: {}", path.display()))?;

        if metadata.is_dir() {
            collect_files_inner(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn index_file(root: &Path, path: &Path, block_size: usize) -> Result<FileResource> {
    let file =
        File::open(path).with_context(|| format!("failed to open file: {}", path.display()))?;
    let file_size = file
        .metadata()
        .with_context(|| format!("failed to read metadata: {}", path.display()))?
        .len();
    let file_name = path
        .strip_prefix(root)
        .with_context(|| format!("failed to build relative file name: {}", path.display()))?
        .to_string_lossy()
        .replace('\\', "/");

    let mut reader = BufReader::new(file);
    let mut buffer = vec![0_u8; block_size];
    let mut file_hasher = Sha256::new();
    let mut blocks = Vec::new();

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read file: {}", path.display()))?;
        if bytes_read == 0 {
            break;
        }

        let chunk = &buffer[..bytes_read];
        file_hasher.update(chunk);
        blocks.push(BlockResource {
            index: blocks.len(),
            hash: sha256_hex(chunk),
            size: bytes_read,
        });
    }

    Ok(FileResource {
        file_hash: hex::encode(file_hasher.finalize()),
        file_name,
        file_size,
        block_size,
        blocks,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
