//! What this build wrote under the asset URL space, and how big each one is.
//!
//! Built by the engine as files are emitted (the asset pipeline, then the
//! externalized images), and read by the site-wide budget check: a page records
//! *which* files it loads while it renders, and this answers how many bytes
//! each of those is. Split that way on purpose, since the two change
//! independently: re-encoding an image changes no page, and a page can start
//! loading a file that has been the same size all along.

use std::collections::BTreeMap;

/// Served URL -> the bytes written there.
///
/// Keys are the URLs the asset pipeline builds (`/assets/app.<hash>.js`), which
/// carry no base path; a page's recorded reference does, since it is read off
/// the finished markup. [`Emitted::at`] is where the two are reconciled, so no
/// caller has to know which spelling it is holding.
#[derive(Debug, Default, Clone)]
pub struct Emitted {
    files: BTreeMap<String, u64>,
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

    /// Record that `bytes` were written at `url`.
    pub fn insert(&mut self, url: String, bytes: u64) {
        self.files.insert(url, bytes);
    }

    /// Fold another ledger into this one: the externalized images, written
    /// beside what the asset pipeline emitted and served out of the same
    /// directory, so one lookup answers for both.
    pub fn absorb(&mut self, other: &Self) {
        self.files
            .extend(other.files.iter().map(|(url, &bytes)| (url.clone(), bytes)));
    }

    /// The size of the file a page's reference names, or `None` when this build
    /// wrote nothing there: an outbound URL, a static file the pipeline never
    /// saw, a reference that is simply wrong.
    pub fn at(&self, url: &str) -> Option<u64> {
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let path = match self.base.is_empty() {
            true => path,
            false => path.strip_prefix(self.base.as_str()).unwrap_or(path),
        };
        self.files.get(path).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::Emitted;

    fn emitted(base: &str) -> Emitted {
        let mut emitted = Emitted::new(base.to_owned());
        emitted.insert("/assets/app.css".to_owned(), 400);
        emitted
    }

    /// A page hosted under a subpath references `/site/assets/app.css`, which
    /// is the same file the pipeline recorded under `/assets/app.css`.
    #[test]
    fn a_reference_is_found_through_the_base_path() {
        assert_eq!(emitted("/site").at("/site/assets/app.css"), Some(400));
        assert_eq!(emitted("").at("/assets/app.css"), Some(400));
    }

    /// A cache-busting query or a fragment names the same file.
    #[test]
    fn a_query_or_fragment_does_not_hide_the_file() {
        assert_eq!(emitted("").at("/assets/app.css?v=2"), Some(400));
    }

    /// Anything this build did not write weighs nothing here: it is somebody
    /// else's byte, and guessing would be worse than not counting.
    #[test]
    fn an_unknown_url_has_no_size() {
        assert_eq!(emitted("").at("https://cdn.example.com/x.css"), None);
        assert_eq!(emitted("").at("/assets/missing.css"), None);
    }
}
