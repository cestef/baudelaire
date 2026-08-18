//! Errors from reading the content tree: frontmatter that is not the shape a
//! page must declare, names that cannot become URLs, and two outputs claiming
//! one file.

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::error::Annotated;
use crate::ui::{Code, Text, markup};

#[derive(Error, Diagnostic, Debug)]
pub enum ContentError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    BadGlob(Annotated),

    #[error(
        "`frontmatter` in {} must be a dictionary, but is a {ty}: {}",
        Text(.path),
        Text(.repr)
    )]
    #[diagnostic(
        code(baudelaire::content::frontmatter_not_dict),
        help("export the fields as a dict: `#let frontmatter = (key: value, ...)`")
    )]
    FrontmatterNotDict {
        path: String,
        ty: &'static str,
        repr: String,
    },

    #[error("frontmatter {} in {} names no such thing: {}", Code(.key), Text(.path), Code(.got))]
    #[diagnostic(code(baudelaire::content::frontmatter_name), help("{help}"))]
    FrontmatterName {
        path: String,
        key: String,
        got: String,
        help: String,
        #[source_code]
        page: Option<NamedSource<String>>,
        #[label("not one of the names this key takes")]
        span: Option<SourceSpan>,
    },

    #[error(
        "frontmatter {} in {} must be {expected}, but is {got}",
        Code(.key),
        Text(.path)
    )]
    #[diagnostic(code(baudelaire::content::frontmatter_field))]
    FrontmatterField {
        path: String,
        key: String,
        expected: &'static str,
        got: String,
        #[help]
        help: Option<String>,
        #[source_code]
        page: Option<NamedSource<String>>,
        // `expected` is one of this crate's own literals, never authored text:
        // a label is rendered raw rather than as markup.
        #[label("must be {expected}")]
        span: Option<SourceSpan>,
    },

    #[error(
        "frontmatter {} in {} has a {} segment",
        Code(.key),
        Text(.path),
        Code("..")
    )]
    #[diagnostic(
        code(baudelaire::content::frontmatter_traversal),
        help("a page's URL cannot point outside the output directory")
    )]
    FrontmatterTraversal {
        path: String,
        key: String,
        #[source_code]
        page: Option<NamedSource<String>>,
        #[label("would write outside `dist`")]
        span: Option<SourceSpan>,
    },

    #[error("{} declares frontmatter with the removed `#frontmatter(..)` call", Text(.path))]
    #[diagnostic(
        code(baudelaire::content::frontmatter_call),
        help("export it instead: `#let frontmatter = (title: \"..\")`")
    )]
    FrontmatterCall { path: String },

    #[error("{} declares `#let frontmatter` without a value", Text(.path))]
    #[diagnostic(
        code(baudelaire::content::frontmatter_uninit),
        help("give it a dict: `#let frontmatter = (title: \"..\")`")
    )]
    FrontmatterUninit { path: String },

    #[error("unknown frontmatter key {} in {}", Code(.key), Text(.path))]
    #[diagnostic(code(baudelaire::content::unknown_frontmatter_key))]
    UnknownFrontmatterKey {
        path: String,
        key: String,
        #[help]
        help: String,
        #[source_code]
        page: Option<NamedSource<String>>,
        #[label("no such frontmatter key")]
        span: Option<SourceSpan>,
    },

    #[error("{} has no URL-safe characters, so its slug would be empty", Code(.name))]
    #[diagnostic(
        code(baudelaire::content::empty_slug),
        help("give it a `slug` with at least one ASCII letter or digit")
    )]
    EmptySlug { name: String },

    #[error("{} has a filename that is not valid UTF-8", Text(.path))]
    #[diagnostic(
        code(baudelaire::content::non_utf8_source),
        help("rename it: a page's filename becomes its slug, and a URL is text")
    )]
    NonUtf8Source { path: String },

    #[error("{} declares unknown language {}", Text(.path), Code(.lang))]
    #[diagnostic(code(baudelaire::content::unknown_language))]
    UnknownLanguage {
        path: String,
        lang: String,
        #[help]
        help: String,
    },

    /// Never a fallback to reading the name as a path: a page could then reach
    /// any file the build can open.
    #[error("{} names source {}, which the config does not declare", Text(.path), Code(.name))]
    #[diagnostic(code(baudelaire::content::unknown_source))]
    UnknownSource {
        path: String,
        name: String,
        #[help]
        help: String,
        #[source_code]
        page: Option<NamedSource<String>>,
        #[label("no declaration under this name")]
        span: Option<SourceSpan>,
    },

    #[error("{} has both a {} and a body of its own", Text(.path), Code("source"))]
    #[diagnostic(
        code(baudelaire::content::source_and_body),
        help(
            "a sourced page is a frontmatter block and nothing else: move the prose into the sourced file, or drop the `source`"
        )
    )]
    SourceAndBody {
        path: String,
        #[source_code]
        page: Option<NamedSource<String>>,
        #[label("this names a body, and the page has one below")]
        span: Option<SourceSpan>,
    },

    #[error("source {} names {}, which is not a body this build can read", Code(.name), Text(.path))]
    #[diagnostic(code(baudelaire::content::source_unreadable))]
    SourceUnreadable {
        name: String,
        path: String,
        #[help]
        help: String,
        #[source_code]
        page: Option<NamedSource<String>>,
        #[label("declared as a file nothing here reads")]
        span: Option<SourceSpan>,
    },

    #[error("{} is a typst page, so its {} would replace what typst compiles", Text(.path), Code("source"))]
    #[diagnostic(
        code(baudelaire::content::source_on_typst),
        help(
            "a typst page reaches a declared file by name: `#import \"@baudelaire/sources:0.1.0\": <name>`, then `#include <name>`"
        )
    )]
    SourceOnTypst {
        path: String,
        #[source_code]
        page: Option<NamedSource<String>>,
        #[label("a typst page's body is its own")]
        span: Option<SourceSpan>,
    },

    #[error("{} and {} both write {}", Code(.first), Code(.second), Code(.target))]
    #[diagnostic(
        code(baudelaire::content::collision),
        help(
            "two outputs cannot share a file: rename one, set a distinct `slug`/`path`, or drop the clashing `redirect`"
        )
    )]
    Collision {
        target: String,
        first: String,
        second: String,
    },

    #[error(
        "terms {} and {} of {} both slug to {}",
        Code(.first),
        Code(.second),
        Code(.taxonomy),
        Code(.slug)
    )]
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
    /// Lowers wax's span-annotated glob error into an [`Annotated`]; `noun`
    /// names what the pattern configures, so a bad `serve { exclude }` does not
    /// report itself as a collection glob.
    pub fn bad_glob(noun: &'static str, pattern: &str, error: wax::BuildError) -> Self {
        let mut diag = Annotated::new(
            "baudelaire::content::bad_glob",
            markup!("invalid {} glob `{}`", noun, pattern),
            pattern.to_owned(),
        );
        for location in error.locations() {
            let (offset, len) = location.span();
            diag = diag.label(location.to_string(), offset, len);
        }
        Self::BadGlob(diag.help(Text(error).to_string()))
    }

    pub fn frontmatter_not_dict(path: &std::path::Path, value: &typst::foundations::Value) -> Self {
        use typst::foundations::Repr;
        Self::FrontmatterNotDict {
            path: path.display().to_string(),
            ty: value.ty().long_name(),
            repr: value.repr().to_string(),
        }
    }

    /// A known frontmatter key whose value has the wrong type; `span` is where
    /// the value sits in `source`.
    pub fn frontmatter_field(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        key: &str,
        expected: &'static str,
        got: &str,
        help: Option<&'static str>,
    ) -> Self {
        let (page, span) = Self::located(path, source, span);
        Self::FrontmatterField {
            path: path.display().to_string(),
            key: key.to_owned(),
            expected,
            got: got.to_owned(),
            help: help.map(str::to_owned),
            page,
            span,
        }
    }

    /// A name a frontmatter key does not know, where the key takes names from a
    /// fixed set rather than free text; `help` arrives as *authored markup*
    /// from [`Keys::help`](crate::config::dispatch::Keys::help).
    pub fn frontmatter_name(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        key: &str,
        got: &str,
        help: &str,
    ) -> Self {
        let (page, span) = Self::located(path, source, span);
        Self::FrontmatterName {
            path: path.display().to_string(),
            key: key.to_owned(),
            got: got.to_owned(),
            help: help.to_owned(),
            page,
            span,
        }
    }

    /// A frontmatter URL key whose value would escape the output directory.
    pub fn frontmatter_traversal(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        key: &str,
    ) -> Self {
        let (page, span) = Self::located(path, source, span);
        Self::FrontmatterTraversal {
            path: path.display().to_string(),
            key: key.to_owned(),
            page,
            span,
        }
    }

    /// A frontmatter key that is a near-miss of a known one; `span` is where
    /// the key itself was written, not its value.
    pub fn unknown_frontmatter(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        key: &str,
        suggestion: &str,
    ) -> Self {
        let (page, span) = Self::located(path, source, span);
        Self::UnknownFrontmatterKey {
            path: path.display().to_string(),
            key: key.to_owned(),
            help: markup!("did you mean `{}`?", suggestion),
            page,
            span,
        }
    }

    /// The page source a snippet renders from and the span into it; both are
    /// `None` when the frontmatter was computed rather than written, so the
    /// snippet is suppressed rather than aimed at an arbitrary offset.
    fn located(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
    ) -> (Option<NamedSource<String>>, Option<SourceSpan>) {
        let page = span.map(|_| NamedSource::new(path.display().to_string(), source.to_owned()));
        (page, span)
    }

    pub fn frontmatter_call(path: &std::path::Path) -> Self {
        Self::FrontmatterCall {
            path: path.display().to_string(),
        }
    }

    /// `name` is a filename stem, a frontmatter slug, or a taxonomy term.
    pub fn empty_slug(name: &str) -> Self {
        Self::EmptySlug {
            name: name.to_owned(),
        }
    }

    pub fn non_utf8_source(path: &std::path::Path) -> Self {
        Self::NonUtf8Source {
            path: path.display().to_string(),
        }
    }

    pub fn unknown_language(path: &std::path::Path, lang: &str, known: &[&str]) -> Self {
        Self::UnknownLanguage {
            path: path.display().to_string(),
            lang: lang.to_owned(),
            help: markup!(
                "declare it under `languages`, or use one of: {}",
                known.join(", ")
            ),
        }
    }

    /// A page naming a declared source of a kind no reader claims, named by the
    /// key rather than the page: the declaration is what has to change.
    ///
    /// The snippet is the *page's* text, so `page` is what [`Self::located`]
    /// gets and `declared` only ever reaches the message.
    pub fn source_unreadable(
        page: &std::path::Path,
        name: &str,
        declared: &std::path::Path,
        readable: &[&str],
        source: &str,
        span: Option<SourceSpan>,
    ) -> Self {
        let kinds = readable
            .iter()
            .map(|ext| format!(".{ext}"))
            .collect::<Vec<_>>()
            .join(", ");
        let (page, span) = Self::located(page, source, span);
        Self::SourceUnreadable {
            name: name.to_owned(),
            path: declared.display().to_string(),
            help: markup!(
                "a source is read as the dialect its extension names: {}",
                kinds
            ),
            page,
            span,
        }
    }

    pub fn unknown_source(
        path: &std::path::Path,
        name: &str,
        declared: &[&str],
        source: &str,
        span: Option<SourceSpan>,
    ) -> Self {
        let help = if declared.is_empty() {
            markup!(
                "declare it: `paths {{ sources {{ {} \"../FILE.md\" }} }}`",
                name
            )
        } else {
            markup!(
                "declare it under `paths {{ sources }}`, or use one of: {}",
                declared.join(", ")
            )
        };
        let (page, span) = Self::located(path, source, span);
        Self::UnknownSource {
            path: path.display().to_string(),
            name: name.to_owned(),
            help,
            page,
            span,
        }
    }

    /// A sourced page that also wrote a body under its frontmatter; refused
    /// rather than resolved, since either resolution silently drops prose.
    pub fn source_and_body(path: &std::path::Path, source: &str, span: Option<SourceSpan>) -> Self {
        let (page, span) = Self::located(path, source, span);
        Self::SourceAndBody {
            path: path.display().to_string(),
            page,
            span,
        }
    }

    pub fn source_on_typst(path: &std::path::Path, source: &str, span: Option<SourceSpan>) -> Self {
        let (page, span) = Self::located(path, source, span);
        Self::SourceOnTypst {
            path: path.display().to_string(),
            page,
            span,
        }
    }

    pub fn collision(target: &str, first: &str, second: &str) -> Self {
        Self::Collision {
            target: target.to_owned(),
            first: first.to_owned(),
            second: second.to_owned(),
        }
    }

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
