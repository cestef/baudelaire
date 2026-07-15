//! Typed AT Protocol identifiers, so repos, collections, keys, and record URIs
//! are never passed around as bare strings that can be swapped by mistake.

use std::fmt;

use serde::{Serialize, Serializer};

/// A repository DID (decentralized identifier), e.g. `did:plc:abc123`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Did(String);

impl Did {
    pub fn new(did: impl Into<String>) -> Self {
        Self(did.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A namespaced identifier: a lexicon id, which doubles as the name of the
/// repository collection its records live in (e.g. `site.standard.document`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nsid(&'static str);

impl Nsid {
    pub const fn new(nsid: &'static str) -> Self {
        Self(nsid)
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Nsid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// FNV-1a 64-bit offset basis.
const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const PRIME: u64 = 0x0000_0100_0000_01b3;
/// The base32-sortable alphabet TIDs use, so derived keys sort and validate
/// like native record keys.
const ALPHABET: &[u8; 32] = b"234567abcdefghijklmnopqrstuvwxyz";

/// A record key within a collection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rkey(String);

impl Rkey {
    /// A fixed, known key (e.g. the conventional `self`).
    pub fn literal(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// A key already produced by the network (from a listed record's URI).
    pub fn parsed(key: &str) -> Self {
        Self(key.to_owned())
    }

    /// A deterministic 13-character key derived from `input` (FNV-1a, top bit
    /// cleared, base32-sortable). A pure function of the input, so the same
    /// source always maps to the same record — republishing updates in place
    /// instead of duplicating.
    pub fn derived(input: &str) -> Self {
        let mut hash = OFFSET;
        for byte in input.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(PRIME);
        }
        // A TID's high bit is always 0; clearing it keeps keys in range.
        hash &= 0x7fff_ffff_ffff_ffff;
        let mut out = [0u8; 13];
        for slot in out.iter_mut().rev() {
            *slot = ALPHABET[(hash & 0x1f) as usize];
            hash >>= 5;
        }
        // Every byte comes from `ALPHABET`, which is ASCII.
        Self(String::from_utf8(out.to_vec()).expect("base32 alphabet is ASCII"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Rkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An `at://did/collection/rkey` reference to a single record. Serializes as its
/// canonical string form, the shape records use to point at one another.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AtUri {
    did: Did,
    collection: Nsid,
    rkey: Rkey,
}

impl AtUri {
    pub fn new(did: Did, collection: Nsid, rkey: Rkey) -> Self {
        Self {
            did,
            collection,
            rkey,
        }
    }
}

impl fmt::Display for AtUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at://{}/{}/{}", self.did, self.collection, self.rkey)
    }
}

impl Serialize for AtUri {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{AtUri, Did, Nsid, Rkey};

    #[test]
    fn derived_key_is_deterministic_and_well_formed() {
        let key = Rkey::derived("/posts/hello/");
        assert_eq!(key, Rkey::derived("/posts/hello/"));
        assert_eq!(key.as_str().len(), 13);
        assert!(
            key.as_str()
                .bytes()
                .all(|b| b"234567abcdefghijklmnopqrstuvwxyz".contains(&b))
        );
    }

    #[test]
    fn distinct_inputs_give_distinct_keys() {
        assert_ne!(Rkey::derived("/a/"), Rkey::derived("/b/"));
    }

    /// The derivation is a permanent contract: the build and the publisher must
    /// agree on it forever, so a change here would silently orphan every record.
    /// This golden value fails loudly if the algorithm ever drifts.
    #[test]
    fn derived_key_is_stable() {
        assert_eq!(Rkey::derived("/posts/hello/").as_str(), "6bgie3n5676tz");
    }

    #[test]
    fn at_uri_renders_and_serializes_as_a_string() {
        let uri = AtUri::new(
            Did::new("did:plc:abc"),
            Nsid::new("site.standard.document"),
            Rkey::literal("xyz"),
        );
        assert_eq!(
            uri.to_string(),
            "at://did:plc:abc/site.standard.document/xyz"
        );
        assert_eq!(
            serde_json::to_value(&uri).unwrap(),
            serde_json::json!("at://did:plc:abc/site.standard.document/xyz")
        );
    }
}
