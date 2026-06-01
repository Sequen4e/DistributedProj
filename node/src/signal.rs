use serde::Serialize;

#[derive(Clone, Debug, PartialEq)]
pub enum BroadcastSignal {
    /// 触发全网优雅退出
    Shutdown,
    /// 某个任务进度更新通知：(当前完成块数, 总块数)
    ProgressUpdate { downloaded_blocks: usize, total_blocks: u64 },
    /// 静态任务信息初始化通知：(文件名, 文件大小, 哈希)
    TaskInfo { file_name: String, file_size: u64, file_hash: String },
}