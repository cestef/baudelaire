//! Errors from lowering a markdown page to Typst: a frontmatter block that
//! never closes, one that does not parse as its fence's dialect, and raw HTML.

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::ui::{Code, Text};

#[derive(Error, Diagnostic, Debug)]
pub enum MarkdownError {
    #[error("frontmatter in {} is never closed", Text(.path))]
    #[diagnostic(
        code(baudelaire::markdown::unterminated_frontmatter),
        help("close the block with a {} line of its own, or remove the opening one", Code(.fence))
    )]
    UnterminatedFrontmatter {
        path: String,
        /// The fence that opened it, which is the only one that closes it.
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

    #[error("frontmatter {} in {} is declared twice", Code(.key), Text(.path))]
    #[diagnostic(
        code(baudelaire::markdown::duplicate_key),
        help("the later one wins, which is unlikely to be what was meant; delete one")
    )]
    DuplicateKey {
        path: String,
        key: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("already declared above")]
        span: SourceSpan,
    },

    #[error("frontmatter {} in {} is written as both a value and a dictionary", Code(.key), Text(.path))]
    #[diagnostic(
        code(baudelaire::markdown::ambiguous_node),
        help(
            "a key holds either a value (`author \"cstef\"`) or fields (`author role=\"editor\"`, or a \
             block), not both; move the argument into a field of its own"
        )
    )]
    AmbiguousNode {
        path: String,
        key: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("this argument would be dropped")]
        span: SourceSpan,
    },

    #[error("frontmatter in {} is not valid {}", Text(.path), Text(.dialect))]
    #[diagnostic(code(baudelaire::markdown::frontmatter))]
    Frontmatter {
        path: String,
        dialect: String,
        /// What that dialect's block looks like when it is right.
        #[help]
        hint: String,
        #[source_code]
        src: NamedSource<String>,
        /// The dialect's own diagnostics, every span rebased onto the file: a
        /// block is parsed on its own, so its spans are relative to the block.
        #[related]
        faults: Vec<FrontmatterFault>,
    },
}

/// One fault a parser found inside a frontmatter block, rebased onto the page.
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
    ///
    /// The message and the help are escaped, the label not:
    /// [`Styled`](crate::ui::Styled) reads a `#[related]` diagnostic as this
    /// crate's markup, and miette forwards a label untouched.
    pub fn rebased(fault: &kdl::KdlDiagnostic, offset: usize) -> Self {
        Self {
            message: fault
                .message
                .as_ref()
                .map_or_else(|| "invalid KDL".to_owned(), |m| Text(m).to_string()),
            span: SourceSpan::new((fault.span.offset() + offset).into(), fault.span.len()),
            label: fault.label.clone().unwrap_or_else(|| "here".to_owned()),
            help: fault.help.as_ref().map(|help| Text(help).to_string()),
        }
    }
}

impl From<MarkdownError> for crate::error::BaudelaireErrorKind {
    fn from(e: MarkdownError) -> Self {
        Self::Markdown(Box::new(e))
    }
}
