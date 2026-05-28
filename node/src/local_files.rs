use std::{
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
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

pub struct LocalBlockData {
    pub file_hash: String,
    pub block_index: usize,
    pub block_hash: String,
    pub size: usize,
    pub bytes: Vec<u8>,
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

pub fn read_block_by_hash(
    root: &Path,
    block_size: usize,
    file_hash: &str,
    block_index: usize,
) -> Result<Option<LocalBlockData>> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve share directory: {}", root.display()))?;
    let index = scan_share_dir(&root, block_size)?;
    let Some(resource) = index
        .resources
        .into_iter()
        .find(|resource| resource.file_hash == file_hash)
    else {
        return Ok(None);
    };

    let Some(block) = resource
        .blocks
        .iter()
        .find(|block| block.index == block_index)
    else {
        return Ok(None);
    };

    let path = root.join(resource.file_name);
    let mut file =
        File::open(&path).with_context(|| format!("failed to open file: {}", path.display()))?;
    file.seek(SeekFrom::Start((block_index * block_size) as u64))
        .with_context(|| format!("failed to seek file: {}", path.display()))?;

    let mut bytes = vec![0_u8; block.size];
    file.read_exact(&mut bytes)
        .with_context(|| format!("failed to read block from file: {}", path.display()))?;

    Ok(Some(LocalBlockData {
        file_hash: file_hash.to_string(),
        block_index,
        block_hash: sha256_hex(&bytes),
        size: bytes.len(),
        bytes,
    }))
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
