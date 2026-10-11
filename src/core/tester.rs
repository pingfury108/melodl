//! 下载链接有效性探测: HEAD → GET 采样, 推断音频格式与大小

use reqwest::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, RANGE};

#[derive(Debug, Clone, Default)]
pub struct LinkTestResult {
    pub ok: bool,
    pub ext: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub final_url: String,
    pub status: u16,
}

pub const VALID_AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma", "ape", "alac",
];

pub async fn test_link(client: &reqwest::Client, url: &str) -> LinkTestResult {
    let mut result = LinkTestResult {
        final_url: url.to_string(),
        ..Default::default()
    };

    // 1) HEAD
    if let Ok(resp) = client.head(url).send().await {
        result.status = resp.status().as_u16();
        result.final_url = resp.url().to_string();
        if resp.status().is_success() {
            result.file_size_bytes = content_length(resp.headers());
            let ct = header_str(resp.headers(), CONTENT_TYPE).map(str::to_string);
            let cd = header_str(resp.headers(), CONTENT_DISPOSITION).map(str::to_string);
            if let Some(ext) = infer_ext(url, &result.final_url, ct.as_deref(), cd.as_deref(), &[])
            {
                result.ext = Some(ext);
                result.ok = true;
                return result;
            }
        }
    }

    // 2) GET 采样前 8KB
    if let Ok(resp) = client.get(url).header(RANGE, "bytes=0-8191").send().await {
        result.status = resp.status().as_u16();
        result.final_url = resp.url().to_string();
        if resp.status().is_success() || resp.status().as_u16() == 206 {
            let content_type = header_str(resp.headers(), CONTENT_TYPE).map(str::to_string);
            let content_disp = header_str(resp.headers(), CONTENT_DISPOSITION).map(str::to_string);
            if result.file_size_bytes.is_none() {
                result.file_size_bytes = content_length(resp.headers());
            }
            let sample = resp.bytes().await.unwrap_or_default();
            if let Some(ext) = infer_ext(
                url,
                &result.final_url,
                content_type.as_deref(),
                content_disp.as_deref(),
                &sample,
            ) {
                result.ext = Some(ext);
                result.ok = true;
            }
        }
    }
    result
}

fn header_str(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn content_length(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
}

/// 按优先级推断扩展名: Content-Type → URL/文件名后缀 → 魔数
fn infer_ext(
    original_url: &str,
    final_url: &str,
    content_type: Option<&str>,
    content_disposition: Option<&str>,
    sample: &[u8],
) -> Option<String> {
    if let Some(ct) = content_type {
        let ct = ct.split(';').next().unwrap_or("").trim().to_lowercase();
        let ext = match ct.as_str() {
            "audio/mpeg" | "audio/mp3" => Some("mp3"),
            "audio/flac" | "audio/x-flac" => Some("flac"),
            "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
            "audio/mp4" | "audio/x-m4a" => Some("m4a"),
            "audio/aac" => Some("aac"),
            "audio/ogg" => Some("ogg"),
            "audio/opus" => Some("opus"),
            "audio/x-ms-wma" => Some("wma"),
            "audio/ape" | "audio/x-ape" => Some("ape"),
            _ => None,
        };
        if let Some(e) = ext {
            return Some(e.to_string());
        }
    }

    // URL / Content-Disposition 文件名后缀
    for candidate in [final_url, original_url] {
        if let Some(ext) = ext_from_url(candidate) {
            return Some(ext);
        }
    }
    if let Some(cd) = content_disposition {
        if let Some(ext) = ext_from_url(cd) {
            return Some(ext);
        }
    }

    // 魔数
    ext_from_magic(sample).map(str::to_string)
}

fn ext_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    let ext = path.rsplit('.').next()?;
    let ext = ext.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let lower = ext.to_lowercase();
    if VALID_AUDIO_EXTS.contains(&lower.as_str()) {
        Some(lower)
    } else {
        None
    }
}

fn ext_from_magic(sample: &[u8]) -> Option<&'static str> {
    if sample.len() < 12 {
        return None;
    }
    if sample.starts_with(b"fLaC") {
        Some("flac")
    } else if sample.starts_with(b"ID3") || (sample[0] == 0xFF && (sample[1] & 0xE0) == 0xE0) {
        Some("mp3")
    } else if sample.starts_with(b"RIFF") && &sample[8..12] == b"WAVE" {
        Some("wav")
    } else if sample.starts_with(b"OggS") {
        Some("ogg")
    } else if &sample[4..8] == b"ftyp" {
        Some("m4a")
    } else {
        None
    }
}
