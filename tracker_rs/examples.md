
#### File announce with correct block count
```bash
curl -v -X POST 127.0.0.1:3000/api/v2/file_announce --json '{
    "file_hash": "181210f8f9c779c26da1d9b2075bde0127302ee0e3fca38c9a83f5b1dd8e5d3b", 
    "file_name": "file_name_test_1.txt",
    "file_size": 3500, 
    "block_size": 1024, 
    "block_hashes": ["BLOCK-1-SHA256", "BLOCK-2-SHA256", "BLOCK-3-SHA256", "BLOCK-4-SHA256"]
}'
```

#### File announce with incorrect block count
```bash
curl -v -X POST 127.0.0.1:3000/api/v2/file_announce --json '{
    "file_hash": "a883dafc480d466ee04e0d6da986bd78eb1fdd2178d04693723da3a8f95d42f4", 
    "file_name": "file_name_test_2.txt",
    "file_size": 3500, 
    "block_size": 1024, 
    "block_hashes": ["BLOCK-1-SHA256", "BLOCK-2-SHA256"]
}'
```

#### Get file list
```bash
curl -v 127.0.0.1:3000/api/v2/file_list
```

#### Query file by file hash
```bash
curl -v 127.0.0.1:3000/api/v2/query?file_id=file_name_test_1.txt
```

#### Peer updates (correct)
```bash
curl -v -X POST 127.0.0.1:3000/api/v2/update --json '{
    "file_hash": "181210f8f9c779c26da1d9b2075bde0127302ee0e3fca38c9a83f5b1dd8e5d3b", 
    "peer_id": "PEER_ID_1",
    "peer_host": null,
    "peer_port": 1145,
    "blocks": "1100"
}'
```

#### Peer updates (incorrect)
```bash
curl -v -X POST 127.0.0.1:3000/api/v2/update --json '{
    "file_hash": "FILE_HASH_TEST_1", 
    "peer_id": "PEER_ID_2",
    "peer_host": null,
    "peer_port": 1145,
    "blocks": "abcd"
}'
```

#### Get peers
```bash
curl -v 127.0.0.1:3000/api/v2/peer_list?file_hash=FILE_HASH_TEST_1
```

pub file_name: String,
pub file_hash: String,
pub file_size: u64,
pub block_size: u64,
// Hash for each block
pub block_hashes: Vec<String>


pub file_hash: String,
pub peer_id: String,
/// If not specified, use HTTP client ID
pub peer_host: Option<String>,
pub peer_port: u16,
/// A 0-1 string
pub blocks: String