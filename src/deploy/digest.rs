//! Hex digests the deploy layer speaks in.
//!
//! Every remote names its objects' contents by a hex digest of some hash, and
//! baudelaire has to produce the identical spelling to compare against it: S3
//! signs requests over a SHA-256 and returns an MD5 ETag, an SSH host runs
//! `sha256sum`. One home for the hashing and the hex, because all three of those
//! callers used to reach into [`sigv4`](super::sigv4) for it, which is the AWS
//! request-signing scheme and not a general place to keep a hash.

use sha2::{Digest as _, Sha256};

/// Lowercase hex digests, in the spellings remote stores use.
pub struct Digest;

impl Digest {
    /// The lowercase hex SHA-256 of `data`: the value S3 wants in
    /// `x-amz-content-sha256`, the digest SigV4 signs over, and what
    /// `sha256sum` prints on an SSH host.
    pub fn sha256(data: &[u8]) -> String {
        Self::hex(&Sha256::digest(data))
    }

    /// Lowercase, zero-padded hex. Not [`Hash::hex`](crate::graph::Hash::hex),
    /// which spells a blake3 digest through blake3's own formatter: this one
    /// encodes the bytes of whatever hash a remote happens to use.
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
    /// request and which therefore has to be exactly right.
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
