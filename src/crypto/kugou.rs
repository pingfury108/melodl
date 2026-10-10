//! 酷狗 gateway 签名 (IS_LITE 配置)
//! 移植自 musicdl/modules/utils/kugouutils.py

use rand::Rng;
use std::collections::BTreeMap;

pub const APPID: u32 = 3116;
pub const CLIENTVER: u32 = 11440;
const SIGN_KEY_SECRET: &str = "185672dd44712f60bb1736df5a377e82";
const SIGNATURE_ANDROID_SECRET: &str = "LnT6xpN3khm36zse0QzvmgTZ3waWdRSA";

pub fn md5_hex(data: &str) -> String {
    format!("{:x}", md5::compute(data.as_bytes()))
}

pub fn random_string(n: usize) -> String {
    const CHARS: &[u8] = b"1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut rng = rand::rng();
    (0..n).map(|_| CHARS[rng.random_range(0..CHARS.len())] as char).collect()
}

/// md5(f"{hash}{secret}{appid}{mid}{userid}")
pub fn sign_key(hash: &str, mid: &str, userid: &str, appid: u32) -> String {
    md5_hex(&format!("{hash}{SIGN_KEY_SECRET}{appid}{mid}{userid}"))
}

/// md5(f"{secret}{sorted_params_string}{data}{secret}")
pub fn signature_android(params: &BTreeMap<String, String>, data: &str) -> String {
    let params_string: String = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    md5_hex(&format!(
        "{SIGNATURE_ANDROID_SECRET}{params_string}{data}{SIGNATURE_ANDROID_SECRET}"
    ))
}

/// 构造 GET /v5/url 的完整签名参数
pub fn build_url_params(hash: &str, quality: &str) -> BTreeMap<String, String> {
    let clienttime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string();
    let mut params = BTreeMap::new();
    // 业务参数
    for (k, v) in [
        ("album_id", "0"),
        ("area_code", "1"),
        ("hash", hash.to_lowercase().as_str()),
        ("ssa_flag", "is_fromtrack"),
        ("version", "11436"),
        ("page_id", "967177915"),
        ("quality", quality),
        ("album_audio_id", "0"),
        ("behavior", "play"),
        ("pid", "411"),
        ("cmd", "26"),
        ("pidversion", "3001"),
        ("IsFreePart", "0"),
        ("ppage_id", "356753938,823673182,967485191"),
        ("cdnBackup", "1"),
        ("kcard", "0"),
        ("module", ""),
    ] {
        params.insert(k.to_string(), v.to_string());
    }
    // 设备参数
    params.insert("dfid".into(), random_string(24));
    params.insert("mid".into(), "-".into());
    params.insert("uuid".into(), "-".into());
    params.insert("appid".into(), APPID.to_string());
    params.insert("clientver".into(), CLIENTVER.to_string());
    params.insert("clienttime".into(), clienttime.clone());
    // key + signature
    let key = sign_key(&hash.to_lowercase(), "-", "0", APPID);
    params.insert("key".into(), key);
    let sig = signature_android(&params, "");
    params.insert("signature".into(), sig);
    params
}
