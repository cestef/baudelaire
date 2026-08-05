//! Frontmatter that does not satisfy its collection's declared schema.
//!
//! A collection declaring `schema { title "str" }` is saying that every one of
//! its pages carries a title of that shape. Without the declaration a page that
//! omits it, or writes the wrong thing, builds green and renders an empty hole
//! wherever the template read it. These are the two ways that goes wrong, each
//! carrying the page source and a span into it so the offending line is
//! underlined rather than merely named.

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode, SourceSpan};
use thiserror::Error;

use crate::config::FieldType;
use crate::ui::{Code, markup};

/// A page whose frontmatter breaks its collection's schema.
#[derive(Error, Debug)]
#[error("{kind}")]
pub struct SchemaError {
    /// The page source, named by its path, so the snippet says which file.
    page: NamedSource<String>,
    /// Where in that source to point. `None` when the frontmatter is not a
    /// literal a dialect can locate (a typst page's may be computed,
    /// or imported), which suppresses the snippet rather than pointing at an
    /// arbitrary offset.
    span: Option<SourceSpan>,
    kind: SchemaErrorKind,
}

impl SchemaError {
    /// A required field the page never declared. `span` points at whatever
    /// should have held it, since what is missing has no place of its own.
    ///
    /// `key` is dotted for a field inside a dictionary (`author.email`), which
    /// names it exactly but is not how it would be written: the help says where
    /// the leaf goes instead.
    pub fn missing(
        page: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        collection: &str,
        key: &str,
        ty: &FieldType,
    ) -> Self {
        let (leaf, parent) = Self::split(key);
        // Two whole strings rather than one with a phrase interpolated: an
        // argument is escaped on the way in, so a nested `markup!` would arrive
        // with its own delimiters written out as text.
        let help = match parent {
            Some(parent) => markup!(
                "add `{}: {}` to `{}` in this page's frontmatter, or declare it `optional=#true` in the `{}` collection's schema",
                leaf,
                ty.example(),
                parent,
                collection
            ),
            None => markup!(
                "add `{}: {}` to this page's frontmatter, or declare it `optional=#true` in the `{}` collection's schema",
                leaf,
                ty.example(),
                collection
            ),
        };
        Self::new(
            page,
            source,
            span,
            SchemaErrorKind::Missing {
                help,
                collection: collection.to_owned(),
                key: key.to_owned(),
            },
        )
    }

    /// A declared field whose value is not the shape the schema asks for.
    /// `span` points at the value itself.
    pub fn mismatch(
        page: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        collection: &str,
        key: &str,
        ty: &FieldType,
        got: &str,
    ) -> Self {
        let (leaf, _) = Self::split(key);
        Self::new(
            page,
            source,
            span,
            SchemaErrorKind::Mismatch {
                help: markup!(
                    "write it as `{}: {}`, or change the `{}` collection's schema",
                    leaf,
                    ty.example(),
                    collection
                ),
                expected: ty.clone(),
                key: key.to_owned(),
                got: got.to_owned(),
            },
        )
    }

    /// A dotted key as the field itself and whatever contains it: what the help
    /// needs, since only the leaf is ever written as a key.
    fn split(key: &str) -> (&str, Option<&str>) {
        match key.rsplit_once('.') {
            Some((parent, leaf)) => (leaf, Some(parent)),
            None => (key, None),
        }
    }

    fn new(
        page: &std::path::Path,
        source: &str,
        span: Option<SourceSpan>,
        kind: SchemaErrorKind,
    ) -> Self {
        Self {
            page: crate::error::PageSource(page.display().to_string(), source.to_owned()).into(),
            span,
            kind,
        }
    }
}

impl Diagnostic for SchemaError {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.code()
    }

    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.help()
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.span.map(|_| &self.page as &dyn SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let span = self.span?;
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some(self.kind.label().to_owned()),
            span,
        ))))
    }
}

/// The two ways a page can break its schema. One error class, two precise
/// reasons: both are located in the page source and reported the same way, and
/// only the wording, the label and the diagnostic code differ.
#[derive(Error, Diagnostic, Debug)]
enum SchemaErrorKind {
    #[error("the {} collection requires frontmatter {}", Code(.collection), Code(.key))]
    #[diagnostic(code(baudelaire::schema::missing))]
    Missing {
        collection: String,
        key: String,
        #[help]
        help: String,
    },

    // "but is of type `x`" rather than "but is a {x}": the type name comes from
    // typst, so no article this side could pick would be right for all of them.
    #[error(
        "frontmatter {} must be {}, but is of type {}",
        Code(.key),
        .expected.article(),
        Code(.got)
    )]
    #[diagnostic(code(baudelaire::schema::mismatch))]
    Mismatch {
        key: String,
        expected: FieldType,
        got: String,
        #[help]
        help: String,
    },
}

impl SchemaErrorKind {
    /// What the underlined span is. Constant, not interpolated: a label is not
    /// markup-rendered, so an authored value in it would land raw in the
    /// output. Which key and which type are already in the message.
    fn label(&self) -> &'static str {
        match self {
            Self::Missing { .. } => "this frontmatter is missing it",
            Self::Mismatch { .. } => "not the declared type",
        }
    }
}

impl From<SchemaError> for crate::error::BaudelaireErrorKind {
    fn from(e: SchemaError) -> Self {
        Self::Schema(Box::new(e))
    }
}
