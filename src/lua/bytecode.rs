//! Authentication of serialized Lua handler bytecode (IF-035).
//!
//! Function handlers are compiled once at flow-parse time via `func.dump()` and
//! reloaded in a fresh sandbox VM at execution time. Loading untrusted Lua 5.4
//! bytecode is memory-unsafe (it bypasses the sandbox and can corrupt the
//! process heap), so a flow author must not be able to substitute a crafted
//! `bytecode_b64` config string. Each dump is tagged with an HMAC-SHA256 over
//! the bytecode using a per-process ephemeral key; only bytecode this process
//! produced verifies and loads.

use std::sync::OnceLock;

use anyhow::Result;
use base64::Engine;
use sha2::{Digest, Sha256};

const TAG_LEN: usize = 32;
const BLOCK: usize = 64;

/// Per-process ephemeral key, random and never persisted. Derived from two
/// UUID v4 values (each backed by the OS CSPRNG).
fn process_key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        key[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        key
    })
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block_key = [0u8; BLOCK];
    if key.len() > BLOCK {
        block_key[..TAG_LEN].copy_from_slice(&Sha256::digest(key));
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        ipad[index] ^= block_key[index];
        opad[index] ^= block_key[index];
    }
    let inner = Sha256::new()
        .chain_update(ipad)
        .chain_update(message)
        .finalize();
    let outer = Sha256::new()
        .chain_update(opad)
        .chain_update(inner)
        .finalize();
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(&outer);
    tag
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

/// Sign compiled handler bytecode, returning base64 of `tag || bytecode`.
pub(crate) fn sign(bytecode: &[u8]) -> String {
    let tag = hmac_sha256(process_key(), bytecode);
    let mut signed = Vec::with_capacity(TAG_LEN + bytecode.len());
    signed.extend_from_slice(&tag);
    signed.extend_from_slice(bytecode);
    base64::engine::general_purpose::STANDARD.encode(&signed)
}

/// Verify a signed base64 handler payload and return the raw bytecode. Fails
/// unless the tag was produced by this process, so a caller-supplied
/// `bytecode_b64` cannot smuggle arbitrary (memory-unsafe) Lua bytecode.
pub(crate) fn verify(signed_b64: &str) -> Result<Vec<u8>> {
    let signed = base64::engine::general_purpose::STANDARD
        .decode(signed_b64)
        .map_err(|e| anyhow::anyhow!("Failed to decode function bytecode: {e}"))?;
    if signed.len() < TAG_LEN {
        anyhow::bail!("Function bytecode is not authenticated by this process");
    }
    let (tag, bytecode) = signed.split_at(TAG_LEN);
    let expected = hmac_sha256(process_key(), bytecode);
    if !constant_time_eq(tag, &expected) {
        anyhow::bail!(
            "Function bytecode failed authentication (only handlers compiled by this process may run)"
        );
    }
    Ok(bytecode.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_signed_bytecode() {
        let bytecode = b"\x1bLua fake bytecode payload";
        let signed = sign(bytecode);
        assert_eq!(verify(&signed).unwrap(), bytecode);
    }

    #[test]
    fn rejects_unsigned_bytecode() {
        // A plain base64 of raw bytecode (as a flow author could craft) has no
        // valid tag and must be refused.
        let forged = base64::engine::general_purpose::STANDARD.encode(b"malicious bytecode here");
        assert!(verify(&forged).is_err());
    }

    #[test]
    fn rejects_tampered_bytecode() {
        let signed = sign(b"original");
        let mut raw = base64::engine::general_purpose::STANDARD
            .decode(&signed)
            .unwrap();
        // Flip a byte in the bytecode region (after the 32-byte tag).
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        let tampered = base64::engine::general_purpose::STANDARD.encode(&raw);
        assert!(verify(&tampered).is_err());
    }
}
