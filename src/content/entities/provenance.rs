//! Where an entity was declared, and how to point at it.
//!
//! A registry is assembled from several sources, so "which file said this" is
//! the first thing a diagnostic has to answer, and "where in it" is the second.
//! Both live here, so an error type never has to know that a roster is KDL and a
//! profile is frontmatter.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use miette::{NamedSource, SourceSpan};

use crate::content::frontmatter::check::Step;
use crate::content::frontmatter::origin::Located;
use crate::world::Project;

/// A file to render a snippet from, and the span in it to underline.
pub struct Snippet {
    pub source: NamedSource<String>,
    pub span: Option<SourceSpan>,
}

impl Snippet {
    /// The two halves a diagnostic carries, for a snippet that may not exist.
    ///
    /// Every entity error is optional in both, and pairing them here is what
    /// keeps a variant from being given a source with no span or a span with no
    /// source: miette renders the first as an unmarked file and the second as
    /// nothing at all.
    pub fn parts(snippet: Option<Self>) -> (Option<NamedSource<String>>, Option<SourceSpan>) {
        snippet.map_or((None, None), |it| (Some(it.source), it.span))
    }
}

/// Where an entity came from.
#[derive(Debug, Clone)]
pub enum Provenance {
    /// A profile page's frontmatter. The page is not held open: it is re-read
    /// only if something needs to point inside it.
    Page { path: PathBuf },
    /// A KDL roster: a `data` file, or the `inline` block of the config. Both
    /// are read once, so both keep their text and the spans they were read at.
    Roster {
        /// The source key, as config spells it.
        source: &'static str,
        /// The file it was read from.
        at: String,
        text: Arc<str>,
        /// Where the entity's own node sits.
        entity: SourceSpan,
        /// Where each of its fields sits.
        fields: BTreeMap<String, SourceSpan>,
    },
}

impl Provenance {
    /// The source that declared it, spelled as its config key.
    ///
    /// Two accessors rather than one assembled phrase: a diagnostic escapes
    /// what it interpolates, so a label that arrived carrying its own markup
    /// would render its delimiters as text.
    pub fn source(&self) -> &'static str {
        match self {
            Self::Page { .. } => "pages",
            Self::Roster { source, .. } => source,
        }
    }

    /// The file it was written in.
    pub fn at(&self) -> String {
        match self {
            Self::Page { path } => path.display().to_string(),
            Self::Roster { at, .. } => at.clone(),
        }
    }

    /// The file and span to underline for the value `steps` names, falling back
    /// to the entity itself where the value has no place of its own.
    ///
    /// `None` leaves the diagnostic snippet-less, which is what a frontmatter
    /// that cannot be read into (computed, imported) has always produced: a
    /// message with no snippet beats one underlining an arbitrary offset.
    pub(crate) fn snippet(&self, project: &Project, steps: &[Step]) -> Option<Snippet> {
        match self {
            Self::Page { path } => {
                let located = Located::of(path, project)?;
                Some(Snippet {
                    span: located.span(steps),
                    source: NamedSource::new(path.display().to_string(), located.text().to_owned())
                        .with_language("typst"),
                })
            }
            Self::Roster {
                at,
                text,
                entity,
                fields,
                ..
            } => Some(Snippet {
                // A roster records a span per field, so a nested step resolves
                // to the field that holds it rather than to nothing.
                span: Some(match steps.first() {
                    Some(Step::Key(key)) => fields.get(key).copied().unwrap_or(*entity),
                    _ => *entity,
                }),
                source: NamedSource::new(at, text.to_string()).with_language("kdl"),
            }),
        }
    }
}
