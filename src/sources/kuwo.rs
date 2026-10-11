//! 酷我音乐: http://www.kuwo.cn

use async_trait::async_trait;
use serde_json::Value;

use crate::core::text::{self, clean_lrc};
use crate::core::{tester, MusicSource, SearchCtx, SongInfo};
use crate::crypto;
use crate::error::Result;
use futures::StreamExt;

pub fn create() -> Box<dyn MusicSource> {
    Box::new(KuwoSource)
}

struct KuwoSource;

/// 可播放格式: (优先级, format 参数)
const QUALITIES: [&str; 2] = ["flac", "mp3"];

#[async_trait]
impl MusicSource for KuwoSource {
    fn name(&self) -> &'static str {
        "kuwo"
    }

    async fn search(&self, keyword: &str, ctx: &SearchCtx) -> Result<Vec<SongInfo>> {
        let mut songs = Vec::new();
        let pages = ctx
            .search_size_per_source
            .div_ceil(ctx.search_size_per_page)
            .max(1);

        for page in 0..pages {
            let resp = ctx
                .client
                .get("http://www.kuwo.cn/search/searchMusicBykeyWord")
                .query(&[
                    ("vipver", "1"),
                    ("client", "kt"),
                    ("ft", "music"),
                    ("cluster", "0"),
                    ("strategy", "2012"),
                    ("encoding", "utf8"),
                    ("rformat", "json"),
                    ("mobi", "1"),
                    ("issubtitle", "1"),
                    ("show_copyright_off", "1"),
                    ("pn", &page.to_string()),
                    ("rn", &ctx.search_size_per_page.to_string()),
                    ("all", keyword),
                ])
                .send()
                .await?;
            let json: Value = resp.json().await?;
            let Some(list) = json.get("abslist").and_then(|v| v.as_array()) else {
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

    async fn parse_playlist(&self, url: &str, ctx: &SearchCtx) -> Result<Vec<SongInfo>> {
        let Some(playlist_id) =
            text::extract_url_param(url, "id").or_else(|| text::extract_path_id(url))
        else {
            return Ok(vec![]);
        };
        // 分页拿歌单曲目
        let mut items: Vec<Value> = Vec::new();
        let mut page = 1u32;
        loop {
            let Ok(resp) = ctx
                .client
                .get("https://m.kuwo.cn/newh5app/wapi/api/www/playlist/playListInfo")
                .query(&[
                    ("pid", playlist_id.as_str()),
                    ("pn", &page.to_string()),
                    ("rn", "100"),
                ])
                .send()
                .await
            else {
                break;
            };
            let Ok(json) = resp.json::<Value>().await else { break };
            let Some(list) = json.pointer("/data/musicList").and_then(|v| v.as_array()) else {
                break;
            };
            if list.is_empty() {
                break;
            }
            items.extend(list.iter().cloned());
            let total = json
                .pointer("/data/total")
                .and_then(|v| v.as_str().and_then(|s| s.parse::<usize>().ok()).or_else(|| v.as_u64().map(|n| n as usize)))
                .unwrap_or(0);
            if items.len() >= total {
                break;
            }
            page += 1;
        }

        let songs = futures::stream::iter(items)
            .map(|item| async move { parse_song(&item, ctx).await })
            .buffered(8)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .flatten()
            .collect();
        Ok(songs)
    }
}

async fn parse_song(item: &Value, ctx: &SearchCtx) -> Option<SongInfo> {
    let rid = item
        .get("MUSICRID")
        .or_else(|| item.get("musicrid"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches("MUSIC_").to_string())?;

    for format in QUALITIES {
        let Some(url) = fetch_download_url(&rid, format, ctx).await else {
            continue;
        };
        let test = tester::test_link(&ctx.client, &url).await;
        if !test.ok {
            continue;
        }
        let mut song = SongInfo {
            source: "kuwo".to_string(),
            identifier: rid.clone(),
            song_name: item
                .get("SONGNAME")
                .or_else(|| item.get("name"))
                .or_else(|| item.get("songName"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            singers: item
                .get("ARTIST")
                .or_else(|| item.get("artist"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            album: item
                .get("ALBUM")
                .or_else(|| item.get("album"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            ext: test.ext,
            file_size_bytes: test.file_size_bytes,
            duration_s: item
                .get("DURATION")
                .or_else(|| item.get("duration"))
                .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_u64())),
            cover_url: item
                .get("hts_MVPIC")
                .or_else(|| item.get("albumpic"))
                .or_else(|| item.get("pic"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            download_url: Some(test.final_url.clone()),
            raw_data: item.clone(),
            ..Default::default()
        };
        supplement_lyric(&mut song, ctx).await;
        return Some(song);
    }
    None
}

/// mobi 接口: DES 加密查询参数, 响应文本中包含下载 URL
async fn fetch_download_url(rid: &str, format: &str, ctx: &SearchCtx) -> Option<String> {
    let query = format!(
        "user=0&corp=kuwo&source=kwplayer_ar_5.1.0.0_B_jiakong_vh.apk&p2p=1&type=convert_url2&sig=0&format={format}&rid={rid}"
    );
    let url = format!(
        "http://mobi.kuwo.cn/mobi.s?f=kuwo&q={}",
        crypto::kuwo::encrypt_query(&query)
    );
    let resp = ctx
        .client
        .get(&url)
        .header("User-Agent", "okhttp/3.10.0")
        .send()
        .await
        .ok()?;
    let text = resp.text().await.ok()?;
    extract_url(&text)
}

/// 从响应文本提取第一个 http 链接 (等价于 re.search(r'http[^\s$"]+'))
fn extract_url(text: &str) -> Option<String> {
    let start = text.find("http")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '$' || c == '"')
        .unwrap_or(rest.len());
    let url = rest[..end].trim();
    (!url.is_empty()).then(|| url.to_string())
}

async fn supplement_lyric(song: &mut SongInfo, ctx: &SearchCtx) {
    let url = format!(
        "http://newlyric.kuwo.cn/newlyric.lrc?{}",
        crypto::kuwo::build_lyrics_params(&song.identifier)
    );
    let Ok(resp) = ctx.client.get(&url).send().await else {
        return;
    };
    let Ok(bytes) = resp.bytes().await else {
        return;
    };
    if let Some(raw) = crypto::kuwo::decode_lyrics(&bytes) {
        let lyric = clean_lrc(&crypto::kuwo::convert_raw_lrc(&raw));
        if !lyric.trim().is_empty() {
            song.lyric = Some(lyric);
        }
    }
}
