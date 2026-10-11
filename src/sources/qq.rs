//! QQ 音乐: https://y.qq.com
//! 搜索走 shc.y.qq.com/soso 老接口 (u.y.qq.com 搜索端点有 TLS 指纹风控)
//! 下载链接走 u.y.qq.com musicu.fcg vkey 接口

use async_trait::async_trait;
use base64::Engine;
use serde_json::Value;

use crate::core::text::{self, clean_lrc, non_empty};
use crate::core::{tester, MusicSource, SearchCtx, SongInfo};
use crate::crypto::qq;
use crate::error::Result;
use futures::StreamExt;

pub fn create() -> Box<dyn MusicSource> {
    Box::new(QqSource)
}

struct QqSource;

/// 可播放音质 (前缀, 后缀), 从高到低; 加密 mflac 格式不支持
const QUALITIES: [(&str, &str); 13] = [
    ("AI00", ".flac"),
    ("Q000", ".flac"),
    ("Q001", ".flac"),
    ("F000", ".flac"),
    ("O801", ".ogg"),
    ("O800", ".ogg"),
    ("O600", ".ogg"),
    ("O400", ".ogg"),
    ("M800", ".mp3"),
    ("M500", ".mp3"),
    ("C600", ".m4a"),
    ("C400", ".m4a"),
    ("C200", ".m4a"),
];

#[async_trait]
impl MusicSource for QqSource {
    fn name(&self) -> &'static str {
        "qq"
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
                .get("https://shc.y.qq.com/soso/fcgi-bin/client_search_cp")
                .header("Referer", "https://y.qq.com/")
                .query(&[
                    ("ct", "24"),
                    ("qqmusic_ver", "1298"),
                    ("remoteplace", "txt.yqq.center"),
                    ("searchid", qq::random_search_id().as_str()),
                    ("t", "0"),
                    ("aggr", "1"),
                    ("cr", "1"),
                    ("catZhida", "1"),
                    ("lossless", "0"),
                    ("flag_qc", "0"),
                    ("p", &page.to_string()),
                    ("n", &ctx.search_size_per_page.to_string()),
                    ("w", keyword),
                    ("g_tk", "5381"),
                    ("loginUin", "0"),
                    ("hostUin", "0"),
                    ("format", "json"),
                    ("inCharset", "utf8"),
                    ("outCharset", "utf-8"),
                    ("notice", "0"),
                    ("platform", "yqq"),
                    ("needNewCode", "0"),
                ])
                .send()
                .await?;
            let json: Value = resp.json().await?;
            let Some(list) = json.pointer("/data/song/list").and_then(|v| v.as_array()) else {
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
        let resp = ctx
            .client
            .get("https://c.y.qq.com/qzone/fcg-bin/fcg_ucc_getcdinfo_byids_cp.fcg")
            .header(
                "Referer",
                format!("https://y.qq.com/n/ryqq/playlist/{playlist_id}"),
            )
            .query(&[
                ("disstid", playlist_id.as_str()),
                ("type", "1"),
                ("json", "1"),
                ("utf8", "1"),
                ("onlysong", "0"),
                ("format", "json"),
            ])
            .send()
            .await?;
        let json: Value = resp.json().await?;
        let list = json
            .pointer("/cdlist/0/songlist")
            .or_else(|| json.pointer("/cdlist/0/list"))
            .or_else(|| json.pointer("/songlist"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let songs = futures::stream::iter(list)
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
    let mid = item
        .get("songmid")
        .or_else(|| item.get("mid"))
        .and_then(|v| v.as_str())?;

    for (prefix, suffix) in QUALITIES {
        let params = serde_json::json!({
            "filename": [format!("{prefix}{mid}{mid}{suffix}")],
            "guid": qq::random_guid(),
            "songmid": [mid],
            "songtype": [0],
        });
        let body = qq::build_request_data(
            params,
            "music.vkey.GetVkey",
            "UrlGetVkey",
            qq::FALLBACK_Q36,
            Some(serde_json::json!({"ct": "19"})),
        );
        let resp = ctx
            .client
            .post(qq::ENDPOINT)
            .header("Referer", "https://y.qq.com/")
            .json(&body)
            .send()
            .await;
        let Ok(resp) = resp else { continue };
        let Ok(json) = resp.json::<Value>().await else {
            continue;
        };
        let purl = json
            .pointer("/music.vkey.GetVkey.UrlGetVkey/data/midurlinfo/0/purl")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                json.pointer("/music.vkey.GetVkey.UrlGetVkey/data/midurlinfo/0/wifiurl")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            });
        let Some(purl) = purl else { continue };
        let url = format!("{}{}", qq::MUSIC_DOMAIN, purl);

        let test = tester::test_link(&ctx.client, &url).await;
        if !test.ok {
            continue;
        }
        let mut song = SongInfo {
            source: "qq".to_string(),
            identifier: mid.to_string(),
            song_name: crate::core::text::strip_html_tags(
                item.get("songname_hilight")
                    .or_else(|| item.get("songname"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            ),
            singers: item
                .get("singer")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "unknown".to_string()),
            album: non_empty(item.get("albumname").and_then(|v| v.as_str())),
            ext: test.ext,
            file_size_bytes: test.file_size_bytes,
            duration_s: item.get("interval").and_then(|v| v.as_u64()),
            cover_url: item.get("albummid").and_then(|v| v.as_str()).map(|amid| {
                format!("https://y.gtimg.cn/music/photo_new/T002R800x800M000{amid}.jpg")
            }),
            download_url: Some(test.final_url.clone()),
            raw_data: item.clone(),
            ..Default::default()
        };
        supplement_lyric(&mut song, ctx).await;
        return Some(song);
    }
    None
}

async fn supplement_lyric(song: &mut SongInfo, ctx: &SearchCtx) {
    let Ok(resp) = ctx
        .client
        .get("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg")
        .header("Referer", "https://y.qq.com/portal/player.html")
        .query(&[
            ("songmid", song.identifier.as_str()),
            ("g_tk", "5381"),
            ("loginUin", "0"),
            ("hostUin", "0"),
            ("format", "json"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("platform", "yqq"),
        ])
        .send()
        .await
    else {
        return;
    };
    let Ok(json) = resp.json::<Value>().await else {
        return;
    };
    let Some(b64) = json.get("lyric").and_then(|v| v.as_str()) else {
        return;
    };
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
        if let Ok(text) = String::from_utf8(bytes) {
            let lyric = clean_lrc(&text);
            if !lyric.trim().is_empty() {
                song.lyric = Some(lyric);
            }
        }
    }
}
