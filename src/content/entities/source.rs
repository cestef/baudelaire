//! Where a registry's entities are read from: one loader per [`SourceConfig`]
//! variant, and the dispatcher that pairs them.

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
type Builtin = (&'static str, fn(&Frontmatter) -> Option<String>);

const BUILTIN: &[Builtin] = &[
    ("title", |fm| fm.title.clone()),
    ("description", |fm| fm.blurb().map(str::to_owned)),
    ("image", |fm| fm.image.clone()),
];

/// Entities are the content pages under a directory, their frontmatter the
/// fields and their body the prose. The default language's edition sorts
/// first, so its fields are the ones a merge keeps.
impl Source for PagesSource {
    fn load(&self, cx: &SourceCtx<'_>) -> Result<Vec<Entity>> {
        let dir = crate::fs::resolved(cx.project.root().join(&self.dir));
        let mut profiles: Vec<&Page> = cx
            .pages
            .iter()
            .filter(|page| page.authored())
            .filter(|page| crate::fs::resolved(&page.source).starts_with(&dir))
            .collect();
        profiles.sort_by_key(|page| (page.lang != cx.config.lang, page.source.clone()));
        let mut out = Vec::new();
        for page in profiles {
            let fm = &page.frontmatter;
            let mut fields: Vec<(String, Value)> = BUILTIN
                .iter()
                .filter_map(|(key, read)| Some(((*key).to_owned(), Value::Str(read(fm)?))))
                .collect();
            for (key, value) in &fm.extra {
                fields.push((key.clone(), value.clone()));
            }
            out.push(Entity::authored(
                page.id.slug(),
                fields,
                Provenance::Page {
                    path: page.source.clone(),
                },
                &page.lang,
                page.source.clone(),
            )?);
        }
        Ok(out)
    }
}

/// Entities are the nodes of a KDL roster file, read through
/// [`Project::source`]. The dev server does not watch it on its own: a site
/// editing its roster under `serve` names it in `serve { include }`.
impl Source for DataSource {
    fn load(&self, cx: &SourceCtx<'_>) -> Result<Vec<Entity>> {
        let path: PathBuf = cx.project.root().join(&self.path);
        let source = cx.project.source(&path)?;
        let text = source.text();
        let declarations = Declared::document(text).map_err(|e| e.named(&path))?;
        Provenance::entities(
            declarations,
            SourceConfig::DATA,
            &self.path.display().to_string(),
            text,
        )
    }
}

/// Entities are written in the config itself: a roster small enough that a file
/// of its own would be ceremony.
impl Source for InlineSource {
    fn load(&self, cx: &SourceCtx<'_>) -> Result<Vec<Entity>> {
        Provenance::entities(
            self.entities.clone(),
            SourceConfig::INLINE,
            Config::FILE,
            &cx.config.source,
        )
    }
}

impl Provenance {
    /// What a KDL roster's declarations become, for the two sources that read
    /// one; every entity keeps the text it was read from, so a fault found long
    /// after the file was closed still underlines the node that wrote it.
    pub(super) fn entities(
        declared: Vec<Declared>,
        source: &'static str,
        at: &str,
        text: &str,
    ) -> Result<Vec<Entity>> {
        let text: Arc<str> = Arc::from(text);
        declared
            .into_iter()
            .map(|entity| {
                let from = Self::Roster {
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
}
