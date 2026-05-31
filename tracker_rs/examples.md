
#### File announce with correct block count
```bash
curl -v -X POST 127.0.0.1:3000/api/v2/file_announce --json '{
    "file_hash": "FILE_HASH_TEST_1", 
    "file_name": "file_name_test_1.txt",
    "file_size": 3500, 
    "block_size": 1024, 
    "block_hashes": ["BLOCK-1-SHA256", "BLOCK-2-SHA256", "BLOCK-3-SHA256", "BLOCK-4-SHA256"]
}'
```

#### File announce with incorrect block count
```bash
curl -v -X POST 127.0.0.1:3000/api/v2/file_announce --json '{
    "file_hash": "FILE_HASH_TEST_2", 
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

### Query file by file hash
```bash
curl -v 127.0.0.1:3000/api/v2/query?file_id=file_name_test_1.txt
```

pub file_name: String,
    pub file_hash: String,
    pub file_size: u64,
    pub block_size: u64,
    // Hash for each block
    pub block_hashes: Vec<String>