use serde::Serialize;

#[derive(Clone, Debug)]
pub enum BroadcastSignal {
    Shutdown,
    TaskInfo { file_name: String, file_size: u64, file_hash: String },
    ProgressUpdate { downloaded_blocks: usize, total_blocks: usize },
    ConsoleLog(String),
}
