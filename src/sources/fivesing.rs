//! 5SING 音乐: https://5sing.kugou.com

use async_trait::async_trait;
use serde_json::Value;

use crate::core::text::{clean_lrc, strip_html_tags};
use crate::core::{tester, MusicSource, SearchCtx, SongInfo};
use crate::error::Result;

pub fn create() -> Box<dyn MusicSource> {
    Box::new(FiveSingSource)
}

struct FiveSingSource;

// 音质优先级: sq(无损) > hq > lq
const QUALITIES: [&str; 3] = ["sq", "hq", "lq"];

#[async_trait]
impl MusicSource for FiveSingSource {
    fn name(&self) -> &'static str {
        "fivesing"
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
                .get("http://search.5sing.kugou.com/home/json")
                .header("Referer", "https://5sing.kugou.com/")
                .query(&[
                    ("keyword", keyword),
                    ("sort", "1"),
                    ("page", &page.to_string()),
                    ("filter", "0"),
                    ("type", "0"),
                ])
                .send()
                .await?;
            let json: Value = resp.json().await?;
            let Some(list) = json.get("list").and_then(|v| v.as_array()) else {
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
    let song_id = item.get("songId")?.as_i64()?;
    let kind = item.get("typeEname")?.as_str()?;

    // 获取下载链接, 按音质降级
    let resp = ctx
        .client
        .get("http://mobileapi.5sing.kugou.com/song/getSongUrl")
        .header("Referer", "https://5sing.kugou.com/")
        .query(&[
            ("songid", song_id.to_string()),
            ("songtype", kind.to_string()),
        ])
        .send()
        .await
        .ok()?;
    let data: Value = resp.json().await.ok()?;

    for quality in QUALITIES {
        let url = data
            .pointer(&format!("/data/{}url", quality))
            .and_then(|v| v.as_str())
            .filter(|u| u.starts_with("http"))
            .or_else(|| {
                data.pointer(&format!("/data/{}url_backup", quality))
                    .and_then(|v| v.as_str())
                    .filter(|u| u.starts_with("http"))
            });
        let Some(url) = url else { continue };

        let test = tester::test_link(&ctx.client, url).await;
        if !test.ok {
            continue;
        }
        let mut song = SongInfo {
            source: "fivesing".to_string(),
            identifier: song_id.to_string(),
            song_name: strip_html_tags(
                item.get("songName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            ),
            singers: strip_html_tags(
                item.get("singer")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            ),
            ext: test.ext,
            file_size_bytes: test.file_size_bytes,
            download_url: Some(test.final_url.clone()),
            raw_data: item.clone(),
            ..Default::default()
        };
        supplement_lyric(&mut song, kind, ctx).await;
        return Some(song);
    }
    None
}

/// 补充歌词/专辑/封面
async fn supplement_lyric(song: &mut SongInfo, kind: &str, ctx: &SearchCtx) {
    let Ok(resp) = ctx
        .client
        .get("http://mobileapi.5sing.kugou.com/song/newget")
        .header("Referer", "https://5sing.kugou.com/")
        .query(&[
            ("songid", song.identifier.clone()),
            ("songtype", kind.to_string()),
            ("songfields", String::new()),
            ("userfields", String::new()),
        ])
        .send()
        .await
    else {
        return;
    };
    let Ok(json) = resp.json::<Value>().await else {
        return;
    };
    song.lyric = json
        .pointer("/data/dynamicWords")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(clean_lrc);
    song.album = json
        .pointer("/data/albumName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    song.cover_url = json
        .pointer("/data/user/I")
        .and_then(|v| v.as_str())
        .map(str::to_string);
}
