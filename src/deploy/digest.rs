//! Hex digests the deploy layer speaks in.

use sha2::{Digest as _, Sha256};

/// Lowercase hex digests, in the spellings remote stores use.
pub struct Digest;

impl Digest {
    pub fn sha256(data: &[u8]) -> String {
        Self::hex(&Sha256::digest(data))
    }

    /// Lowercase, zero-padded hex.
    pub fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write;
        bytes
            .iter()
            .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            })
    }
}

#[cfg(test)]
mod tests {
    use super::Digest;

    /// The SHA-256 of the empty string, which SigV4 sends for a bodyless
    /// request.
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn sha256_of_empty_is_the_known_constant() {
        assert_eq!(Digest::sha256(b""), EMPTY);
    }

    #[test]
    fn hex_is_lowercase_and_zero_padded() {
        assert_eq!(Digest::hex(&[0x00, 0x0f, 0xff, 0xa0]), "000fffa0");
        assert_eq!(Digest::hex(&[]), "");
    }
}
