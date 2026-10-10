pub mod kugou;
pub mod kuwo;
pub mod migu;
pub mod netease;
pub mod qq;

/// 共享 PKCS7 填充
pub(crate) fn pkcs7_pad(data: &[u8], block_size: usize) -> Vec<u8> {
    let pad = block_size - data.len() % block_size;
    let mut out = data.to_vec();
    out.extend(std::iter::repeat_n(pad as u8, pad));
    out
}
