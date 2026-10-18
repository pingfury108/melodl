//! HTTP 流式下载器: 分块写盘 + 进度事件 + 歌词保存

use std::path::{Path, PathBuf};

use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use lofty::picture::MimeType;
use super::{tags, ProgressEvent, SongInfo};
use crate::error::{MelodlError, Result};

/// 根据 URL host 推断 Referer (防盗链)
fn infer_referer(url: &str) -> Option<&'static str> {
    if url.contains("y.qq.com") || url.contains("gtimg.cn") {
        Some("https://y.qq.com/")
    } else if url.contains("163.com") || url.contains("music.126.net") {
        Some("https://music.163.com/")
    } else if url.contains("kuwo.cn") {
        Some("https://www.kuwo.cn/")
    } else if url.contains("kugou.com") {
        Some("https://www.kugou.com/")
    } else {
        None
    }
}

/// magic bytes 检测图片 mime
fn detect_mime(bytes: &[u8]) -> Option<MimeType> {
    match bytes {
        [0xFF, 0xD8, ..] => Some(MimeType::Jpeg),
        [0x89, 0x50, 0x4E, 0x47, ..] => Some(MimeType::Png),
        [0x47, 0x49, 0x46, ..] => Some(MimeType::Gif),
        [0x42, 0x4D, ..] => Some(MimeType::Bmp),
        _ => None,
    }
}

/// 下载封面图片, 失败返回 None 并打 warn
async fn fetch_cover(client: &reqwest::Client, url: &str) -> Option<(Vec<u8>, MimeType)> {
    if !url.starts_with("http") {
        return None;
    }
    let mut req = client.get(url);
    if let Some(referer) = infer_referer(url) {
        req = req.header("Referer", referer);
    }
    match req.send().await {
        Ok(resp) => match resp.bytes().await {
            Ok(bytes) if !bytes.is_empty() => match detect_mime(&bytes) {
                Some(mime) => Some((bytes.to_vec(), mime)),
                None => {
                    tracing::warn!("cover unknown mime: {} (first bytes: {:02x?})", url, &bytes[..bytes.len().min(8)]);
                    None
                }
            },
            Ok(_) => {
                tracing::warn!("cover empty: {}", url);
                None
            }
            Err(e) => {
                tracing::warn!("cover download failed: {} ({})", url, e);
                None
            }
        },
        Err(e) => {
            tracing::warn!("cover request failed: {} ({})", url, e);
            None
        }
    }
}

pub async fn download_song(
    client: &reqwest::Client,
    song: &SongInfo,
    dir: &Path,
    events: Option<&mpsc::Sender<ProgressEvent>>,
) -> Result<PathBuf> {
    if song.protocol == super::Protocol::Hls {
        return download_hls(client, song, dir, events).await;
    }
    download_http(client, song, dir, events).await
}

/// HLS 下载: 解析 m3u8 → 分片下载 → 顺序拼接 (仅支持无加密流)
async fn download_hls(
    client: &reqwest::Client,
    song: &SongInfo,
    dir: &Path,
    events: Option<&mpsc::Sender<ProgressEvent>>,
) -> Result<PathBuf> {
    let url = song
        .download_url
        .clone()
        .ok_or(MelodlError::NoDownloadUrl)?;
    tokio::fs::create_dir_all(dir).await?;
    let id = format!("{}:{}", song.source, song.identifier);

    let resp = client.get(&url).send().await?;
    let base_url = resp.url().clone();
    let body = resp.text().await?;

    // master playlist → 取第一个 variant
    let media = match m3u8_rs::parse_playlist_res(body.as_bytes()) {
        Ok(m3u8_rs::Playlist::MasterPlaylist(mp)) => {
            let variant = mp
                .variants
                .first()
                .ok_or_else(|| MelodlError::Other("empty master playlist".into()))?;
            let variant_url = base_url
                .join(&variant.uri)
                .map_err(|e| MelodlError::Other(format!("url join: {e}")))?;
            let resp = client.get(variant_url.clone()).send().await?;
            let body = resp.text().await?;
            m3u8_rs::parse_media_playlist_res(body.as_bytes())
                .map_err(|e| MelodlError::Other(format!("m3u8 parse: {:?}", e)))?
        }
        Ok(m3u8_rs::Playlist::MediaPlaylist(mp)) => mp,
        Err(e) => return Err(MelodlError::Other(format!("m3u8 parse: {:?}", e))),
    };

    if media.segments.iter().any(|s| s.key.is_some()) {
        return Err(MelodlError::Other("encrypted HLS not supported".into()));
    }
    let total = media.segments.len() as u64;
    if total == 0 {
        return Err(MelodlError::Other("empty media playlist".into()));
    }

    // 分片扩展名决定输出格式
    let first_uri = &media.segments[0].uri;
    let ext = first_uri
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("ts");
    let mut song = song.clone();
    song.ext = Some(ext.to_string());
    let save_path = song.save_path(dir);

    let emit = |ev: ProgressEvent| async {
        if let Some(tx) = events {
            let _ = tx.send(ev).await;
        }
    };
    emit(ProgressEvent::DownloadStarted {
        id: id.clone(),
        total: Some(total),
    })
    .await;

    let mut file = tokio::fs::File::create(&save_path).await?;
    for (i, seg) in media.segments.iter().enumerate() {
        let seg_url = base_url
            .join(&seg.uri)
            .map_err(|e| MelodlError::Other(format!("url join: {e}")))?;
        let bytes = client.get(seg_url).send().await?.bytes().await?;
        file.write_all(&bytes).await?;
        emit(ProgressEvent::DownloadProgress {
            id: id.clone(),
            downloaded: i as u64 + 1,
            total: Some(total),
        })
        .await;
    }
    file.flush().await?;

    // 封面 + 标签
    let cover = match &song.cover_url {
        Some(u) => fetch_cover(client, u).await,
        None => None,
    };
    tags::write_tags(&save_path, &song, cover);
    emit(ProgressEvent::DownloadFinished {
        id,
        path: save_path.display().to_string(),
    })
    .await;
    Ok(save_path)
}

async fn download_http(
    client: &reqwest::Client,
    song: &SongInfo,
    dir: &Path,
    events: Option<&mpsc::Sender<ProgressEvent>>,
) -> Result<PathBuf> {
    let url = song
        .download_url
        .clone()
        .ok_or(MelodlError::NoDownloadUrl)?;
    tokio::fs::create_dir_all(dir).await?;

    let save_path = song.save_path(dir);
    let id = format!("{}:{}", song.source, song.identifier);

    let emit = |ev: ProgressEvent| async {
        if let Some(tx) = events {
            let _ = tx.send(ev).await;
        }
    };

    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(MelodlError::Other(format!(
            "download failed: status {}",
            resp.status()
        )));
    }
    let total = resp.content_length().filter(|&n| n > 0);
    emit(ProgressEvent::DownloadStarted {
        id: id.clone(),
        total,
    })
    .await;

    let mut file = tokio::fs::File::create(&save_path).await?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        emit(ProgressEvent::DownloadProgress {
            id: id.clone(),
            downloaded,
            total,
        })
        .await;
    }
    file.flush().await?;

    // 保存歌词
    if let Some(lyric) = &song.lyric {
        if !lyric.is_empty() && lyric != "NULL" {
            let lrc_path = save_path.with_extension("lrc");
            let _ = tokio::fs::write(lrc_path, lyric).await;
        }
    }

    // 封面 + 标签
    let cover = match &song.cover_url {
        Some(u) => fetch_cover(client, u).await,
        None => None,
    };
    tags::write_tags(&save_path, song, cover);

    emit(ProgressEvent::DownloadFinished {
        id,
        path: save_path.display().to_string(),
    })
    .await;
    Ok(save_path)
}
