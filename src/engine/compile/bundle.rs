//! Many pages as one document: what a bundle binds, and the paged compile that
//! typesets it.
//!
//! What is bound is [`Selection`](crate::content::Selection)'s answer, and what
//! it is written as is the format's: the PDF is here because it is a *compile*,
//! a second pass over the pages the site already has, and the EPUB is an
//! [`emit`](crate::engine::emit) processor because it is a container built from
//! pages that have already rendered.
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

use crate::codegen::{Import, Str, Typst, Value};
use crate::config::{BundleConfig, BundleFormat, Config};
use crate::content::{Data, Page, Selection};
use crate::error::Result;
use crate::graph::Deps;
use crate::world::Project;

use super::paged::Paged;
use super::prepare::Prepare;
use crate::content::Frontmatter;

/// One bundle, in one format: the pages it binds and where the file goes.
///
/// The binding itself is [`Selection`]'s, which is the whole point of it: two
/// formats of one bundle are the same pages under the same title, differing
/// only in what is written. This adds the format and what follows from it --
/// the URL, the cache id, the template -- and nothing else.
pub(in crate::engine) struct Bundle<'a> {
    /// The bundle's id as the config names it, which is the filename stem every
    /// format is written under.
    key: &'a str,
    /// What is written.
    format: BundleFormat,
    /// The bundle's config, for the template and whatever else a format reads.
    cfg: &'a BundleConfig,
    /// The pages, in order, under their title.
    selection: Selection<'a>,
    /// Root-relative URL of the file, e.g. `/guide.pdf`.
    url: String,
}

impl<'a> Bundle<'a> {
    /// Every bundle this config asks for, over `pages`: one per named bundle,
    /// per format it names, per built language.
    ///
    /// A format the binary cannot write is dropped here, having been reported
    /// once by the feature gate: the alternative is a file nothing writes and a
    /// prune that deletes last build's.
    pub(in crate::engine) fn planned(config: &'a Config, pages: &'a [Page]) -> Vec<Self> {
        let mut out = Vec::new();
        for (key, cfg) in &config.generate.bundles {
            for selection in Selection::planned(key, cfg, config, pages) {
                for format in cfg.active() {
                    out.push(Self {
                        key,
                        format,
                        cfg,
                        url: selection.url(config, format.ext()),
                        // One selection per format: the two are the same pages,
                        // and holding one between them would tie every format's
                        // lifetime to the others for a struct of borrows.
                        selection: Selection {
                            id: selection.id.clone(),
                            title: selection.title.clone(),
                            lang: selection.lang,
                            pages: selection.pages.clone(),
                        },
                    });
                }
            }
        }
        out
    }

    /// The cache id: the bundle, its language, and its format. Two formats of
    /// one selection are two files and two entries, since only one of them
    /// changes when a template does.
    pub(in crate::engine) fn id(&self) -> String {
        format!("{}.{}", self.selection.id, self.format.ext())
    }

    /// What this bundle is called in the summary: the file, which is the thing
    /// the reader is waiting for.
    pub(in crate::engine) fn label(&self) -> String {
        self.url.trim_start_matches('/').to_owned()
    }

    /// What this kind of artifact is called: its module's file id, the label
    /// its compile errors carry, and the noun the summary counts.
    pub(in crate::engine) const KIND: &'static str = "bundle";

    /// Whether this one is written by the paged compile. The other formats are
    /// built from the rendered pages instead, so the compile pass has to know
    /// which of the planned bundles are its.
    pub(in crate::engine) fn typeset(&self) -> bool {
        self.format == BundleFormat::Pdf
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
        let cfg = self.cfg;
        let mut entries = Vec::with_capacity(self.selection.pages.len());
        let mut imports = String::new();
        for (i, page) in self.selection.pages.iter().enumerate() {
            let vpath = format!(
                "/{}",
                project
                    .virtualize(&page.source)?
                    .vpath()
                    .get_without_slash()
            );
            // Where this page's frontmatter and body come from, exactly as
            // `Prepare::bound` decides it for the page's own compile: the two
            // must agree, or a page reads one way on screen and another on
            // paper. A page with no `frontmatter` export has nothing to import,
            // and an empty dict is what its own compile passes too.
            let (frontmatter, body) = match &page.data {
                Data::Export => {
                    let alias = format!("__fm{i}");
                    writeln!(
                        imports,
                        "{}",
                        Import::new(&vpath, Frontmatter::EXPORT, &alias)
                    )
                    .expect("writing to a String cannot fail");
                    (alias, format!("include {}", Str(&vpath)))
                }
                // A markdown page has a file, but not one the compiler could
                // `#include`: its frontmatter is the dict it lowered to and its
                // body is the Typst it lowered to, inlined as a content block.
                // Included instead, typst was handed raw markdown, and the
                // document either failed on the first heading or bound the page
                // verbatim -- fences, `#` and all, its title gone.
                #[cfg(feature = "markdown")]
                Data::Lowered { dict, .. } => (dict.clone(), format!("[{}]", page.body)),
                _ => ("(:)".to_owned(), format!("include {}", Str(&vpath))),
            };
            entries.push(format!(
                "(page: {}, body: {body})",
                prepare.dict(page, &frontmatter)
            ));
        }
        Ok(Module {
            import: format!("{}/{}", prepare.dir(&cfg.template), cfg.template),
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
            ("id", Value::str(self.key)),
            ("title", Value::str(&self.selection.title)),
            ("lang", Value::str(self.selection.lang)),
            ("url", Value::str(&self.url)),
            ("site", Value::str(config.title(self.selection.lang))),
            ("author", Value::opt(config.author(self.selection.lang))),
            (
                "pages",
                Value::Int(i64::try_from(self.selection.pages.len()).unwrap_or(i64::MAX)),
            ),
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
            name: self.id(),
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
        writeln!(f, "{}", Import::new(&self.import, &self.func, "__bundle"))?;
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
