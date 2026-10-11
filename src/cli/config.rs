//! 配置文件: ~/.config/melodl/melodl.toml

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// 默认音乐源
    pub sources: Option<Vec<String>>,
    /// 下载目录
    pub work_dir: Option<String>,
    /// 每源搜索数量
    pub limit: Option<usize>,
    /// 下载并发数
    pub concurrency: Option<usize>,
}

impl Config {
    pub fn load() -> Self {
        let Some(path) = Self::path() else { return Self::default() };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("配置文件 {} 解析失败: {}, 使用默认配置", path.display(), e);
            Self::default()
        })
    }

    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("melodl").join("melodl.toml"))
    }
}

/// 展开 ~ 前缀
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}
