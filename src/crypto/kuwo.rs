//! 酷我加密: 自定义 DES(小端位序) 用于下载链接查询, XOR 用于歌词参数
//! 移植自 musicdl/modules/utils/kuwoutils.py

use std::io::Read;

use base64::Engine;

const SECRET_KEY_SONG: &[u8; 8] = b"ylzsxkwm";
const SECRET_KEY_LYRIC: &[u8] = b"yeelion";

const ARRAY_LS: [usize; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];
const ARRAY_LS_MASK: [u64; 3] = [0, 0x100001, 0x300003];
const ARRAY_E: [i32; 64] = [
    31, 0, 1, 2, 3, 4, -1, -1, 3, 4, 5, 6, 7, 8, -1, -1, 7, 8, 9, 10, 11, 12, -1, -1, 11, 12, 13,
    14, 15, 16, -1, -1, 15, 16, 17, 18, 19, 20, -1, -1, 19, 20, 21, 22, 23, 24, -1, -1, 23, 24,
    25, 26, 27, 28, -1, -1, 27, 28, 29, 30, 31, 30, -1, -1,
];
const ARRAY_IP1: [i32; 64] = [
    39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30, 37, 5, 45, 13, 53, 21, 61, 29,
    36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27, 34, 2, 42, 10, 50, 18, 58, 26,
    33, 1, 41, 9, 49, 17, 57, 25, 32, 0, 40, 8, 48, 16, 56, 24,
];
const ARRAY_IP2: [i32; 64] = [
    57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61, 53, 45, 37, 29, 21, 13, 5,
    63, 55, 47, 39, 31, 23, 15, 7, 56, 48, 40, 32, 24, 16, 8, 0, 58, 50, 42, 34, 26, 18, 10, 2,
    60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
];
const ARRAY_P: [i32; 32] = [
    15, 6, 19, 20, 28, 11, 27, 16, 0, 14, 22, 25, 4, 17, 30, 9, 1, 7, 23, 13, 31, 26, 2, 8, 18,
    12, 29, 5, 21, 10, 3, 24,
];
const ARRAY_PC1: [i32; 56] = [
    56, 48, 40, 32, 24, 16, 8, 0, 57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59,
    51, 43, 35, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 60, 52, 44, 36, 28,
    20, 12, 4, 27, 19, 11, 3,
];
const ARRAY_PC2: [i32; 64] = [
    13, 16, 10, 23, 0, 4, -1, -1, 2, 27, 14, 5, 20, 9, -1, -1, 22, 18, 11, 3, 25, 7, -1, -1, 15,
    6, 26, 19, 12, 1, -1, -1, 40, 51, 30, 36, 46, 54, -1, -1, 29, 39, 50, 44, 32, 47, -1, -1, 43,
    48, 38, 55, 33, 52, -1, -1, 45, 41, 49, 35, 28, 31, -1, -1,
];
#[rustfmt::skip]
const SBOX: [[u8; 64]; 8] = [
    [14,4,3,15,2,13,5,3,13,14,6,9,11,2,0,5,4,1,10,12,15,6,9,10,1,8,12,7,8,11,7,0,0,15,10,5,14,4,9,10,7,8,12,3,13,1,3,6,15,12,6,11,2,9,5,0,4,2,11,14,1,7,8,13],
    [15,0,9,5,6,10,12,9,8,7,2,12,3,13,5,2,1,14,7,8,11,4,0,3,14,11,13,6,4,1,10,15,3,13,12,11,15,3,6,0,4,10,1,7,8,4,11,14,13,8,0,6,2,15,9,5,7,1,10,12,14,2,5,9],
    [10,13,1,11,6,8,11,5,9,4,12,2,15,3,2,14,0,6,13,1,3,15,4,10,14,9,7,12,5,0,8,7,13,1,2,4,3,6,12,11,0,13,5,14,6,8,15,2,7,10,8,15,4,9,11,5,9,0,14,3,10,7,1,12],
    [7,10,1,15,0,12,11,5,14,9,8,3,9,7,4,8,13,6,2,1,6,11,12,2,3,0,5,14,10,13,15,4,13,3,4,9,6,10,1,12,11,0,2,5,0,13,14,2,8,15,7,4,15,1,10,7,5,6,12,11,3,8,9,14],
    [2,4,8,15,7,10,13,6,4,1,3,12,11,7,14,0,12,2,5,9,10,13,0,3,1,11,15,5,6,8,9,14,14,11,5,6,4,1,3,10,2,12,15,0,13,2,8,5,11,8,0,15,7,14,9,4,12,7,10,9,1,13,6,3],
    [12,9,0,7,9,2,14,1,10,15,3,4,6,12,5,11,1,14,13,0,2,8,7,13,15,5,4,10,8,3,11,6,10,4,6,11,7,9,0,6,4,2,13,1,9,15,3,8,15,3,1,14,12,5,11,0,2,12,14,7,5,10,8,13],
    [4,1,3,10,15,12,5,0,2,11,9,6,8,7,6,9,11,4,12,15,0,3,10,5,14,13,7,8,13,14,1,2,13,6,14,9,4,1,2,14,11,13,5,0,1,10,8,3,0,11,3,5,9,4,15,2,7,8,12,15,10,7,6,12],
    [13,7,10,0,6,9,5,15,8,4,3,10,11,14,12,5,2,11,9,6,15,12,0,3,4,1,14,13,1,2,7,8,1,2,12,15,10,4,0,3,13,14,6,9,7,8,9,6,15,1,5,12,3,10,14,5,8,7,11,0,4,13,2,11],
];

fn bittransform(arr: &[i32], l: u64) -> u64 {
    let mut out = 0u64;
    for (i, &idx) in arr.iter().enumerate() {
        if idx >= 0 && (l >> idx) & 1 == 1 {
            out |= 1u64 << i;
        }
    }
    out
}

fn des64(longs: &[u64; 16], l: u64) -> u64 {
    let out = bittransform(&ARRAY_IP2, l);
    let mut p_source = [(out & 0xFFFF_FFFF) as u32, (out >> 32) as u32];
    for long in longs.iter() {
        let r = bittransform(&ARRAY_E, p_source[1] as u64) ^ long;
        let mut s_out: u32 = 0;
        for sbi in (0..8).rev() {
            let idx = ((r >> (sbi * 8)) & 0xFF) as usize;
            s_out = (s_out << 4) | (SBOX[sbi][idx] as u32 & 0xF);
        }
        p_source = [
            p_source[1],
            p_source[0] ^ (bittransform(&ARRAY_P, s_out as u64) as u32),
        ];
    }
    let combined = ((p_source[0] as u64) << 32) | (p_source[1] as u64);
    bittransform(&ARRAY_IP1, combined)
}

/// mode: 0 = 加密, 1 = 解密
fn subkeys(l: u64, mode: u8) -> [u64; 16] {
    let mut longs = [0u64; 16];
    let mut x = bittransform(&ARRAY_PC1, l);
    for (i, &r) in ARRAY_LS.iter().enumerate() {
        let mask = ARRAY_LS_MASK[r];
        x = ((x & mask) << (28 - r)) | ((x & !mask) >> r);
        longs[i] = bittransform(&ARRAY_PC2, x);
    }
    if mode == 1 {
        longs.reverse();
    }
    longs
}

fn crypt(msg: &[u8], key: &[u8; 8], mode: u8) -> Vec<u8> {
    let l = key
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, &b)| acc | ((b as u64) << (i * 8)));
    let longs = subkeys(l, mode);
    let j = msg.len() / 8;
    let mut out = Vec::with_capacity((j + 1) * 8);
    for m in 0..j {
        let block = u64::from_le_bytes(msg[m * 8..m * 8 + 8].try_into().unwrap());
        out.extend_from_slice(&des64(&longs, block).to_le_bytes());
    }
    let rem = &msg[j * 8..];
    let mut rem_bytes = [0u8; 8];
    rem_bytes[..rem.len()].copy_from_slice(rem);
    let tail = if !rem.is_empty() || mode == 0 {
        des64(&longs, u64::from_le_bytes(rem_bytes)).to_le_bytes()
    } else {
        [0u8; 8]
    };
    out.extend_from_slice(&tail);
    out
}

/// 酷我下载链接查询参数加密: base64(DES(query))
pub fn encrypt_query(query: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(crypt(query.as_bytes(), SECRET_KEY_SONG, 0))
}

fn xor_crypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect()
}

/// 歌词请求参数: base64(XOR(params, key))
pub fn build_lyrics_params(music_id: &str) -> String {
    let params =
        format!("user=12345,web,web,web&requester=localhost&req=1&rid=MUSIC_{music_id}&lrcx=1");
    base64::engine::general_purpose::STANDARD.encode(xor_crypt(params.as_bytes(), SECRET_KEY_LYRIC))
}

/// 歌词响应解码: 去头 → zlib 解压 → base64 → XOR → gb18030
pub fn decode_lyrics(buf: &[u8]) -> Option<String> {
    if !buf.starts_with(b"tp=content") {
        return None;
    }
    let split = find_subslice(buf, b"\r\n\r\n")? + 4;
    let mut decoder = flate2::read::ZlibDecoder::new(&buf[split..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).ok()?;
    let b64decoded = base64::engine::general_purpose::STANDARD
        .decode(decompressed)
        .ok()?;
    let decrypted = xor_crypt(&b64decoded, SECRET_KEY_LYRIC);
    let (text, _, _) = encoding_rs::GB18030.decode(&decrypted);
    Some(text.into_owned())
}

/// 卡拉OK 逐字歌词转标准 LRC (移植自 convertrawlrc)
pub fn convert_raw_lrc(raw: &str) -> String {
    let rx_line = regex::Regex::new(r"^\[(\d{2}:\d{2}\.\d{3})\](.*)$").unwrap();
    let rx_word = regex::Regex::new(r"<(-?\d+),(-?\d+)>([^<]*)").unwrap();
    let rx_zh = regex::Regex::new(r"[\u4e00-\u9fa5]").unwrap();

    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(m) = rx_line.captures(lines[i]) else {
            out.push(lines[i].to_string());
            i += 1;
            continue;
        };
        let (ts, payload) = (m[1].to_string(), m[2].to_string());
        if payload.replace("<0,0>", "").trim().is_empty()
            || (payload.starts_with("<0,0>") && rx_zh.is_match(&payload))
        {
            i += 1;
            continue;
        }
        let words: Vec<_> = rx_word.captures_iter(&payload).collect();
        let lyric = if words.is_empty() {
            payload.replace("<0,0>", "").trim().to_string()
        } else {
            words.iter().map(|w| w[3].to_string()).collect()
        };
        let mut trans = String::new();
        if i + 1 < lines.len() {
            if let Some(nm) = rx_line.captures(lines[i + 1]) {
                let next_payload = nm[2].to_string();
                if next_payload.starts_with("<0,0>") && rx_zh.is_match(&next_payload) {
                    trans = next_payload.replace("<0,0>", "").trim().to_string();
                    i += 1;
                }
            }
        }
        out.push(format!("[{ts}]{lyric}"));
        if !trans.is_empty() {
            out.push(format!("[{ts}]{trans}"));
        }
        i += 1;
    }
    out.join("\n")
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_query_produces_base64() {
        let q = encrypt_query("user=0&corp=kuwo&p2p=1&type=convert_url2&sig=0&format=mp3&rid=228908");
        assert!(!q.is_empty());
        assert!(base64::engine::general_purpose::STANDARD.decode(&q).is_ok());
    }

    #[test]
    fn convert_raw_lrc_strips_word_tags() {
        // 逐字标签被合并
        assert_eq!(convert_raw_lrc("[00:01.000]<0,100>你<100,200>好"), "[00:01.000]你好");
        // <0,0> 开头且含中文的行为翻译行标记, 跳过
        assert_eq!(convert_raw_lrc("[00:01.000]<0,0>你好"), "");
        // 普通行原样保留
        assert_eq!(convert_raw_lrc("[00:02.000]plain line"), "[00:02.000]plain line");
    }
}
