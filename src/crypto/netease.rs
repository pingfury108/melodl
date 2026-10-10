//! 网易云 eapi 加密: AES-128-ECB(PKCS7) + md5 摘要
//! 移植自 musicdl/modules/utils/neteaseutils.py

use super::pkcs7_pad;
use aes::cipher::{BlockEncryptMut, KeyInit};

const EAPI_KEY: &[u8; 16] = b"e82ckenh8dichen8";

pub fn md5_hex(data: &str) -> String {
    format!("{:x}", md5::compute(data.as_bytes()))
}

/// url 传完整接口地址, payload_str 为 Python json.dumps 风格(键序 + ", " / ": " 分隔)
pub fn encrypt_params(url: &str, payload_str: &str) -> String {
    let url_path = reqwest::Url::parse(url)
        .map(|u| u.path().replace("/eapi/", "/api/"))
        .unwrap_or_default();
    let digest = md5_hex(&format!("nobody{url_path}use{payload_str}md5forencrypt"));
    let message = format!("{url_path}-36cd479b6b5-{payload_str}-36cd479b6b5-{digest}");

    let mut buf = pkcs7_pad(message.as_bytes(), 16);
    let mut enc = ecb::Encryptor::<aes::Aes128>::new(EAPI_KEY.into());
    for chunk in buf.chunks_mut(16) {
        enc.encrypt_block_mut(aes::Block::from_mut_slice(chunk));
    }
    hex::encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 Python 原版 + openssl 计算结果对比的已知向量
    #[test]
    fn eapi_known_vector() {
        let payload = "{\"ids\": [2652820720], \"level\": \"standard\", \"encodeType\": \"flac\", \"header\": \"{\\\"os\\\": \\\"pc\\\", \\\"appver\\\": \\\"\\\", \\\"osver\\\": \\\"\\\", \\\"deviceId\\\": \\\"pyncm!\\\", \\\"requestId\\\": \\\"25000000\\\"}\"}";
        let enc = encrypt_params("https://interface3.music.163.com/eapi/song/enhance/player/url/v1", payload);
        assert!(enc.starts_with("fa90b329e9614f79e79598f37dc2edb487f00d1bc4c9b24cd57e6c318b907356"));
        assert!(enc.ends_with("e97a75046d87db6f5460ac240ad0b1c264a705303e"));
    }
}
