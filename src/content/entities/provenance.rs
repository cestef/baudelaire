//! Where an entity was declared, and how to point at it, so an error type never
//! has to know that a roster is KDL and a profile is frontmatter.

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
    /// The two halves a diagnostic carries, for a snippet that may not exist,
    /// paired so a variant is never given a span without its source.
    pub fn parts(snippet: Option<Self>) -> (Option<NamedSource<String>>, Option<SourceSpan>) {
        snippet.map_or((None, None), |it| (Some(it.source), it.span))
    }
}

/// Where an entity came from.
#[derive(Debug, Clone)]
pub enum Provenance {
    /// A profile page's frontmatter, re-read only if something needs to point
    /// inside it.
    Page { path: PathBuf },
    /// A KDL roster: a `data` file, or the `inline` block of the config, each
    /// keeping the text and spans it was read at.
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
    /// to the entity itself where the value has no place of its own. `None`
    /// leaves the diagnostic snippet-less, as a frontmatter that cannot be read
    /// into does.
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
                span: Some(match steps.first() {
                    Some(Step::Key(key)) => fields.get(key).copied().unwrap_or(*entity),
                    _ => *entity,
                }),
                source: NamedSource::new(at, text.to_string()).with_language("kdl"),
            }),
        }
    }
}
