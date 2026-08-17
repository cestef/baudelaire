//! Cryptographic digests a browser verifies, for `integrity` attributes and
//! `Content-Security-Policy` hash sources, and the base64 they are written in.
//!
//! Not [`crate::graph::Hash`], which answers whether something changed.

use std::fmt::{self, Write as _};

use sha2::{Digest as _, Sha256, Sha384};

/// Display adapter writing bytes as standard base64 (RFC 4648, `=` padded).
pub struct Base64<'a>(pub &'a [u8]);

impl fmt::Display for Base64<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// The RFC 4648 alphabet, indexed by the six-bit group it encodes.
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        /// The `n`th six-bit group of a chunk's 24 bits, most significant
        /// first.
        fn sextet(bits: u32, n: u32) -> char {
            TABLE[(bits >> (18 - n * 6) & 0x3f) as usize] as char
        }
        for chunk in self.0.chunks(3) {
            let bits = (u32::from(chunk[0]) << 16)
                | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
                | u32::from(chunk.get(2).copied().unwrap_or(0));
            f.write_char(sextet(bits, 0))?;
            f.write_char(sextet(bits, 1))?;
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

/// A digest in the one spelling both SRI and CSP read: `sha384-Xy0..`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(String);

impl Digest {
    /// The digest a `Content-Security-Policy` names an inline script or style
    /// by.
    pub fn sha256(bytes: &[u8]) -> Self {
        Self(format!("sha256-{}", Base64(&Sha256::digest(bytes))))
    }

    /// The digest a subresource `integrity` attribute carries.
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
