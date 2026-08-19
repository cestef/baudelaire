//! What this build wrote under the asset URL space: how big each file is, and
//! what it hashes to.

use std::collections::BTreeMap;

use crate::digest::Digest;

/// One file this build wrote.
#[derive(Debug, Clone)]
pub struct Emission {
    pub bytes: u64,
    /// Its subresource digest, `None` unless `security { sri }` asked for one.
    pub digest: Option<Digest>,
}

/// Served URL -> what was written there.
///
/// Keys carry no base path; a page's reference does, since it is read off the
/// finished markup, and [`Emitted::at`] is where the two are reconciled.
#[derive(Debug, Default, Clone)]
pub struct Emitted {
    files: BTreeMap<String, Emission>,
    /// The base path this site is served under, `""` for a site at the root.
    base: String,
}

impl Emitted {
    pub fn new(base: String) -> Self {
        Self {
            files: BTreeMap::new(),
            base,
        }
    }

    /// Record what was written at `url`, digesting it only when asked.
    pub fn insert(&mut self, url: String, bytes: &[u8], digest: bool) {
        self.files.insert(
            url,
            Emission {
                bytes: bytes.len() as u64,
                digest: digest.then(|| Digest::sha384(bytes)),
            },
        );
    }

    /// Fold another ledger into this one, so one lookup answers for both.
    pub fn absorb(&mut self, other: &Self) {
        self.files
            .extend(other.files.iter().map(|(url, e)| (url.clone(), e.clone())));
    }

    /// What this build wrote at the URL a page's reference names, or `None`
    /// when it wrote nothing there.
    pub fn at(&self, url: &str) -> Option<&Emission> {
        let path = super::Tail::of(url).path;
        let path = if self.base.is_empty() {
            path
        } else {
            path.strip_prefix(self.base.as_str()).unwrap_or(path)
        };
        self.files.get(path)
    }
}

#[cfg(test)]
mod tests {
    use super::Emitted;

    fn emitted(base: &str) -> Emitted {
        let mut emitted = Emitted::new(base.to_owned());
        emitted.insert("/assets/app.css".to_owned(), &[b'x'; 400], true);
        emitted
    }

    fn bytes(emitted: &Emitted, url: &str) -> Option<u64> {
        emitted.at(url).map(|e| e.bytes)
    }

    #[test]
    fn a_reference_is_found_through_the_base_path() {
        assert_eq!(bytes(&emitted("/site"), "/site/assets/app.css"), Some(400));
        assert_eq!(bytes(&emitted(""), "/assets/app.css"), Some(400));
    }

    #[test]
    fn a_query_or_fragment_does_not_hide_the_file() {
        assert_eq!(bytes(&emitted(""), "/assets/app.css?v=2"), Some(400));
    }

    #[test]
    fn an_unknown_url_has_no_size_and_no_digest() {
        assert!(emitted("").at("https://cdn.example.com/x.css").is_none());
        assert!(emitted("").at("/assets/missing.css").is_none());
    }

    #[test]
    fn a_digest_is_only_taken_when_wanted() {
        let mut plain = Emitted::new(String::new());
        plain.insert("/assets/app.css".to_owned(), b"body{}", false);
        let entry = plain.at("/assets/app.css").expect("recorded");
        assert_eq!(entry.bytes, 6);
        assert!(entry.digest.is_none());

        let hashed = emitted("");
        let entry = hashed.at("/assets/app.css").expect("recorded");
        assert!(
            entry
                .digest
                .as_ref()
                .is_some_and(|d| d.as_str().starts_with("sha384-"))
        );
    }
}
