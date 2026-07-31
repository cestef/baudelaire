//! Many pages as one document: a collection bound end to end, or the whole
//! site, exported as a single PDF.
//!
//! The paged sibling of the single-file HTML export. Where a
//! [`sidecar`](super::sidecar) is one page compiled twice, a bundle is *every*
//! page compiled once more together: one module `#include`s them in the site's
//! own order and hands the lot to a template, so page breaks, running heads, a
//! contents list and continuous numbering are the template's to decide.
//!
//! It is not a sidecar for that reason. A sidecar belongs to a page and is
//! cached with it; a bundle belongs to no page and has to rebuild when *any* of
//! its pages moves, which is a cache entry of its own ([`crate::graph::Cache`]).

use std::fmt::{self, Write as _};
use std::path::PathBuf;

use crate::codegen::{Str, Typst, Value};
use crate::config::Config;
use crate::content::{Data, Page};
use crate::error::Result;
use crate::graph::Deps;
use crate::world::Project;

use super::paged::Paged;
use super::prepare::Prepare;

/// What one bundle binds: the pages, in order, and where the result goes.
///
/// Resolved from the config and the planned page set before anything compiles,
/// so the prune, the cache and the exporter all read one list.
pub(in crate::engine) struct Bundle<'a> {
    /// The bundle's id, as the summary and the cache name it: the collection,
    /// or `site`, suffixed with the language on a multilingual site.
    id: String,
    /// The document's title, handed to the template.
    title: String,
    lang: &'a str,
    /// Root-relative URL of the file, e.g. `/guide.pdf`.
    url: String,
    pages: Vec<&'a Page>,
}

impl<'a> Bundle<'a> {
    /// Every bundle this config asks for, over `pages`.
    ///
    /// One per named collection and one for the site, each per built language:
    /// a French manual is a French document, and binding both languages into
    /// one file would interleave them.
    pub(in crate::engine) fn planned(config: &'a Config, pages: &'a [Page]) -> Vec<Self> {
        let cfg = &config.generate.pdf.bundle;
        if !cfg.active() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for lang in config.langs() {
            for collection in &cfg.collections {
                let bound = Self::bind(pages, lang, |page| &page.collection == collection);
                if bound.is_empty() {
                    continue;
                }
                out.push(Self {
                    id: Self::named(collection, lang, config),
                    // The collection's own id. There is no configured title for
                    // a collection, and inventing one here would be a second
                    // spelling of a name the site already has.
                    title: collection.clone(),
                    lang,
                    url: Self::url(config, lang, collection),
                    pages: bound,
                });
            }
            if cfg.site {
                let bound = Self::bind(pages, lang, |_| true);
                if bound.is_empty() {
                    continue;
                }
                out.push(Self {
                    id: Self::named(Self::SITE, lang, config),
                    title: config.title(lang).to_owned(),
                    lang,
                    url: Self::url(config, lang, Self::SITE),
                    pages: bound,
                });
            }
        }
        out
    }

    /// The whole site's bundle, under the name it is written as.
    const SITE: &'static str = "site";

    /// Where a bundle is served: `/<target>.pdf`, localized like every other
    /// per-language artifact. One rule for both kinds of target, so a reader
    /// who knows where `/guide.pdf` came from knows where `/site.pdf` did.
    fn url(config: &Config, lang: &str, target: &str) -> String {
        format!("/{}.pdf", config.scope(lang, target))
    }

    /// The pages one bundle binds, in the order [`crate::content::plan`] put
    /// them, which is each collection's own sort order.
    ///
    /// Generated listings are excluded, as they are from every other paged
    /// artifact: a tag index inside a manual is a page of links to a document
    /// the reader is already holding.
    fn bind(pages: &'a [Page], lang: &str, mut want: impl FnMut(&Page) -> bool) -> Vec<&'a Page> {
        pages
            .iter()
            .filter(|page| page.lang == lang)
            .filter(|page| !matches!(page.data, Data::Generated(_)))
            .filter(|page| want(page))
            .collect()
    }

    /// A bundle's id: its target, plus the language on a site that builds more
    /// than one, so two editions never claim one cache entry.
    fn named(target: &str, lang: &str, config: &Config) -> String {
        match config.langs().len() > 1 {
            true => format!("{target}.{lang}"),
            false => target.to_owned(),
        }
    }

    /// What this kind of artifact is called: its module's file id, the label
    /// its compile errors carry, and the noun the summary counts.
    pub(in crate::engine) const KIND: &'static str = "bundle";

    pub(in crate::engine) fn id(&self) -> &str {
        &self.id
    }

    /// Where the file lands under `dist`. Read by the exporter and by the
    /// prune, so a bundle an earlier build wrote is kept rather than swept.
    pub(in crate::engine) fn path(&self, config: &Config) -> PathBuf {
        config.file(&self.url)
    }

    /// The synthetic module: the template, one frontmatter import per page, and
    /// every page's body included in order.
    ///
    /// Its text is the bundle's cache fingerprint, exactly as a page's wrapper
    /// is that page's, so adding, removing or reordering a page invalidates it
    /// even though no file any of them names has changed.
    pub(in crate::engine) fn source(
        &self,
        prepare: &Prepare<'_>,
        project: &Project,
    ) -> Result<String> {
        let cfg = &prepare.config().generate.pdf.bundle;
        let mut entries = Vec::with_capacity(self.pages.len());
        let mut imports = String::new();
        for (i, page) in self.pages.iter().enumerate() {
            let vpath = format!(
                "/{}",
                project
                    .virtualize(&page.source)?
                    .vpath()
                    .get_without_slash()
            );
            // A page with no `frontmatter` export has nothing to import, and an
            // empty dict is what its own compile passes too.
            let frontmatter = match page.data {
                Data::Export => {
                    let alias = format!("__fm{i}");
                    writeln!(imports, "#import {}: frontmatter as {alias}", Str(&vpath))
                        .expect("writing to a String cannot fail");
                    alias
                }
                _ => "(:)".to_owned(),
            };
            entries.push(format!(
                "(page: {}, body: include {})",
                prepare.dict(page, &frontmatter),
                Str(&vpath)
            ));
        }
        Ok(Module {
            import: format!("{}/{}", prepare.template_root(&cfg.template), cfg.template),
            func: std::path::Path::new(&cfg.template)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("book")
                .to_owned(),
            imports,
            meta: Typst(&self.meta(prepare.config())).to_string(),
            entries,
        }
        .to_string())
    }

    /// What the template is told about the document itself, as opposed to about
    /// any one page.
    fn meta(&self, config: &Config) -> Value {
        Value::dict([
            ("id", Value::str(&self.id)),
            ("title", Value::str(&self.title)),
            ("lang", Value::str(self.lang)),
            ("url", Value::str(&self.url)),
            ("site", Value::str(config.title(self.lang))),
            ("author", Value::opt(config.author(self.lang))),
            ("pages", Value::Int(self.pages.len() as i64)),
        ])
    }

    /// Lay the bundle out and export it, reporting what the compile read: every
    /// page it bound, the template, and everything either imports.
    ///
    /// The same runner every paged artifact uses, so the export options that
    /// keep the bytes stable are pinned in one place rather than per caller.
    pub(in crate::engine) fn export(
        &self,
        project: &Project,
        _prepare: &Prepare<'_>,
        text: String,
    ) -> Result<(Vec<u8>, Deps)> {
        let laid = Paged {
            name: self.id.clone(),
            kind: Self::KIND,
            text,
        }
        .run(project)?;
        Ok((laid.pdf(Self::KIND, &self.url)?, laid.deps))
    }
}

/// The generated module, rendered through [`fmt::Display`] like every other
/// piece of Typst this build writes.
struct Module {
    import: String,
    func: String,
    /// The per-page `#import .. : frontmatter as __fmN` lines, already written.
    imports: String,
    /// The document dict, already Typst source.
    meta: String,
    /// One `(page: .., body: include ..)` literal per bound page, in order.
    entries: Vec<String>,
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "#import {}: {} as __bundle",
            Str(&self.import),
            self.func
        )?;
        f.write_str(&self.imports)?;
        write!(f, "#__bundle({}, (", self.meta)?;
        for entry in &self.entries {
            write!(f, "{entry}, ")?;
        }
        write!(f, "))")
    }
}

#[cfg(test)]
mod tests {
    use super::Module;

    #[test]
    fn the_module_applies_the_template_to_every_entry() {
        let out = Module {
            import: "/templates/book.typ".into(),
            func: "book".into(),
            imports: "#import \"/content/a.typ\": frontmatter as __fm0\n".into(),
            meta: "(title: \"Guide\")".into(),
            entries: vec!["(page: (frontmatter: __fm0), body: include \"/content/a.typ\")".into()],
        }
        .to_string();
        assert_eq!(
            out,
            "#import \"/templates/book.typ\": book as __bundle\n\
             #import \"/content/a.typ\": frontmatter as __fm0\n\
             #__bundle((title: \"Guide\"), ((page: (frontmatter: __fm0), body: include \"/content/a.typ\"), ))"
        );
    }

    /// A bundle with no page still has to produce compilable source: the
    /// template decides what an empty document says.
    #[test]
    fn an_empty_bundle_is_still_a_call() {
        let out = Module {
            import: "/templates/book.typ".into(),
            func: "book".into(),
            imports: String::new(),
            meta: "(:)".into(),
            entries: Vec::new(),
        }
        .to_string();
        assert!(out.ends_with("#__bundle((:), ())"), "{out}");
    }
}
