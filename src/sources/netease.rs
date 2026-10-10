//! 网易云音乐: https://music.163.com
//! 搜索走 cloudsearch 明文接口, 下载链接走 eapi(AES-ECB) 加密接口

use async_trait::async_trait;
use rand::Rng;
use serde_json::Value;

use crate::core::text::{clean_lrc, non_empty};
use crate::core::{tester, MusicSource, SearchCtx, SongInfo};
use crate::crypto;
use crate::error::Result;

pub fn create() -> Box<dyn MusicSource> {
    Box::new(NeteaseSource)
}

struct NeteaseSource;

const PLAYER_URL_API: &str = "https://interface3.music.163.com/eapi/song/enhance/player/url/v1";
/// 音质从高到低
const MUSIC_QUALITIES: [&str; 8] = [
    "jymaster", "jyeffect", "sky", "hires", "lossless", "dolby", "exhigh", "standard",
];
/// 匿名体验 cookie (与原版一致)
const DEFAULT_MUSIC_U: &str = "1eb9ce22024bb666e99b6743b2222f29ef64a9e88fda0fd5754714b900a5d70d993166e004087dd3b95085f6a85b059f5e9aba41e3f2646e3cebdbec0317df58c119e5";

#[async_trait]
impl MusicSource for NeteaseSource {
    fn name(&self) -> &'static str {
        "netease"
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
                .post("https://music.163.com/api/cloudsearch/pc")
                .form(&[
                    ("s", keyword),
                    ("type", "1"),
                    ("limit", &ctx.search_size_per_page.to_string()),
                    ("offset", &(page * ctx.search_size_per_page).to_string()),
                ])
                .send()
                .await?;
            let json: Value = resp.json().await?;
            let Some(list) = json.pointer("/result/songs").and_then(|v| v.as_array()) else {
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
    let song_id = item.get("id")?.as_u64()?;

    // 官方 eapi (需要有效 MUSIC_U cookie, 匿名基本不可用)
    for quality in MUSIC_QUALITIES {
        let Some(url) = fetch_download_url(song_id, quality, ctx).await else {
            continue;
        };
        let test = tester::test_link(&ctx.client, &url).await;
        if !test.ok {
            continue;
        }
        let mut song = build_song_info(item, song_id, &test);
        supplement_lyric(&mut song, ctx).await;
        return Some(song);
    }
    // 第三方兜底
    if let Some(song) = parse_with_kangqiovo(item, song_id, ctx).await {
        return Some(song);
    }
    parse_with_bugpk(item, song_id, ctx).await
}

/// kangqiovo 第三方解析: POST https://ncm.kangqiovo.com/Song_V1
async fn parse_with_kangqiovo(item: &Value, song_id: u64, ctx: &SearchCtx) -> Option<SongInfo> {
    for quality in MUSIC_QUALITIES {
        let Ok(resp) = ctx
            .client
            .post("https://ncm.kangqiovo.com/Song_V1")
            .header("Referer", "https://ncm.kangqiovo.com/")
            .form(&[
                ("url", song_id.to_string()),
                ("level", quality.to_string()),
                ("type", "json".to_string()),
            ])
            .send()
            .await
        else {
            break;
        };
        let Ok(json) = resp.json::<Value>().await else { continue };
        let Some(url) = json
            .pointer("/data/url")
            .and_then(|v| v.as_str())
            .filter(|u| u.starts_with("http"))
        else {
            break;
        };
        let test = tester::test_link(&ctx.client, url).await;
        if !test.ok {
            continue;
        }
        let mut song = build_song_info(item, song_id, &test);
        if let Some(name) = non_empty(json.pointer("/data/name").and_then(|v| v.as_str())) {
            song.song_name = name;
        }
        if let Some(ar) = non_empty(json.pointer("/data/ar_name").and_then(|v| v.as_str())) {
            song.singers = ar;
        }
        if let Some(al) = non_empty(json.pointer("/data/al_name").and_then(|v| v.as_str())) {
            song.album = Some(al);
        }
        if let Some(pic) = non_empty(json.pointer("/data/pic").and_then(|v| v.as_str())) {
            song.cover_url = Some(pic);
        }
        if let Some(lrc) = non_empty(json.pointer("/data/lyric").and_then(|v| v.as_str())) {
            song.lyric = Some(clean_lrc(&lrc));
        }
        return Some(song);
    }
    None
}

fn build_song_info(item: &Value, song_id: u64, test: &tester::LinkTestResult) -> SongInfo {
    SongInfo {
        source: "netease".to_string(),
        identifier: song_id.to_string(),
        song_name: item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        singers: item
            .get("ar")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "unknown".to_string()),
        album: non_empty(item.pointer("/al/name").and_then(|v| v.as_str())),
        ext: test.ext.clone(),
        file_size_bytes: test.file_size_bytes,
        duration_s: item.get("dt").and_then(|v| v.as_u64()).map(|ms| ms / 1000),
        cover_url: non_empty(item.pointer("/al/picUrl").and_then(|v| v.as_str())),
        download_url: Some(test.final_url.clone()),
        raw_data: item.clone(),
        ..Default::default()
    }
}

/// bugpk 第三方解析: https://api.bugpk.com/api/163_music
async fn parse_with_bugpk(item: &Value, song_id: u64, ctx: &SearchCtx) -> Option<SongInfo> {
    for quality in MUSIC_QUALITIES {
        let resp = ctx
            .client
            .get("https://api.bugpk.com/api/163_music")
            .query(&[
                ("ids", song_id.to_string()),
                ("level", quality.to_string()),
                ("type", "json".to_string()),
            ])
            .send()
            .await
            .ok()?;
        let Ok(json) = resp.json::<Value>().await else { continue };
        let Some(url) = json
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|u| {
                u.starts_with("http")
                    && !u.starts_with("https://music.163.com/song/media/outer/url?id=")
            })
        else {
            break;
        };
        let test = tester::test_link(&ctx.client, url).await;
        if !test.ok {
            continue;
        }
        let mut song = build_song_info(item, song_id, &test);
        // bugpk 返回的信息更全
        if let Some(name) = non_empty(json.get("name").and_then(|v| v.as_str())) {
            song.song_name = name;
        }
        if let Some(ar) = non_empty(json.get("ar_name").and_then(|v| v.as_str())) {
            song.singers = ar.replace('/', ", ");
        }
        if let Some(al) = non_empty(json.get("al_name").and_then(|v| v.as_str())) {
            song.album = Some(al);
        }
        if let Some(pic) = non_empty(json.get("pic").and_then(|v| v.as_str())) {
            song.cover_url = Some(pic);
        }
        if let Some(lrc) = non_empty(json.get("lyric").and_then(|v| v.as_str())) {
            song.lyric = Some(clean_lrc(&lrc));
        }
        return Some(song);
    }
    None
}

/// eapi 加密请求获取播放地址
async fn fetch_download_url(song_id: u64, quality: &str, ctx: &SearchCtx) -> Option<String> {
    let request_id = rand::rng().random_range(20_000_000..30_000_000);
    let header_json = format!(
        "{{\"os\": \"pc\", \"appver\": \"\", \"osver\": \"\", \"deviceId\": \"pyncm!\", \"requestId\": \"{request_id}\"}}"
    );
    let encode_type = if quality == "dolby" { "mp4" } else { "flac" };
    // 严格复刻 Python json.dumps: 键按插入序, ", " / ": " 分隔
    let mut payload = format!(
        "{{\"ids\": [{}], \"level\": \"{}\", \"encodeType\": \"{}\", \"header\": \"{}\"",
        song_id,
        quality,
        encode_type,
        header_json.replace('"', "\\\"")
    );
    if quality == "sky" {
        payload.push_str(", \"immerseType\": \"c51\"");
    }
    payload.push('}');

    let encrypted = crypto::netease::encrypt_params(PLAYER_URL_API, &payload);
    let cookie = format!("os=pc; appver=; osver=; deviceId=pyncm!; MUSIC_U={DEFAULT_MUSIC_U}");
    let resp = ctx
        .client
        .post(PLAYER_URL_API)
        .header("Cookie", cookie)
        .form(&[("params", encrypted)])
        .send()
        .await
        .ok()?;
    let json: Value = resp.json().await.ok()?;
    json.pointer("/data/0/url")
        .and_then(|v| v.as_str())
        .filter(|u| u.starts_with("http"))
        .map(str::to_string)
}

async fn supplement_lyric(song: &mut SongInfo, ctx: &SearchCtx) {
    let Ok(resp) = ctx
        .client
        .post("https://interface3.music.163.com/api/song/lyric")
        .form(&[
            ("id", song.identifier.as_str()),
            ("cp", "false"),
            ("tv", "0"),
            ("lv", "0"),
            ("rv", "0"),
            ("kv", "0"),
            ("yv", "0"),
            ("ytv", "0"),
            ("yrv", "0"),
        ])
        .send()
        .await
    else {
        return;
    };
    if let Ok(json) = resp.json::<Value>().await {
        if let Some(text) = json.pointer("/lrc/lyric").and_then(|v| v.as_str()) {
            let lyric = clean_lrc(text);
            if !lyric.trim().is_empty() {
                song.lyric = Some(lyric);
            }
        }
    }
}
