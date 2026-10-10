//! 酷狗音乐: http://www.kugou.com
//! 搜索 song_search_v2; 下载: 老移动接口 getSongInfo(免费歌直链) + baka.plus 兜底

use async_trait::async_trait;
use base64::Engine;
use serde_json::Value;

use crate::core::text::{clean_lrc, non_empty};
use crate::core::{tester, MusicSource, SearchCtx, SongInfo};
use crate::error::Result;

pub fn create() -> Box<dyn MusicSource> {
    Box::new(KugouSource)
}

struct KugouSource;

#[async_trait]
impl MusicSource for KugouSource {
    fn name(&self) -> &'static str {
        "kugou"
    }

    async fn search(&self, keyword: &str, ctx: &SearchCtx) -> Result<Vec<SongInfo>> {
        let mut songs = Vec::new();
        let pages = ctx
            .search_size_per_source
            .div_ceil(ctx.search_size_per_page)
            .max(1);

        for page in 1..=pages {
            let resp = ctx
                .client
                .get("https://songsearch.kugou.com/song_search_v2")
                .query(&[
                    ("format", "json"),
                    ("keyword", keyword),
                    ("platform", "WebFilter"),
                    ("page", &page.to_string()),
                    ("pagesize", &ctx.search_size_per_page.to_string()),
                ])
                .send()
                .await?;
            let json: Value = resp.json().await?;
            let Some(list) = json.pointer("/data/lists").and_then(|v| v.as_array()) else {
                break;
            };
            if list.is_empty() {
                break;
            }
            for item in list {
                if let Some(song) = parse_song(item, ctx).await {
                    songs.push(song);
                }
                if songs.len() >= ctx.search_size_per_source {
                    return Ok(songs);
                }
            }
            if list.len() < ctx.search_size_per_page {
                break;
            }
        }
        Ok(songs)
    }
}

async fn parse_song(item: &Value, ctx: &SearchCtx) -> Option<SongInfo> {
    let hash = item
        .get("hash")
        .or_else(|| item.get("FileHash"))
        .and_then(|v| v.as_str())?;

    // 1) 老移动接口: 免费歌直接返回 url
    let api = format!("https://m.kugou.com/app/i/getSongInfo.php?cmd=playInfo&hash={hash}");
    if let Ok(resp) = ctx.client.get(&api).send().await {
        if let Ok(json) = resp.json::<Value>().await {
            if let Some(url) = json
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|u| u.starts_with("http"))
            {
                let test = tester::test_link(&ctx.client, url).await;
                if test.ok {
                    return Some(build_song(item, hash, &test, ctx).await);
                }
            }
        }
    }

    // 2) baka.plus 兜底: 302 跳转直链 (部分网络环境被 TLS 指纹风控)
    let api = format!("https://api.baka.plus/meting/?server=kugou&type=url&id={hash}&br=2000");
    let test = tester::test_link(&ctx.client, &api).await;
    if !test.ok {
        return None;
    }
    Some(build_song(item, hash, &test, ctx).await)
}

async fn build_song(
    item: &Value,
    hash: &str,
    test: &tester::LinkTestResult,
    ctx: &SearchCtx,
) -> SongInfo {
    let mut song = SongInfo {
        source: "kugou".to_string(),
        identifier: hash.to_string(),
        song_name: item
            .get("songname")
            .or_else(|| item.get("SongName"))
            .or_else(|| item.get("OriSongName"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        singers: item
            .get("singername")
            .or_else(|| item.get("SingerName"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        album: non_empty(
            item.get("album_name")
                .or_else(|| item.get("AlbumName"))
                .and_then(|v| v.as_str()),
        ),
        ext: test.ext.clone(),
        file_size_bytes: test.file_size_bytes,
        duration_s: item
            .get("duration")
            .or_else(|| item.get("Duration"))
            .and_then(|v| v.as_u64())
            .or_else(|| item.get("timelen").and_then(|v| v.as_u64()).map(|ms| ms / 1000)),
        cover_url: item
            .pointer("/trans_param/union_cover")
            .and_then(|v| v.as_str())
            .map(|u| u.replace("{size}", "300")),
        download_url: Some(test.final_url.clone()),
        raw_data: item.clone(),
        ..Default::default()
    };
    supplement_lyric(&mut song, item, ctx).await;
    song
}

async fn supplement_lyric(song: &mut SongInfo, item: &Value, ctx: &SearchCtx) {
    let filename = item
        .get("filename")
        .or_else(|| item.get("FileName"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let duration = song.duration_s.unwrap_or(0).to_string();

    // 搜歌词候选 → 下载 lrc (base64)
    let Ok(resp) = ctx
        .client
        .get("https://krcs.kugou.com/search")
        .query(&[
            ("keyword", filename),
            ("duration", duration.as_str()),
            ("hash", song.identifier.as_str()),
            ("ver", "1"),
            ("man", "yes"),
            ("client", "mobi"),
        ])
        .send()
        .await
    else {
        return;
    };
    let Ok(json) = resp.json::<Value>().await else { return };
    let candidate = json
        .pointer("/candidates/0")
        .cloned()
        .unwrap_or(Value::Null);
    let (id, accesskey) = match (
        candidate.get("id").and_then(|v| v.as_str()),
        candidate.get("accesskey").and_then(|v| v.as_str()),
    ) {
        (Some(i), Some(k)) => (i.to_string(), k.to_string()),
        _ => return,
    };
    let Ok(resp) = ctx
        .client
        .get("https://lyrics.kugou.com/download")
        .query(&[
            ("ver", "1"),
            ("client", "pc"),
            ("id", id.as_str()),
            ("accesskey", accesskey.as_str()),
            ("fmt", "lrc"),
            ("charset", "utf8"),
        ])
        .send()
        .await
    else {
        return;
    };
    let Ok(json) = resp.json::<Value>().await else { return };
    if let Some(content) = json.get("content").and_then(|v| v.as_str()) {
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(content) {
            if let Ok(text) = String::from_utf8(bytes) {
                let lyric = clean_lrc(&text);
                if !lyric.trim().is_empty() {
                    song.lyric = Some(lyric);
                }
            }
        }
    }
}
