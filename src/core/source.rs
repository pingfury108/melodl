use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{ProgressEvent, SongInfo};
use crate::error::Result;

/// 搜索上下文: 各源共享的 HTTP 客户端与配置
pub struct SearchCtx {
    pub client: reqwest::Client,
    pub search_size_per_source: usize,
    pub search_size_per_page: usize,
    pub max_retries: usize,
    pub events: Option<mpsc::Sender<ProgressEvent>>,
}

impl SearchCtx {
    pub async fn emit(&self, ev: ProgressEvent) {
        if let Some(tx) = &self.events {
            let _ = tx.send(ev).await;
        }
    }
}

#[async_trait]
pub trait MusicSource: Send + Sync {
    fn name(&self) -> &'static str;

    async fn search(&self, keyword: &str, ctx: &SearchCtx) -> Result<Vec<SongInfo>>;

    async fn parse_playlist(&self, _url: &str, _ctx: &SearchCtx) -> Result<Vec<SongInfo>> {
        Ok(vec![])
    }
}
