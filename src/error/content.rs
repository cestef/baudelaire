//! Errors from reading the content tree: frontmatter that is not the shape a
//! page must declare, names that cannot become URLs, and two outputs claiming
//! one file. Each names the page it came from, since a build reports them long
//! after the file was opened.

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::error::Annotated;
use crate::ui::{Code, Text, markup};

/// A failure while discovering or interpreting a content file.
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
        "frontmatter {} in {} must be {expected}, but is a {got}",
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
        // What the key holds, not what the value is: the message already says
        // the latter, and `expected` is one of this crate's own literals rather
        // than authored text, which a label renders raw.
        #[label("must be {expected}")]
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

    /// A page named a `source` the config does not declare.
    ///
    /// Deliberately not a fallback to reading the name as a path: that would be
    /// the hole this design exists to close, since a page could then reach any
    /// file the build can open. The name is either declared or it is nothing.
    #[error("{} names source {}, which the config does not declare", Text(.path), Code(.name))]
    #[diagnostic(code(baudelaire::content::unknown_source))]
    UnknownSource {
        path: String,
        name: String,
        #[help]
        help: String,
    },

    /// A page carrying both a `source` and a body of its own.
    #[error("{} has both a {} and a body of its own", Text(.path), Code("source"))]
    #[diagnostic(
        code(baudelaire::content::source_and_body),
        help(
            "a sourced page is a frontmatter block and nothing else: move the prose into the sourced file, or drop the `source`"
        )
    )]
    SourceAndBody { path: String },

    /// A `.typ` page carrying a `source`.
    #[error("{} is a typst page, so its {} would replace what typst compiles", Text(.path), Code("source"))]
    #[diagnostic(
        code(baudelaire::content::source_on_typst),
        help(
            "use typst's own `include` for a `.typ` page, which tracks the file it reads; `source` is for markdown, which has no include"
        )
    )]
    SourceOnTypst { path: String },

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
    /// Lower wax's own span-annotated glob error into an [`Annotated`] so its
    /// labels point straight at the offending part of the pattern.
    /// `noun` names what the pattern configures, so a bad `serve { exclude }`
    /// does not report itself as a collection glob.
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
        // wax's own message: foreign text, escaped rather than read as markup.
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

    /// A known frontmatter key whose value has the wrong type; previously
    /// dropped silently (`title: 3` vanished, `draft: "yes"` became `false`).
    ///
    /// `span` is where the value sits in `source`; see [`Self::located`].
    pub fn frontmatter_field(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        key: &str,
        expected: &'static str,
        got: &'static str,
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
    /// fixed set rather than free text.
    ///
    /// Distinct from [`ContentError::frontmatter_field`], which is about the
    /// value's *shape*: here the shape is right and the word is not one this
    /// crate answers to, so the help is the list of words that are.
    pub fn frontmatter_name(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        key: &str,
        got: &str,
        valid: &str,
    ) -> Self {
        let (page, span) = Self::located(path, source, span);
        Self::FrontmatterName {
            path: path.display().to_string(),
            key: key.to_owned(),
            got: got.to_owned(),
            help: markup!("valid names: {}", valid),
            page,
            span,
        }
    }

    /// A frontmatter key that is a near-miss of a known one (a typo). Unknown
    /// keys with no close match pass through to `extra` untouched.
    ///
    /// `span` is where the key itself was written, not its value: the mistake
    /// is in the key, and that is what the label underlines.
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

    /// The page source a snippet renders from and the span into it, as the one
    /// pair a located diagnostic is built from.
    ///
    /// A page may compute its frontmatter rather than spell it out
    /// (`#let frontmatter = load(..)`), and then no offset in the source is the
    /// thing at fault: the source is dropped along with the span, so the snippet
    /// is suppressed rather than aimed at an arbitrary offset. Both come back
    /// together so no variant can carry one without the other. `SchemaError`
    /// draws the same line, by hand.
    fn located(
        path: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
    ) -> (Option<NamedSource<String>>, Option<SourceSpan>) {
        let page = span.map(|_| NamedSource::new(path.display().to_string(), source.to_owned()));
        (page, span)
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

    /// A content file whose name cannot be read as text.
    pub fn non_utf8_source(path: &std::path::Path) -> Self {
        Self::NonUtf8Source {
            path: path.display().to_string(),
        }
    }

    /// A page's resolved language is not among the declared `languages`.
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

    /// A page naming a source the config never declared. The help lists what it
    /// did declare, since the name is the only thing a page may write and a
    /// typo is otherwise indistinguishable from a missing declaration.
    pub fn unknown_source(path: &std::path::Path, name: &str, declared: &[&str]) -> Self {
        let help = match declared.is_empty() {
            true => markup!(
                "declare it: `paths {{ sources {{ {} \"../FILE.md\" }} }}`",
                name
            ),
            false => markup!(
                "declare it under `paths {{ sources }}`, or use one of: {}",
                declared.join(", ")
            ),
        };
        Self::UnknownSource {
            path: path.display().to_string(),
            name: name.to_owned(),
            help,
        }
    }

    /// A sourced page that also wrote a body under its frontmatter. Refused
    /// rather than resolved either way: one of the two would be dropped, and
    /// dropping the prose somebody wrote is not something to do quietly.
    pub fn source_and_body(path: &std::path::Path) -> Self {
        Self::SourceAndBody {
            path: path.display().to_string(),
        }
    }

    /// A `source` on a `.typ` page, where typst's own `include` is the answer.
    pub fn source_on_typst(path: &std::path::Path) -> Self {
        Self::SourceOnTypst {
            path: path.display().to_string(),
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
