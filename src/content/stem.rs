//! Decoding a source filename: the slug it carries, plus an optional language
//! and an optional draft marker, in either order.

use std::path::Path;

use crate::config::Config;

/// The parsed stem of a source path: its language and draft markers peeled off,
/// leaving the slug. The two markers stack in either order, so
/// `post.draft.fr.typ` and `post.fr.draft.typ` both name a French draft.
pub(super) struct Stem<'a> {
    slug: &'a str,
    draft: bool,
    /// Declared non-default language named by a trailing `.{code}`, if any.
    lang: Option<&'a str>,
}

impl<'a> Stem<'a> {
    pub(super) fn of(path: &'a Path, config: &'a Config) -> Self {
        let full = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| config.index());
        let mut slug = full;
        let mut lang = None;
        let mut draft = false;
        for _ in 0..2 {
            if let (None, Some((head, code))) = (lang, Self::language(slug, config)) {
                slug = head;
                lang = Some(code);
            } else if let (false, Some(head)) =
                (draft, Self::undraft(slug, &config.content.drafts.suffix))
            {
                slug = head;
                draft = true;
            }
        }
        Self { slug, draft, lang }
    }

    /// The declared, non-default language a trailing `.{code}` names, with the
    /// stem before it; the default language uses bare filenames, so `.en` on an
    /// en site stays put.
    fn language(stem: &'a str, config: &Config) -> Option<(&'a str, &'a str)> {
        match stem.rsplit_once('.') {
            Some((head, code)) if code != config.lang && config.knows(code) => Some((head, code)),
            _ => None,
        }
    }

    /// The stem with the draft marker peeled, or `None` when it carries none.
    /// An empty suffix disables the marker entirely.
    fn undraft(stem: &'a str, suffix: &str) -> Option<&'a str> {
        if suffix.is_empty() {
            return None;
        }
        stem.strip_suffix(suffix)
    }

    pub(super) fn is_draft(&self) -> bool {
        self.draft
    }

    pub(super) fn lang(&self) -> Option<&'a str> {
        self.lang
    }

    /// A trailing segment that looks like a language code but is not declared,
    /// on a site that declares languages at all. Deliberately narrow: two or
    /// three lowercase ASCII letters, so an ordinary dotted filename
    /// (`notes.v2.typ`) is untouched.
    pub(super) fn undeclared(&self, config: &Config) -> Option<&'a str> {
        if !config.multilingual() || self.lang.is_some() {
            return None;
        }
        let (_, code) = self.slug.rsplit_once('.')?;
        let shaped = matches!(code.len(), 2 | 3) && code.chars().all(|c| c.is_ascii_lowercase());
        (shaped && !config.knows(code)).then_some(code)
    }

    /// Whether this stem names a bundle index (`config.content.index`), so the
    /// file's parent directory supplies the slug rather than the file name.
    pub(super) fn is_index(&self, config: &Config) -> bool {
        config
            .content
            .index
            .as_deref()
            .is_some_and(|idx| self.slug == idx)
    }

    pub(super) fn slug(&self) -> &'a str {
        self.slug
    }
}

#[cfg(test)]
mod tests {
    use super::Stem;
    use crate::config::Config;
    use std::path::Path;

    fn config() -> Config {
        Config::parse("lang \"en\"\nlanguages {\n  fr { }\n}\n").expect("config")
    }

    #[test]
    fn draft_and_language_markers_decode_in_either_order() {
        let config = config();
        for name in ["post.draft.fr.typ", "post.fr.draft.typ"] {
            let stem = Stem::of(Path::new(name), &config);
            assert_eq!(stem.slug(), "post", "{name}");
            assert_eq!(stem.lang(), Some("fr"), "{name}");
            assert!(stem.is_draft(), "{name}");
        }
    }

    #[test]
    fn a_bare_stem_carries_neither_marker() {
        let config = config();
        let stem = Stem::of(Path::new("post.typ"), &config);
        assert_eq!(stem.slug(), "post");
        assert_eq!(stem.lang(), None);
        assert!(!stem.is_draft());
    }

    #[test]
    fn an_undeclared_trailing_code_stays_in_the_slug() {
        let config = config();
        let stem = Stem::of(Path::new("post.de.typ"), &config);
        assert_eq!(stem.slug(), "post.de");
        assert_eq!(stem.lang(), None);
    }

    #[test]
    fn an_undeclared_code_is_reported_on_a_multilingual_site() {
        let config = config();
        assert_eq!(
            Stem::of(Path::new("post.de.typ"), &config).undeclared(&config),
            Some("de")
        );
        assert_eq!(
            Stem::of(Path::new("post.fr.typ"), &config).undeclared(&config),
            None
        );
        assert_eq!(
            Stem::of(Path::new("post.en.typ"), &config).undeclared(&config),
            None
        );
        for name in [
            "notes.v2.typ",
            "report.2024.typ",
            "a.LONG.typ",
            "x.abcd.typ",
        ] {
            assert_eq!(
                Stem::of(Path::new(name), &config).undeclared(&config),
                None,
                "{name}"
            );
        }
    }

    #[test]
    fn a_monolingual_site_never_flags_a_suffix() {
        let config = Config::parse("lang \"en\"\n").expect("config");
        assert_eq!(
            Stem::of(Path::new("post.de.typ"), &config).undeclared(&config),
            None
        );
    }
}
