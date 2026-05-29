use std::time::{SystemTime, UNIX_EPOCH};
use sha2::{Digest, Sha256};

pub fn generate_node_id(host: &str, port: u16) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let dummy = Box::new(42);
    let memory_entropy = &*dummy as *const i32 as usize;

    let raw_id = format!("{}:{}-{}-{}", host, port, timestamp, memory_entropy);

    let mut hasher = Sha256::new();
    hasher.update(raw_id.as_bytes());
    let hash_result = hasher.finalize();

    let hex_hash = format!("{:x}", hash_result);
    format!("node_{}", &hex_hash[..16])
}