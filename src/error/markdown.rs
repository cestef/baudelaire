//! Errors from lowering a markdown page to Typst.
//!
//! Three things can go wrong that are the author's to fix: a frontmatter block
//! that never closes, one that does not parse as the dialect its fence opened,
//! and raw HTML, which this pipeline has nowhere to put.
//!
//! Nothing here is per-dialect. A block reports the language it was read as and
//! carries that dialect's own diagnostics, rebased onto the page, so a new
//! dialect adds no variant.
//!
//! A block that parses but names a key no page has is *not* here: that is a
//! [`crate::error::ContentError`], raised by the same walk and the same typo
//! suggester a typst page's frontmatter goes through, which is what makes a
//! misspelled `titel` say "did you mean `title`?".

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::ui::{Code, Text};

/// A failure while reading a `.md` page.
#[derive(Error, Diagnostic, Debug)]
pub enum MarkdownError {
    #[error("frontmatter in {} is never closed", Text(.path))]
    #[diagnostic(
        code(baudelaire::markdown::unterminated_frontmatter),
        help("close the block with a {} line of its own, or remove the opening one", Code(.fence))
    )]
    UnterminatedFrontmatter {
        path: String,
        /// The fence that opened it, which is the only one that closes it: a
        /// help naming `---` under a `+++` block sends the reader to write the
        /// one spelling that would not have worked.
        fence: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("this block is never closed")]
        span: SourceSpan,
    },

    #[error("{} contains raw HTML", Text(.path))]
    #[diagnostic(
        code(baudelaire::markdown::raw_html),
        help(
            "the DOM a build produces is typed, so markup cannot be spliced in as text; write it \
             in a ```typ fence instead, with `html.elem(\"div\")[..]`"
        )
    )]
    RawHtml {
        path: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("this markup has nowhere to go")]
        span: SourceSpan,
    },

    #[error("frontmatter in {} is not valid {}", Text(.path), Text(.dialect))]
    #[diagnostic(code(baudelaire::markdown::frontmatter))]
    Frontmatter {
        path: String,
        /// The language the block was read as, so the message names what it
        /// failed to be rather than what it most often is.
        dialect: String,
        /// What that dialect's block looks like when it is right. Carried rather
        /// than written here, so a new dialect brings its own and this variant
        /// stays the one frontmatter-syntax error there is.
        #[help]
        hint: String,
        #[source_code]
        src: NamedSource<String>,
        /// The dialect's own diagnostics, shifted onto the file the author
        /// wrote: the block is parsed on its own, so every span it reports is
        /// relative to the block and would underline the wrong line here.
        #[related]
        faults: Vec<FrontmatterFault>,
    },
}

/// One fault a parser found inside a frontmatter block, rebased onto the page.
///
/// Kept as a diagnostic of its own rather than flattened into a message: a
/// parser reports a span and often a help per fault, and folding them into one
/// string is exactly the coercion `error/mod.rs` forbids.
///
/// Dialect-agnostic on purpose. KDL reports several faults at once and each
/// carries its own help; YAML and TOML report one, with none. Both are a list of
/// this.
#[derive(Error, Diagnostic, Debug)]
#[error("{message}")]
#[diagnostic(code(baudelaire::markdown::frontmatter_fault))]
pub struct FrontmatterFault {
    message: String,
    #[label("{label}")]
    span: SourceSpan,
    label: String,
    #[help]
    help: Option<String>,
}

impl FrontmatterFault {
    /// A fault a parser reported at a span already rebased onto the file.
    ///
    /// What a dialect with one error and no help of its own builds: everything
    /// it knows is the message and where.
    pub fn at(message: String, span: std::ops::Range<usize>) -> Self {
        Self {
            message,
            span: SourceSpan::new(span.start.into(), span.len()),
            label: "here".to_owned(),
            help: None,
        }
    }

    /// Rebase one of kdl's diagnostics by `offset`, the byte position the block
    /// starts at in the file.
    pub fn rebased(fault: &kdl::KdlDiagnostic, offset: usize) -> Self {
        Self {
            message: fault
                .message
                .clone()
                .unwrap_or_else(|| "invalid KDL".to_owned()),
            span: SourceSpan::new((fault.span.offset() + offset).into(), fault.span.len()),
            label: fault.label.clone().unwrap_or_else(|| "here".to_owned()),
            help: fault.help.clone(),
        }
    }
}

impl From<MarkdownError> for crate::error::BaudelaireErrorKind {
    fn from(e: MarkdownError) -> Self {
        Self::Markdown(Box::new(e))
    }
}
