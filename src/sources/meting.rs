//! Meting 聚合源: https://api.qijieya.cn/meting (聚合 netease/kugou/migu)

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;

use crate::core::text::{clean_lrc, extract_url_param};
use crate::core::{tester, MusicSource, SearchCtx, SongInfo};
use crate::error::Result;

pub fn create() -> Box<dyn MusicSource> {
    Box::new(MetingSource)
}

struct MetingSource;

const API: &str = "https://api.qijieya.cn/meting/";
/// (上游名, meting server 参数); qq/kuwo 当前无数据
const SERVERS: [(&str, &str); 3] = [("netease", "netease"), ("kugou", "kugou"), ("migu", "migu")];

#[async_trait]
impl MusicSource for MetingSource {
    fn name(&self) -> &'static str {
        "meting"
    }

    async fn search(&self, keyword: &str, ctx: &SearchCtx) -> Result<Vec<SongInfo>> {
        // 各上游并发搜索
        let mut tasks = FuturesUnordered::new();
        for (root, server) in SERVERS {
            let client = ctx.client.clone();
            tasks.push(async move {
                client
                    .get(API)
                    .query(&[("server", server), ("type", "search"), ("id", keyword)])
                    .send()
                    .await
                    .ok()?
                    .json::<Value>()
                    .await
                    .ok()
                    .and_then(|v| v.as_array().cloned())
                    .map(|items| (root, items))
            });
        }

        let mut all: Vec<(&str, Vec<Value>)> = Vec::new();
        while let Some(Some(pair)) = tasks.next().await {
            all.push(pair);
        }

        // 轮流从各上游取, 保证多样性, 总量 cap 到 search_size_per_source
        let mut songs = Vec::new();
        let per_source = ctx.search_size_per_source.div_ceil(all.len().max(1));
        for (root, items) in all {
            let futs: Vec<_> = items
                .into_iter()
                .take(per_source)
                .map(|item| parse_song(root, item, ctx))
                .collect();
            let parsed: Vec<SongInfo> = futures::stream::iter(futs)
                .buffer_unordered(5)
                .filter_map(|x| async { x })
                .collect()
                .await;
            songs.extend(parsed);
        }
        songs.truncate(ctx.search_size_per_source);
        Ok(songs)
    }
}

async fn parse_song(root: &'static str, item: Value, ctx: &SearchCtx) -> Option<SongInfo> {
    let url_field = item.get("url")?.as_str()?;
    let song_id = extract_url_param(url_field, "id")?;
    let server = SERVERS.iter().find(|(r, _)| r == &root).map(|(_, s)| *s)?;

    // 真实下载地址 (302 跳转)
    let api = format!("{API}?server={server}&type=url&id={song_id}&br=999");
    let test = tester::test_link(&ctx.client, &api).await;
    if !test.ok {
        return None;
    }

    // 歌词
    let mut lyric = None;
    if let Ok(resp) = ctx
        .client
        .get(API)
        .query(&[("server", server), ("type", "lrc"), ("id", &song_id)])
        .send()
        .await
    {
        if let Ok(text) = resp.text().await {
            let l = clean_lrc(&text);
            if !l.trim().is_empty() {
                lyric = Some(l);
            }
        }
    }

    Some(SongInfo {
        source: "meting".to_string(),
        root_source: Some(root.to_string()),
        identifier: song_id.clone(),
        song_name: item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        singers: item
            .get("artist")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        ext: test.ext,
        file_size_bytes: test.file_size_bytes,
        lyric,
        cover_url: item.get("pic").and_then(|v| v.as_str()).map(str::to_string),
        download_url: Some(test.final_url.clone()),
        raw_data: item,
        ..Default::default()
    })
}
