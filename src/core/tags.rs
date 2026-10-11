//! 音频标签写入: 标题/歌手/专辑/封面 (lofty)

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture};
use lofty::prelude::Accessor;
use lofty::tag::{ItemKey, Tag};

use super::SongInfo;

/// 写入基本标签 + 封面, 失败静默(不影响下载结果)
pub fn write_tags(path: &Path, song: &SongInfo, cover: Option<(Vec<u8>, MimeType)>) {
    let _ = write_tags_inner(path, song, cover).map_err(|e| {
        tracing::warn!("write tags failed for {}: {}", path.display(), e);
    });
}

fn write_tags_inner(
    path: &Path,
    song: &SongInfo,
    cover: Option<(Vec<u8>, MimeType)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tagged = lofty::read_from_path(path)?;

    // 复用主标签, 不存在则按文件类型新建
    if tagged.primary_tag_mut().is_none() {
        let tag = Tag::new(tagged.primary_tag_type());
        tagged.insert_tag(tag);
    }
    {
        let Some(tag) = tagged.primary_tag_mut() else {
            return Ok(());
        };
        tag.set_title(song.song_name.clone());
        if !song.singers.is_empty() {
            tag.set_artist(song.singers.clone());
        }
        if let Some(album) = &song.album {
            tag.insert_text(ItemKey::AlbumTitle, album.clone());
        }
        if let Some((bytes, mime)) = cover {
            tag.push_picture(Picture::new_unchecked(
                lofty::picture::PictureType::CoverFront,
                Some(mime),
                None,
                bytes,
            ));
        }
    }

    tagged.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

pub fn cover_mime(url: &str) -> MimeType {
    if url.contains(".png") {
        MimeType::Png
    } else {
        MimeType::Jpeg
    }
}
