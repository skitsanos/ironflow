use sha2::{Digest, Sha256};

const SHA256_LEN: usize = 32;
const SHA256_BLOCK_LEN: usize = 64;

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    std::hint::black_box(difference) == 0
}

pub(crate) fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; SHA256_LEN] {
    let mut block_key = [0u8; SHA256_BLOCK_LEN];
    if key.len() > SHA256_BLOCK_LEN {
        block_key[..SHA256_LEN].copy_from_slice(&Sha256::digest(key));
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0x36u8; SHA256_BLOCK_LEN];
    let mut outer_pad = [0x5cu8; SHA256_BLOCK_LEN];
    for index in 0..SHA256_BLOCK_LEN {
        inner_pad[index] ^= block_key[index];
        outer_pad[index] ^= block_key[index];
    }

    let inner = Sha256::new()
        .chain_update(inner_pad)
        .chain_update(message)
        .finalize();
    let digest = Sha256::new()
        .chain_update(outer_pad)
        .chain_update(inner)
        .finalize();
    let mut tag = [0u8; SHA256_LEN];
    tag.copy_from_slice(&digest);
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_github_documented_vector() {
        let tag = hmac_sha256(b"It's a Secret to Everybody", b"Hello, World!");
        assert_eq!(
            hex::encode(tag),
            "757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17"
        );
    }

    #[test]
    fn constant_time_comparison_matches_only_identical_bytes() {
        assert!(constant_time_eq(b"signature", b"signature"));
        assert!(!constant_time_eq(b"signature", b"signatur!"));
        assert!(!constant_time_eq(b"signature", b"short"));
    }
}
