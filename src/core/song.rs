use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    Http,
    Hls,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SongInfo {
    pub source: String,
    /// 聚合源的上游来源 (如 meting -> netease)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_source: Option<String>,
    pub identifier: String,
    pub song_name: String,
    pub singers: String,
    pub album: Option<String>,
    pub ext: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub duration_s: Option<u64>,
    pub lyric: Option<String>,
    pub cover_url: Option<String>,
    pub download_url: Option<String>,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(skip)]
    pub raw_data: serde_json::Value,
}

impl SongInfo {
    pub fn with_valid_download_url(&self) -> bool {
        self.download_url
            .as_deref()
            .map(|u| u.starts_with("http"))
            .unwrap_or(false)
            && self.ext.is_some()
    }

    pub fn file_size_display(&self) -> String {
        match self.file_size_bytes {
            Some(b) if b > 0 => format!("{:.2}MB", b as f64 / 1024.0 / 1024.0),
            _ => "-".to_string(),
        }
    }

    pub fn duration_display(&self) -> String {
        match self.duration_s {
            Some(s) => format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60),
            None => "-:-:-".to_string(),
        }
    }

    pub fn file_name(&self) -> String {
        let ext = self.ext.as_deref().unwrap_or("mp3").trim_start_matches('.');
        sanitize_filename::sanitize(format!("{} - {}.{}", self.song_name, self.identifier, ext))
    }

    /// 重名时追加 (n), 路径过长时截断文件名
    pub fn save_path(&self, dir: &std::path::Path) -> std::path::PathBuf {
        let mut path = dir.join(self.file_name());
        let mut idx = 1;
        while path.exists() {
            let stem = format!("{} - {} ({})", self.song_name, self.identifier, idx);
            let ext = self.ext.as_deref().unwrap_or("mp3").trim_start_matches('.');
            path = dir.join(sanitize_filename::sanitize(format!("{}.{}", stem, ext)));
            idx += 1;
        }
        path
    }
}
