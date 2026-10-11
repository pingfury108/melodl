use std::path::PathBuf;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::mpsc;

use crate::core::{downloader, registry, MusicSource, ProgressEvent, SearchCtx, SongInfo};
use crate::error::{MelodlError, Result};

pub struct MusicClient {
    sources: Vec<Box<dyn MusicSource>>,
    work_dir: PathBuf,
    search_size_per_source: usize,
    search_size_per_page: usize,
    max_retries: usize,
    concurrency: usize,
    http: reqwest::Client,
}

pub struct MusicClientBuilder {
    source_names: Option<Vec<String>>,
    work_dir: PathBuf,
    search_size_per_source: usize,
    search_size_per_page: usize,
    max_retries: usize,
    concurrency: usize,
}

impl MusicClient {
    pub fn builder() -> MusicClientBuilder {
        MusicClientBuilder {
            source_names: None,
            work_dir: PathBuf::from("./music"),
            search_size_per_source: 10,
            search_size_per_page: 10,
            max_retries: 3,
            concurrency: 5,
        }
    }

    pub fn source_names(&self) -> Vec<&str> {
        self.sources.iter().map(|s| s.name()).collect()
    }

    pub async fn search(&self, keyword: &str) -> Vec<SongInfo> {
        self.search_inner(keyword, None).await
    }

    pub async fn search_with_events(
        &self,
        keyword: &str,
        tx: mpsc::Sender<ProgressEvent>,
    ) -> Vec<SongInfo> {
        self.search_inner(keyword, Some(tx)).await
    }

    /// 所有源并发搜索, 单源失败不影响其他源
    async fn search_inner(
        &self,
        keyword: &str,
        events: Option<mpsc::Sender<ProgressEvent>>,
    ) -> Vec<SongInfo> {
        let mut tasks = FuturesUnordered::new();
        for source in &self.sources {
            let ctx = SearchCtx {
                client: self.http.clone(),
                search_size_per_source: self.search_size_per_source,
                search_size_per_page: self.search_size_per_page.min(self.search_size_per_source),
                max_retries: self.max_retries,
                events: events.clone(),
            };
            let keyword = keyword.to_string();
            tasks.push(async move {
                let name = source.name().to_string();
                ctx.emit(ProgressEvent::SearchStarted {
                    source: name.clone(),
                })
                .await;
                match source.search(&keyword, &ctx).await {
                    Ok(songs) => {
                        ctx.emit(ProgressEvent::SearchFinished {
                            source: name,
                            count: songs.len(),
                        })
                        .await;
                        songs
                    }
                    Err(e) => {
                        ctx.emit(ProgressEvent::Error {
                            source: name,
                            message: e.to_string(),
                        })
                        .await;
                        vec![]
                    }
                }
            });
        }

        let mut all = Vec::new();
        while let Some(songs) = tasks.next().await {
            all.extend(songs);
        }
        dedup(all)
    }

    /// 解析歌单 URL: 优先 host 匹配的源(未配置时临时构建), 再依次尝试其他源
    pub async fn parse_playlist(&self, url: &str) -> Vec<SongInfo> {
        let ctx = SearchCtx {
            client: self.http.clone(),
            search_size_per_source: self.search_size_per_source,
            search_size_per_page: self.search_size_per_page,
            max_retries: self.max_retries,
            events: None,
        };
        let host = reqwest::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();

        // host 匹配的源名(即使未在配置中也启用)
        let matched_name = ["netease", "qq", "kuwo", "kugou", "migu", "fivesing"]
            .into_iter()
            .find(|n| host_matches_source(&host, n));
        if let Some(name) = matched_name {
            if let Some(src) = registry::build(name) {
                match src.parse_playlist(url, &ctx).await {
                    Ok(songs) if !songs.is_empty() => return songs,
                    Ok(_) => {}
                    Err(e) => tracing::debug!("{} parse_playlist failed: {}", name, e),
                }
            }
        }
        for source in &self.sources {
            if Some(source.name()) == matched_name {
                continue;
            }
            match source.parse_playlist(url, &ctx).await {
                Ok(songs) if !songs.is_empty() => return songs,
                Ok(_) => {}
                Err(e) => tracing::debug!("{} parse_playlist failed: {}", source.name(), e),
            }
        }
        vec![]
    }

    pub async fn download(&self, songs: &[SongInfo]) -> Vec<PathBuf> {
        self.download_inner(songs, None).await
    }

    pub async fn download_with_events(
        &self,
        songs: &[SongInfo],
        tx: mpsc::Sender<ProgressEvent>,
    ) -> Vec<PathBuf> {
        self.download_inner(songs, Some(tx)).await
    }

    async fn download_inner(
        &self,
        songs: &[SongInfo],
        events: Option<mpsc::Sender<ProgressEvent>>,
    ) -> Vec<PathBuf> {
        let tasks = songs
            .iter()
            .filter(|s| s.with_valid_download_url())
            .map(|song| {
                let client = self.http.clone();
                let dir = self.work_dir.clone();
                let events = events.clone();
                async move { downloader::download_song(&client, song, &dir, events.as_ref()).await }
            });

        let mut paths = Vec::new();
        let mut stream = futures::stream::iter(tasks).buffer_unordered(self.concurrency);
        while let Some(res) = stream.next().await {
            match res {
                Ok(p) => paths.push(p),
                Err(e) => tracing::warn!("download failed: {}", e),
            }
        }
        paths
    }
}

impl MusicClientBuilder {
    pub fn sources<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.source_names = Some(names.into_iter().map(Into::into).collect());
        self
    }

    pub fn work_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.work_dir = dir.into();
        self
    }

    pub fn search_size_per_source(mut self, n: usize) -> Self {
        self.search_size_per_source = n.max(1);
        self
    }

    pub fn max_retries(mut self, n: usize) -> Self {
        self.max_retries = n.max(1);
        self
    }

    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = n.max(1);
        self
    }

    pub fn build(self) -> Result<MusicClient> {
        let names = self
            .source_names
            .unwrap_or_else(|| vec!["fivesing".to_string()]);
        let mut sources: Vec<Box<dyn MusicSource>> = Vec::new();
        for name in &names {
            match registry::build(name) {
                Some(s) => sources.push(s),
                None => return Err(MelodlError::UnknownSource(name.clone())),
            }
        }
        let http = reqwest::Client::builder()
            .user_agent(default_ua())
            .no_proxy() // 直连, 避免环境代理干扰第三方音乐接口
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(MusicClient {
            sources,
            work_dir: self.work_dir,
            search_size_per_source: self.search_size_per_source,
            search_size_per_page: self.search_size_per_page,
            max_retries: self.max_retries,
            concurrency: self.concurrency,
            http,
        })
    }
}

/// 按 source+root_source+identifier 去重, 保序
fn dedup(songs: Vec<SongInfo>) -> Vec<SongInfo> {
    let mut seen = std::collections::HashSet::new();
    songs
        .into_iter()
        .filter(|s| {
            seen.insert((
                s.source.clone(),
                s.root_source.clone().unwrap_or_default(),
                s.identifier.clone(),
            )) && s.with_valid_download_url()
        })
        .collect()
}

fn default_ua() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36".to_string()
}

fn host_matches_source(host: &str, source: &str) -> bool {
    match source {
        "netease" => host.ends_with("163.com"),
        "qq" => host.ends_with("qq.com"),
        "kugou" => host.contains("kugou.com"),
        "kuwo" => host.contains("kuwo.cn"),
        "migu" => host.contains("migu.cn"),
        "fivesing" => host.contains("5sing"),
        _ => false,
    }
}
