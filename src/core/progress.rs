use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ProgressEvent {
    SearchStarted { source: String },
    SearchFinished { source: String, count: usize },
    DownloadStarted { id: String, total: Option<u64> },
    DownloadProgress { id: String, downloaded: u64, total: Option<u64> },
    DownloadFinished { id: String, path: String },
    Error { source: String, message: String },
}
