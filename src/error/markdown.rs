//! Errors from lowering a markdown page to Typst.
//!
//! Three things can go wrong that are the author's to fix: a frontmatter block
//! that never closes, one that is not KDL, and raw HTML, which this pipeline
//! has nowhere to put.
//!
//! A block that parses but names a key no page has is *not* here: that is a
//! [`crate::error::ContentError`], raised by the same walk and the same typo
//! suggester a typst page's frontmatter goes through, which is what makes
//! pasted YAML say "did you mean `title`?".

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::ui::Text;

/// A failure while reading a `.md` page.
#[derive(Error, Diagnostic, Debug)]
pub enum MarkdownError {
    #[error("frontmatter in {} is never closed", Text(.path))]
    #[diagnostic(
        code(baudelaire::markdown::unterminated_frontmatter),
        help("close the block with a `---` line of its own, or remove the opening one")
    )]
    UnterminatedFrontmatter {
        path: String,
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

    #[error("frontmatter in {} is not valid KDL", Text(.path))]
    #[diagnostic(
        code(baudelaire::markdown::frontmatter),
        help(
            "the block between the `---` fences is KDL, the language `config.kdl` uses: `title \"A page\"`"
        )
    )]
    Frontmatter {
        path: String,
        #[source_code]
        src: NamedSource<String>,
        /// kdl's own diagnostics, shifted onto the file the author wrote: the
        /// block is parsed on its own, so every span it reports is relative to
        /// the block and would underline the wrong line here.
        #[related]
        faults: Vec<FrontmatterFault>,
    },
}

/// One fault kdl found inside a frontmatter block, rebased onto the page.
///
/// Kept as a diagnostic of its own rather than flattened into a message: kdl
/// reports a span and a help per fault, and folding them into one string is
/// exactly the coercion `error/mod.rs` forbids.
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
