//! QQ 音乐辅助: qimei 设备指纹 bootstrap + comm 参数构造
//! 移植自 musicdl/modules/utils/qqutils.py

use aes::cipher::{BlockEncryptMut, KeyIvInit};
use base64::Engine;
use rand::Rng;
use rsa::pkcs1v15::Pkcs1v15Encrypt;
use rsa::pkcs8::DecodePublicKey;
use serde_json::Value;

use super::pkcs7_pad;

pub const ENDPOINT: &str = "https://u.y.qq.com/cgi-bin/musicu.fcg";
pub const MUSIC_DOMAIN: &str = "https://isure.stream.qqmusic.qq.com/";
pub const VERSION: &str = "13.2.5.8";
pub const VERSION_CODE: u64 = 13020508;
/// obtain_qimei 失败时的 fallback
pub const FALLBACK_Q36: &str = "6c9d3cd110abca9b16311cee10001e717614";

const SECRET: &str = "ZdJqM15EeO2zWc08";
const APP_KEY: &str = "0AND0HD6FE4HY80F";
const PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDEIxgwoutfwoJxcGQeedgP7FG9qaIuS0qzfR8gWkrkTZKM2iWHn2ajQpBRZjMSoSf6+KJGvar2ORhBfpDXyVtZCKpqLQ+FLkpncClKVIrBwv6PHyUvuCb0rIarmgDnzkfQAqVufEtR64iazGDKatvJ9y6B9NMbHddGSAUmRTCrHQIDAQAB
-----END PUBLIC KEY-----";

fn md5_hex(parts: &[&str]) -> String {
    let mut ctx = md5::Context::new();
    for p in parts {
        ctx.consume(p.as_bytes());
    }
    format!("{:x}", ctx.compute())
}

fn random_chars(chars: &[u8], n: usize) -> String {
    let mut rng = rand::rng();
    (0..n).map(|_| chars[rng.random_range(0..chars.len())] as char).collect()
}

pub fn random_guid() -> String {
    random_chars(b"abcdef1234567890", 32)
}

pub fn random_search_id() -> String {
    let mut rng = rand::rng();
    let t: u64 = rng.random_range(1..=20) * 18014398509481984;
    let n: u64 = rng.random_range(0..=4194304) * 4294967296;
    let r = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        % 86_400_000;
    (t + n + r).to_string()
}

fn random_imei() -> String {
    let mut rng = rand::rng();
    let digits: Vec<u32> = (0..14).map(|_| rng.random_range(0..10)).collect();
    let check: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            if i % 2 == 0 {
                let d = x * 2;
                d / 10 + d % 10
            } else {
                x
            }
        })
        .sum::<u32>()
        * 9
        % 10;
    let body: String = digits.iter().map(u32::to_string).collect();
    format!("{body}{check}")
}

fn random_beacon_id() -> String {
    let mut rng = rand::rng();
    let time_month = format!("{}-01", chrono::Local::now().format("%Y-%m"));
    let rand1: u32 = rng.random_range(100_000..1_000_000);
    let rand2: u32 = rng.random_range(100_000_000..1_000_000_000);
    let mut out = String::new();
    for i in 1..=40 {
        let field = match i {
            1 | 2 | 13 | 14 | 17 | 18 | 21 | 22 | 25 | 26 | 29 | 30 | 33 | 34 | 37 | 38 => {
                format!("k{i}:{time_month}{rand1}.{rand2}")
            }
            3 => "k3:0000000000000000".to_string(),
            4 => format!("k4:{}", random_chars(b"123456789abcdef", 16)),
            _ => format!("k{i}:{}", rng.random_range(0..10000)),
        };
        out.push_str(&field);
        out.push(';');
    }
    out
}

fn aes128_cbc_encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut buf = pkcs7_pad(data, 16);
    let mut enc = cbc::Encryptor::<aes::Aes128>::new_from_slices(key, key).unwrap();
    for chunk in buf.chunks_mut(16) {
        enc.encrypt_block_mut(aes::Block::from_mut_slice(chunk));
    }
    buf
}

fn rsa_encrypt(content: &[u8]) -> Option<Vec<u8>> {
    let key = rsa::RsaPublicKey::from_public_key_pem(PUBLIC_KEY).ok()?;
    let mut rng = rand_core::OsRng;
    key.encrypt(&mut rng, Pkcs1v15Encrypt, content).ok()
}

fn random_payload() -> Value {
    let uptimes = chrono::Local::now() - chrono::Duration::seconds(rand::rng().random_range(0..14400));
    let reserved = serde_json::json!({
        "harmony": "0", "oz": "UhYmelwouA+V2nPWbOvLTgN2/m8jwGB+yUB5v9tysQg=",
        "oo": "Xecjt+9S1+f8Pz2VLSxgpw==", "kelong": "0",
        "uptimes": uptimes.format("%Y-%m-%d %H:%M:%S").to_string(),
        "clone": "0", "containe": "", "multiUser": "0",
        "bod": "Xiaomi", "dv": "sagit", "firstLevel": "", "manufact": "Xiaomi",
        "name": "MI 6", "host": "se.infra",
        "kernel": format!("Linux 5.4.0-54-generic-{} (android-build@google.com)", random_chars(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", 8)),
    });
    serde_json::json!({
        "androidId": random_chars(b"0123456789abcdef", 16),
        "platformId": 1,
        "appKey": APP_KEY,
        "appVersion": VERSION,
        "beaconIdSrc": random_beacon_id(),
        "brand": "Xiaomi",
        "channelId": "10003505",
        "cid": "",
        "imei": random_imei(),
        "imsi": "",
        "mac": "",
        "model": "MI 6",
        "networkType": "unknown",
        "oaid": "",
        "osVersion": "Android 10,level 29",
        "qimei": "",
        "qimei36": "",
        "sdkVersion": "1.2.13.6",
        "targetSdkVersion": "33",
        "audit": "",
        "userId": "{}",
        "packageId": "com.tencent.qqmusic",
        "deviceType": "Phone",
        "sdkName": "",
        "reserved": serde_json::to_string(&reserved).unwrap(),
    })
}

/// 获取 qimei q36, 失败返回 fallback
pub async fn obtain_qimei(client: &reqwest::Client) -> String {
    match obtain_qimei_inner(client).await {
        Some(q36) if !q36.is_empty() => q36,
        _ => FALLBACK_Q36.to_string(),
    }
}

async fn obtain_qimei_inner(client: &reqwest::Client) -> Option<String> {
    let payload = random_payload();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let crypt_key = random_chars(b"adbcdef1234567890", 16);
    let nonce = random_chars(b"adbcdef1234567890", 16);

    let key = base64::engine::general_purpose::STANDARD.encode(rsa_encrypt(crypt_key.as_bytes())?);
    let params = base64::engine::general_purpose::STANDARD.encode(aes128_cbc_encrypt(
        crypt_key.as_bytes(),
        serde_json::to_string(&payload).unwrap().as_bytes(),
    ));
    let extra = format!("{{\"appKey\":\"{APP_KEY}\"}}");
    let sign = md5_hex(&[
        &key,
        &params,
        &(ts * 1000).to_string(),
        &nonce,
        SECRET,
        &extra,
    ]);
    let header_sign = md5_hex(&["qimei_qq_androidpzAuCmaFAaFaHrdakPjLIEqKrGnSOOvH", &ts.to_string()]);

    let resp = client
        .post("https://api.tencentmusic.com/tme/trpc/proxy")
        .header("method", "GetQimei")
        .header("service", "trpc.tme_datasvr.qimeiproxy.QimeiProxy")
        .header("appid", "qimei_qq_android")
        .header("sign", header_sign)
        .header("user-agent", "QQMusic")
        .header("timestamp", ts.to_string())
        .json(&serde_json::json!({
            "app": 0,
            "os": 1,
            "qimeiParams": {
                "key": key,
                "params": params,
                "time": ts.to_string(),
                "nonce": nonce,
                "sign": sign,
                "extra": extra,
            }
        }))
        .send()
        .await
        .ok()?;
    let json: Value = resp.json().await.ok()?;
    let data_str = json.get("data")?.as_str()?;
    let data: Value = serde_json::from_str(data_str).ok()?;
    data.pointer("/data/q36")?.as_str().map(str::to_string)
}

/// 构造 musicu.fcg 请求体: {comm, "{module}.{method}": {module, method, param}}
pub fn build_request_data(
    params: Value,
    module: &str,
    method: &str,
    qimei36: &str,
    common_override: Option<Value>,
) -> Value {
    let mut comm = serde_json::json!({
        "cv": VERSION_CODE,
        "v": VERSION_CODE,
        "QIMEI36": qimei36,
        "ct": "11",
        "tmeAppID": "qqmusic",
        "format": "json",
        "inCharset": "utf-8",
        "outCharset": "utf-8",
        "uid": "3931641530",
    });
    if let Some(over) = common_override {
        if let (Some(c), Some(o)) = (comm.as_object_mut(), over.as_object()) {
            for (k, v) in o {
                c.insert(k.clone(), v.clone());
            }
        }
    }
    serde_json::json!({
        "comm": comm,
        format!("{module}.{method}"): {
            "module": module,
            "method": method,
            "param": params,
        }
    })
}
