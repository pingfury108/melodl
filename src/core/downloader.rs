//! HTTP 流式下载器: 分块写盘 + 进度事件 + 歌词保存

use std::path::{Path, PathBuf};

use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use super::{ProgressEvent, SongInfo};
use crate::error::{MelodlError, Result};

pub async fn download_song(
    client: &reqwest::Client,
    song: &SongInfo,
    dir: &Path,
    events: Option<&mpsc::Sender<ProgressEvent>>,
) -> Result<PathBuf> {
    let url = song.download_url.clone().ok_or(MelodlError::NoDownloadUrl)?;
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

    emit(ProgressEvent::DownloadFinished {
        id,
        path: save_path.display().to_string(),
    })
    .await;
    Ok(save_path)
}
