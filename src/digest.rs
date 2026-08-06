//! Content digests as the web spells them, and the base64 they are written in.
//!
//! One `<algorithm>-<base64>` token serves two features that are the same idea
//! seen from either end: a `<script integrity>` says what a browser must find
//! when it fetches a file, and a `script-src 'sha256-..'` in a
//! `Content-Security-Policy` says what it may run when it does not fetch one at
//! all. Both are produced here so the two can never disagree about how a digest
//! is written.
//!
//! Not [`crate::graph::Hash`], which is blake3 and hex and answers "has this
//! changed since the last build". These are cryptographic digests a *browser*
//! verifies, so the algorithm and the encoding are not ours to choose.

use std::fmt::{self, Write as _};

use sha2::{Digest as _, Sha256, Sha384};

/// Bytes as standard base64 (RFC 4648, `=` padded): a Display adapter, so
/// nothing has to hold an encoded copy to write one out.
pub struct Base64<'a>(pub &'a [u8]);

impl fmt::Display for Base64<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// The RFC 4648 alphabet, indexed by the six-bit group it encodes.
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        /// The `n`th six-bit group of a chunk's 24 bits, most significant
        /// first, which is the order they are written in.
        fn sextet(bits: u32, n: u32) -> char {
            TABLE[(bits >> (18 - n * 6) & 0x3f) as usize] as char
        }
        for chunk in self.0.chunks(3) {
            let bits = (u32::from(chunk[0]) << 16)
                | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
                | u32::from(chunk.get(2).copied().unwrap_or(0));
            f.write_char(sextet(bits, 0))?;
            f.write_char(sextet(bits, 1))?;
            // A short chunk pads rather than encoding the zero bits it never had.
            let third = if chunk.len() > 1 {
                sextet(bits, 2)
            } else {
                '='
            };
            let fourth = if chunk.len() > 2 {
                sextet(bits, 3)
            } else {
                '='
            };
            f.write_char(third)?;
            f.write_char(fourth)?;
        }
        Ok(())
    }
}

/// A subresource digest, in the one spelling both SRI and CSP read:
/// `sha384-Xy0..`. Held as the finished token, since that is what an attribute
/// carries, what a header carries, and what is stored between builds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(String);

impl Digest {
    /// The digest a `Content-Security-Policy` names an inline script or style
    /// by. SHA-256 because that is what the hash-source grammar's shortest form
    /// is, and a policy carries one of these per distinct inline body.
    pub fn sha256(bytes: &[u8]) -> Self {
        Self(format!("sha256-{}", Base64(&Sha256::digest(bytes))))
    }

    /// The digest a subresource `integrity` attribute carries. SHA-384 is what
    /// the SRI specification's own examples use, and the extra 16 bytes cost
    /// nothing on a file a browser was going to fetch anyway.
    pub fn sha384(bytes: &[u8]) -> Self {
        Self(format!("sha384-{}", Base64(&Sha384::digest(bytes))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Base64, Digest};

    fn base64(bytes: &[u8]) -> String {
        Base64(bytes).to_string()
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    /// The two published vectors every SRI generator agrees on, so a wrong
    /// algorithm or a wrong encoding cannot ship: a browser would simply refuse
    /// the file, and only in production.
    #[test]
    fn digests_match_the_published_vectors() {
        assert_eq!(
            Digest::sha256(b"").as_str(),
            "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
        );
        assert_eq!(
            Digest::sha384(b"").as_str(),
            "sha384-OLBgp1GsljhM2TJ+sbHjaiH9txEUvgdDTAzHv2P24donTt6/529l+9Ua0vFImLlb"
        );
    }
}
