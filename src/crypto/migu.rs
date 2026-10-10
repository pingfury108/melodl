//! 咪咕响应解密: magic(3B) + seed(1B) + payload, plain[i] = raw[4+i] + seed - key[i%32]

const MAGIC: [u8; 3] = [0xab, 0xcd, 0x01];
const KEY: &[u8] = b"Jk8qzuePiJ1qE3mDYhLQ3T73DtDoAhLP";

pub fn decrypt_if_needed(raw: &[u8], signature_header: Option<&str>) -> Vec<u8> {
    let encrypted = signature_header == Some("1") || raw.starts_with(&MAGIC);
    if !encrypted || raw.len() < 4 {
        return raw.to_vec();
    }
    let seed = raw[3];
    raw[4..]
        .iter()
        .enumerate()
        .map(|(i, &b)| b.wrapping_add(seed).wrapping_sub(KEY[i % KEY.len()]))
        .collect()
}
