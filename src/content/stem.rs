//! Decoding a source filename.
//!
//! A content file's name carries more than a slug: an optional language and an
//! optional draft marker, in either order. This is the single place a filename
//! is decoded, shared by slugging, language resolution, and section nesting.

use std::path::Path;

use crate::config::Config;

/// The parsed stem of a source path: its language and draft markers peeled off,
/// leaving the slug. `post.fr.typ` carries language `fr`; `post.draft.typ` is a
/// draft; the two stack in either order (`post.draft.fr.typ`, `post.fr.draft.typ`).
pub(super) struct Stem<'a> {
    /// The stem with both markers peeled: what the page's slug derives from.
    slug: &'a str,
    /// Whether the stem carried the draft marker.
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
        // Peel the two optional trailing markers in whichever order the author
        // wrote them: `post.draft.fr` and `post.fr.draft` both name a French
        // draft. Reading only one order left the other decoding as a
        // default-language page whose slug embedded the code (`post-fr`), so a
        // translated draft published. Two passes, each marker peeled at most
        // once, so a stem that genuinely ends in a language code keeps it.
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
    /// stem before it. The default language uses bare filenames, so `.en` on an
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

    /// The declared language named by the filename, if any.
    pub(super) fn lang(&self) -> Option<&'a str> {
        self.lang
    }

    /// A trailing segment that looks like a language code but is not declared,
    /// on a site that declares languages at all.
    ///
    /// Deliberately narrow: two or three lowercase ASCII letters, so an
    /// ordinary dotted filename (`notes.v2.typ`, `report.2024.typ`) is
    /// untouched. Within that shape it is a misspelt or forgotten `languages`
    /// entry far more often than a filename, and silently publishing it as a
    /// default-language page is the worst of the available answers.
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

    /// A stem decodes the same whichever order the author stacked the markers
    /// in: `post.fr.draft.typ` used to decode as a default-language page slugged
    /// `post-fr`, so a French draft published at a real URL.
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

    /// An undeclared trailing segment is part of the slug, not a language.
    #[test]
    fn an_undeclared_trailing_code_stays_in_the_slug() {
        let config = config();
        let stem = Stem::of(Path::new("post.de.typ"), &config);
        assert_eq!(stem.slug(), "post.de");
        assert_eq!(stem.lang(), None);
    }

    /// ...but on a multilingual site it is flagged rather than published as a
    /// default-language page: `post.de.typ` without a `de` entry is a typo the
    /// same way `lang: "de"` is, and that one always stopped the build.
    #[test]
    fn an_undeclared_code_is_reported_on_a_multilingual_site() {
        let config = config();
        assert_eq!(
            Stem::of(Path::new("post.de.typ"), &config).undeclared(&config),
            Some("de")
        );
        // A declared one resolves, and the default language is always known.
        assert_eq!(
            Stem::of(Path::new("post.fr.typ"), &config).undeclared(&config),
            None
        );
        assert_eq!(
            Stem::of(Path::new("post.en.typ"), &config).undeclared(&config),
            None
        );
        // An ordinary dotted filename is left alone.
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

    /// A single-language site never guesses: it has no `languages` block to
    /// compare against.
    #[test]
    fn a_monolingual_site_never_flags_a_suffix() {
        let config = Config::parse("lang \"en\"\n").expect("config");
        assert_eq!(
            Stem::of(Path::new("post.de.typ"), &config).undeclared(&config),
            None
        );
    }
}
