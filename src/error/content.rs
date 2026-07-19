use miette::Diagnostic;
use thiserror::Error;

use crate::error::Annotated;

#[derive(Error, Diagnostic, Debug)]
pub enum ContentError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    BadGlob(Annotated),

    #[error("`frontmatter` in {path} must be a dictionary, but is a {ty}: {repr}")]
    #[diagnostic(
        code(baudelaire::content::frontmatter_not_dict),
        help("export the fields as a dict: `#let frontmatter = (key: value, ...)`")
    )]
    FrontmatterNotDict {
        path: String,
        ty: &'static str,
        repr: String,
    },

    #[error("frontmatter `{key}` in {path} must be {expected}, but is a {got}")]
    #[diagnostic(code(baudelaire::content::frontmatter_field))]
    FrontmatterField {
        path: String,
        key: String,
        expected: &'static str,
        got: String,
        #[help]
        help: Option<String>,
    },

    #[error("{path} declares frontmatter with the removed `#frontmatter(..)` call")]
    #[diagnostic(
        code(baudelaire::content::frontmatter_call),
        help("export it instead: `#let frontmatter = (title: \"..\")`")
    )]
    FrontmatterCall { path: String },

    #[error("{path} declares `#let frontmatter` without a value")]
    #[diagnostic(
        code(baudelaire::content::frontmatter_uninit),
        help("give it a dict: `#let frontmatter = (title: \"..\")`")
    )]
    FrontmatterUninit { path: String },

    #[error("unknown frontmatter key `{key}` in {path}")]
    #[diagnostic(code(baudelaire::content::unknown_frontmatter_key))]
    UnknownFrontmatterKey {
        path: String,
        key: String,
        #[help]
        help: String,
    },

    #[error("`{name}` has no URL-safe characters, so its slug would be empty")]
    #[diagnostic(
        code(baudelaire::content::empty_slug),
        help("give it a `slug` with at least one ASCII letter or digit")
    )]
    EmptySlug { name: String },

    #[error("{path} declares unknown language `{lang}`")]
    #[diagnostic(code(baudelaire::content::unknown_language))]
    UnknownLanguage {
        path: String,
        lang: String,
        #[help]
        help: String,
    },

    #[error("`{first}` and `{second}` both write `{target}`")]
    #[diagnostic(
        code(baudelaire::content::collision),
        help(
            "two outputs cannot share a file: rename one, set a distinct `slug`/`permalink`, or drop the clashing `redirect`"
        )
    )]
    Collision {
        target: String,
        first: String,
        second: String,
    },

    #[error("terms `{first}` and `{second}` of `{taxonomy}` both slug to `{slug}`")]
    #[diagnostic(
        code(baudelaire::content::term_collision),
        help("two terms cannot share a URL: rename one so their slugs differ")
    )]
    TermCollision {
        taxonomy: String,
        slug: String,
        first: String,
        second: String,
    },
}

impl ContentError {
    /// Lower wax's own span-annotated glob error into an [`Annotated`] so its
    /// labels point straight at the offending part of the pattern.
    pub fn bad_glob(pattern: &str, error: wax::BuildError) -> Self {
        let mut diag = Annotated::new(
            "baudelaire::content::bad_glob",
            format!("invalid collection glob `{pattern}`"),
            pattern.to_owned(),
        )
        .help(error.to_string());
        for location in error.locations() {
            let (offset, len) = location.span();
            diag = diag.label(location.to_string(), offset, len);
        }
        Self::BadGlob(diag)
    }

    pub fn frontmatter_not_dict(path: &std::path::Path, value: &typst::foundations::Value) -> Self {
        use typst::foundations::Repr;
        Self::FrontmatterNotDict {
            path: path.display().to_string(),
            ty: value.ty().long_name(),
            repr: value.repr().to_string(),
        }
    }

    /// A known frontmatter key whose value has the wrong type; previously
    /// dropped silently (`title: 3` vanished, `draft: "yes"` became `false`).
    pub fn frontmatter_field(
        path: &std::path::Path,
        key: &str,
        expected: &'static str,
        got: &'static str,
        help: Option<&'static str>,
    ) -> Self {
        Self::FrontmatterField {
            path: path.display().to_string(),
            key: key.to_owned(),
            expected,
            got: got.to_owned(),
            help: help.map(str::to_owned),
        }
    }

    /// A frontmatter key that is a near-miss of a known one (a typo). Unknown
    /// keys with no close match pass through to `extra` untouched.
    pub fn unknown_frontmatter(path: &std::path::Path, key: &str, suggestion: &str) -> Self {
        Self::UnknownFrontmatterKey {
            path: path.display().to_string(),
            key: key.to_owned(),
            help: format!("did you mean `{suggestion}`?"),
        }
    }

    /// The pre-export `#frontmatter(..)` call form, pointed at the binding syntax.
    pub fn frontmatter_call(path: &std::path::Path) -> Self {
        Self::FrontmatterCall {
            path: path.display().to_string(),
        }
    }

    /// A name (filename stem, frontmatter slug, or taxonomy term) with no
    /// URL-safe characters.
    pub fn empty_slug(name: &str) -> Self {
        Self::EmptySlug {
            name: name.to_owned(),
        }
    }

    /// A page's resolved language is not among the declared `languages`.
    pub fn unknown_language(path: &std::path::Path, lang: &str, known: &[&str]) -> Self {
        Self::UnknownLanguage {
            path: path.display().to_string(),
            lang: lang.to_owned(),
            help: format!("declare it under `languages`, or use one of: {}", known.join(", ")),
        }
    }

    /// Two outputs resolving to the same file (a silent overwrite otherwise).
    pub fn collision(target: &str, first: &str, second: &str) -> Self {
        Self::Collision {
            target: target.to_owned(),
            first: first.to_owned(),
            second: second.to_owned(),
        }
    }

    /// Two taxonomy terms slugging to the same URL.
    pub fn term_collision(taxonomy: &str, slug: &str, first: &str, second: &str) -> Self {
        Self::TermCollision {
            taxonomy: taxonomy.to_owned(),
            slug: slug.to_owned(),
            first: first.to_owned(),
            second: second.to_owned(),
        }
    }
}

impl From<ContentError> for crate::error::BaudelaireErrorKind {
    fn from(e: ContentError) -> Self {
        Self::Content(Box::new(e))
    }
}
