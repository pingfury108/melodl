//! 咪咕音乐: https://music.migu.cn

use async_trait::async_trait;
use serde_json::Value;

use crate::core::text::{clean_lrc, non_empty};
use crate::core::{tester, MusicSource, SearchCtx, SongInfo};
use crate::crypto;
use crate::error::Result;

pub fn create() -> Box<dyn MusicSource> {
    Box::new(MiguSource)
}

struct MiguSource;

const MIGU_HEADERS: &[(&str, &str)] = &[
    ("Accept", "application/json, text/plain, */*"),
    ("Origin", "https://h5.nf.migu.cn"),
    ("Referer", "https://h5.nf.migu.cn/"),
    ("ua", "Android_migu"),
    ("version", "6.8.8"),
    ("channel", "014021I"),
    ("subchannel", "014021I"),
];

#[async_trait]
impl MusicSource for MiguSource {
    fn name(&self) -> &'static str {
        "migu"
    }

    async fn search(&self, keyword: &str, ctx: &SearchCtx) -> Result<Vec<SongInfo>> {
        let mut songs = Vec::new();
        let pages = ctx
            .search_size_per_source
            .div_ceil(ctx.search_size_per_page)
            .max(1);
        let search_switch =
            serde_json::json!({"song":1,"album":0,"singer":0,"tagSong":1,"mvSong":0,"bestShow":1});

        for page in 1..=pages {
            let mut req = ctx
                .client
                .get("https://c.musicapp.migu.cn/v1.0/content/search_all.do")
                .query(&[
                    ("text", keyword),
                    ("pageNo", &page.to_string()),
                    ("pageSize", &ctx.search_size_per_page.to_string()),
                    ("isCopyright", "1"),
                    ("sort", "1"),
                    ("searchSwitch", &search_switch.to_string()),
                ]);
            for (k, v) in MIGU_HEADERS {
                req = req.header(*k, *v);
            }
            let json: Value = req.send().await?.json().await?;
            let Some(list) = json
                .pointer("/songResultData/result")
                .and_then(|v| v.as_array())
            else {
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

/// 咪咕音质: 值越大越优, Z3D 为加密格式跳过
fn quality_rank(format_type: &str) -> u32 {
    match format_type {
        "ZQ32" => 8,
        "ZQ24" => 7,
        "ZQ" => 6,
        "SQ" => 5,
        "HQ" => 4,
        "PQ" => 3,
        "LQ" => 2,
        _ => 1,
    }
}

async fn parse_song(item: &Value, ctx: &SearchCtx) -> Option<SongInfo> {
    let content_id = item.get("contentId")?.as_str()?.to_string();
    let copyright_id = item.get("copyrightId")?.as_str()?.to_string();

    // 收集音质并按等级降序
    let mut formats: Vec<&Value> = ["rateFormats", "newRateFormats", "audioFormats"]
        .iter()
        .filter_map(|k| item.get(k).and_then(|v| v.as_array()))
        .flatten()
        .collect();
    formats.sort_by_key(|f| {
        std::cmp::Reverse(quality_rank(
            f.get("formatType").and_then(|v| v.as_str()).unwrap_or(""),
        ))
    });

    for format in formats {
        let format_type = format.get("formatType")?.as_str()?;
        if format_type == "Z3D" {
            continue;
        }
        let resource_type = format.get("resourceType")?.as_str()?;
        let Some(url) =
            fetch_download_url(&content_id, &copyright_id, resource_type, format_type, ctx).await
        else {
            continue;
        };
        let test = tester::test_link(&ctx.client, &url).await;
        if !test.ok {
            continue;
        }

        let mut song = SongInfo {
            source: "migu".to_string(),
            identifier: content_id.clone(),
            song_name: item
                .get("name")
                .or_else(|| item.get("songName"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            singers: item
                .get("singers")
                .or_else(|| item.get("singerList"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "unknown".to_string()),
            album: non_empty(item.get("album").and_then(|v| v.as_str())).or_else(|| {
                item.get("albums").and_then(|v| v.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
            }),
            ext: test.ext,
            file_size_bytes: test.file_size_bytes,
            cover_url: extract_cover(item),
            download_url: Some(test.final_url.clone()),
            raw_data: item.clone(),
            ..Default::default()
        };
        supplement_lyric(&mut song, &copyright_id, item, ctx).await;
        return Some(song);
    }
    None
}

async fn fetch_download_url(
    content_id: &str,
    copyright_id: &str,
    resource_type: &str,
    format_type: &str,
    ctx: &SearchCtx,
) -> Option<String> {
    let resp = ctx
        .client
        .get("https://c.musicapp.migu.cn/strategy/listen-url/h5/v2.4")
        .header("Content-Type", "application/json;charset=UTF-8")
        .header("birth", "h5page")
        .header("signature", "1")
        .query(&[
            ("contentId", content_id),
            ("copyrightId", copyright_id),
            ("resourceType", resource_type),
            ("netType", "01"),
            ("toneFlag", format_type),
            ("scene", ""),
            ("lowerQualityContentId", content_id),
        ])
        .send()
        .await
        .ok()?;
    let signature = resp
        .headers()
        .get("signature")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let raw = resp.bytes().await.ok()?;
    let json: Value =
        serde_json::from_slice(&crypto::migu::decrypt_if_needed(&raw, signature.as_deref()))
            .ok()?;
    let mut url = json
        .pointer("/data/url")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "https://app.pd.nf.migu.cn/MIGUM3.0/v1.0/content/sub/listenSong.do?channel=mx&copyrightId={copyright_id}&contentId={content_id}&toneFlag={format_type}&resourceType={resource_type}&userId=15548614588710179085069&netType=00"
            )
        });
    // 128K 升 320K
    url = url.replace("MP3_128_16_Stero", "MP3_320_16_Stero");
    url.starts_with("http").then_some(url)
}

fn extract_cover(item: &Value) -> Option<String> {
    let url = item
        .get("imgItems")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.last())
        .and_then(|img| img.get("img"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            ["img3", "img2", "img1"]
                .iter()
                .find_map(|k| item.get(k).and_then(|v| v.as_str()))
                .map(str::to_string)
        })?;
    if url.starts_with("http") {
        Some(url)
    } else {
        Some(format!("https://d.musicapp.migu.cn{url}"))
    }
}

async fn supplement_lyric(song: &mut SongInfo, copyright_id: &str, item: &Value, ctx: &SearchCtx) {
    // 优先搜索结果自带 lyricUrl, 否则走 pc listen 接口拿 lrcUrl
    let mut lyric_url = item
        .get("lyricUrl")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if lyric_url.is_none() {
        if let Ok(resp) = ctx
            .client
            .get("https://app.c.nf.migu.cn/MIGUM3.0/strategy/pc/listen/v1.0")
            .query(&[
                ("scene", ""),
                ("netType", "01"),
                ("resourceType", "2"),
                ("copyrightId", copyright_id),
                ("contentId", &song.identifier),
                ("toneFlag", "PQ"),
            ])
            .send()
            .await
        {
            if let Ok(json) = resp.json::<Value>().await {
                lyric_url = json
                    .pointer("/data/lrcUrl")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
        }
    }
    let Some(url) = lyric_url.filter(|u| u.starts_with("http")) else {
        return;
    };
    if let Ok(resp) = ctx
        .client
        .get(&url)
        .header("Referer", "https://y.migu.cn/")
        .send()
        .await
    {
        if let Ok(text) = resp.text().await {
            let lyric = clean_lrc(&text);
            if !lyric.trim().is_empty() {
                song.lyric = Some(lyric);
            }
        }
    }
}
