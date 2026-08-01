//! What this build wrote under the asset URL space: how big each file is, and
//! what it hashes to.
//!
//! Built by the engine as files are emitted (the asset pipeline, then the
//! externalized images), and read twice over. The site-wide budget check asks
//! how many bytes a page's recorded load is; the integrity transform asks what
//! digest to stamp on it. Neither belongs to the page, which is the point: a
//! page records *which* files it loads and this build answers for them, so
//! re-encoding an image changes no page and a page can start loading a file
//! that has been the same size all along.

use std::collections::BTreeMap;

use crate::digest::Digest;

/// One file this build wrote.
#[derive(Debug, Clone)]
pub struct Emission {
    /// Its size, for the weight budgets.
    pub bytes: u64,
    /// Its subresource digest, computed only when `security { sri }` asks for
    /// one: it is a second cryptographic pass over every asset, and a site not
    /// stamping integrity attributes should not pay for it.
    pub digest: Option<Digest>,
}

/// Served URL -> what was written there.
///
/// Keys are the URLs the asset pipeline builds (`/assets/app.<hash>.js`), which
/// carry no base path; a page's reference does, since it is read off the
/// finished markup. [`Emitted::at`] is where the two are reconciled, so no
/// caller has to know which spelling it is holding.
#[derive(Debug, Default, Clone)]
pub struct Emitted {
    files: BTreeMap<String, Emission>,
    /// The base path this site is served under, `""` for a site at the root.
    base: String,
}

impl Emitted {
    /// An empty ledger for a site served under `base` (`Config::base_path`).
    pub fn new(base: String) -> Self {
        Self {
            files: BTreeMap::new(),
            base,
        }
    }

    /// Record what was written at `url`. `digest` is `None` when this build was
    /// not asked for integrity attributes.
    pub fn insert(&mut self, url: String, bytes: &[u8], digest: bool) {
        self.files.insert(
            url,
            Emission {
                bytes: bytes.len() as u64,
                digest: digest.then(|| Digest::sha384(bytes)),
            },
        );
    }

    /// Fold another ledger into this one: the externalized images, written
    /// beside what the asset pipeline emitted and served out of the same
    /// directory, so one lookup answers for both.
    pub fn absorb(&mut self, other: &Self) {
        self.files
            .extend(other.files.iter().map(|(url, e)| (url.clone(), e.clone())));
    }

    /// What this build wrote at the URL a page's reference names, or `None`
    /// when it wrote nothing there: an outbound URL, a static file the pipeline
    /// never saw, a reference that is simply wrong.
    pub fn at(&self, url: &str) -> Option<&Emission> {
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let path = match self.base.is_empty() {
            true => path,
            false => path.strip_prefix(self.base.as_str()).unwrap_or(path),
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

    /// A page hosted under a subpath references `/site/assets/app.css`, which
    /// is the same file the pipeline recorded under `/assets/app.css`.
    #[test]
    fn a_reference_is_found_through_the_base_path() {
        assert_eq!(bytes(&emitted("/site"), "/site/assets/app.css"), Some(400));
        assert_eq!(bytes(&emitted(""), "/assets/app.css"), Some(400));
    }

    /// A cache-busting query or a fragment names the same file.
    #[test]
    fn a_query_or_fragment_does_not_hide_the_file() {
        assert_eq!(bytes(&emitted(""), "/assets/app.css?v=2"), Some(400));
    }

    /// Anything this build did not write weighs nothing here and is stamped
    /// with nothing: it is somebody else's byte, and guessing would be worse
    /// than not counting.
    #[test]
    fn an_unknown_url_has_no_size_and_no_digest() {
        assert!(emitted("").at("https://cdn.example.com/x.css").is_none());
        assert!(emitted("").at("/assets/missing.css").is_none());
    }

    /// A digest is only computed when it was asked for; the size is always
    /// there.
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
