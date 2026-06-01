use chrono::Utc;
use rand::Rng;
use sha2::{Digest, Sha256};

pub fn generate_node_id(port: u16) -> String {
    let timestamp_us = Utc::now().timestamp_micros();

    let mut random = [0u8; 16];

    rand::rng().fill_bytes(&mut random);

    let raw_id = format!("{}-{}", port, timestamp_us);

    let mut hasher = Sha256::new();
    hasher.update(raw_id.as_bytes());
    hasher.update(&random);
    let hash_result = hasher.finalize();

    let hex_hash = format!("{:x}", hash_result);
    format!("node_{}", &hex_hash[..16])
}