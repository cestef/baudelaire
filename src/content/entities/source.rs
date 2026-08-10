//! Where a registry's entities are read from.
//!
//! One loader per [`SourceConfig`] variant, and the dispatcher that pairs them.
//! Adding a source is a payload struct beside its siblings in
//! [`crate::config::content::entities::source`], one variant, one row in that
//! module's table, one `impl Source` here, and one arm in [`SourceConfig::loader`],
//! which the compiler will demand.
//!
//! What a loader must not do is read a file behind the build's back. A `pages`
//! source draws on pages the plan already read; a `data` source goes through the
//! project's file store, the way every other tracked read does.

use std::path::PathBuf;
use std::sync::Arc;

use crate::codegen::Value;
use crate::config::{Config, DataSource, Declared, InlineSource, PagesSource, SourceConfig};
use crate::content::{Frontmatter, Page};
use crate::error::Result;
use crate::world::Project;

use super::{Entity, Provenance};

/// What a loader may read: the plan's own inputs, and nothing else.
pub struct SourceCtx<'a> {
    pub config: &'a Config,
    pub project: &'a Project,
    /// The pages discovery found, before any generated page joins them.
    pub pages: &'a [Page],
    /// The registry being loaded, for a diagnostic that has to name it.
    pub registry: &'a str,
}

/// One place entities come from.
pub trait Source {
    /// The entities this source declares, in the order it declares them.
    fn load(&self, cx: &SourceCtx<'_>) -> Result<Vec<Entity>>;
}

impl SourceConfig {
    /// The loader for this declaration.
    pub(super) fn loader(&self) -> &dyn Source {
        match self {
            Self::Pages(source) => source,
            Self::Data(source) => source,
            Self::Inline(source) => source,
        }
    }
}

/// The frontmatter keys a profile page contributes as fields under their own
/// names, beside everything it declares that this crate does not name.
///
/// Three, and each because a profile would otherwise have to write the same
/// thing twice: a page's title is its name, its description is its blurb, its
/// image is its picture. Everything else a shape asks for (`url`, `avatar`,
/// `socials`) is not a built-in frontmatter key, so it already arrives as
/// declared.
type Builtin = (&'static str, fn(&Frontmatter) -> Option<String>);

const BUILTIN: &[Builtin] = &[
    ("title", |fm| fm.title.clone()),
    ("description", |fm| fm.blurb().map(str::to_owned)),
    ("image", |fm| fm.image.clone()),
];

/// Entities are the content pages under a directory.
///
/// Their frontmatter is the fields and their body is the prose, which is what
/// makes a profile an ordinary page: it has a permalink, an edition per
/// language, a card, and per-page cache tracking, none of which a roster format
/// could give it.
impl Source for PagesSource {
    fn load(&self, cx: &SourceCtx<'_>) -> Result<Vec<Entity>> {
        let dir = cx.project.root().join(&self.dir);
        let mut out = Vec::new();
        for page in cx.pages.iter().filter(|page| page.authored()) {
            if !crate::fs::canonical(&page.source).starts_with(&dir) {
                continue;
            }
            let fm = &page.frontmatter;
            let mut fields: Vec<(String, Value)> = BUILTIN
                .iter()
                .filter_map(|(key, read)| Some(((*key).to_owned(), Value::Str(read(fm)?))))
                .collect();
            for (key, value) in &fm.extra {
                fields.push((key.clone(), value.clone()));
            }
            out.push(Entity::new(
                page.id.slug(),
                fields,
                Provenance::Page {
                    path: page.source.clone(),
                },
            )?);
        }
        Ok(out)
    }
}

/// Entities are the nodes of a KDL roster file.
///
/// Read through [`Project::source`], so the file is opened by the same store
/// every other build input goes through and the dev server watches it without
/// being told to.
impl Source for DataSource {
    fn load(&self, cx: &SourceCtx<'_>) -> Result<Vec<Entity>> {
        let path: PathBuf = cx.project.root().join(&self.path);
        let source = cx.project.source(&path)?;
        let text = source.text();
        let declarations = Declared::document(text).map_err(|e| e.named(&path))?;
        roster(declarations, "data", &self.path.display().to_string(), text)
    }
}

/// Entities are written in the config itself: a roster small enough that a file
/// of its own would be ceremony.
impl Source for InlineSource {
    fn load(&self, cx: &SourceCtx<'_>) -> Result<Vec<Entity>> {
        roster(
            self.entities.clone(),
            "inline",
            cx.config.label(),
            &cx.config.source,
        )
    }
}

/// The two KDL rosters' shared tail: what a declaration becomes.
///
/// Both keep the text they were read from, so a fault found while the registry
/// is assembled -- long after either file was closed -- still underlines the
/// node that wrote it.
fn roster(
    entities: Vec<Declared>,
    source: &'static str,
    at: &str,
    text: &str,
) -> Result<Vec<Entity>> {
    let text: Arc<str> = Arc::from(text);
    entities
        .into_iter()
        .map(|entity| {
            let from = Provenance::Roster {
                source,
                at: at.to_owned(),
                text: Arc::clone(&text),
                entity: entity.at.into(),
                fields: entity
                    .spans
                    .into_iter()
                    .map(|(key, at)| (key, at.into()))
                    .collect(),
            };
            Entity::new(&entity.id, entity.fields, from)
        })
        .collect()
}
