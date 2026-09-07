//! Many pages as one document: what a bundle binds, and the paged compile that
//! typesets it.
//!
//! A bundle belongs to no page, so it has a cache entry of its own and rebuilds
//! when any of its pages moves.

use std::path::PathBuf;

use crate::config::{BundleFormat, Config};
use crate::content::{Page, Selection};

#[cfg(feature = "pdf")]
use {
    super::paged::Paged,
    super::prepare::Prepare,
    crate::codegen::{Import, Str, Typst, Value},
    crate::config::BundleConfig,
    crate::content::{Data, Frontmatter},
    crate::error::Result,
    crate::graph::Deps,
    crate::world::Project,
    std::fmt::{self, Write as _},
};

/// One bundle, in one format: the pages it binds and where the file goes.
pub(in crate::engine) struct Bundle<'a> {
    /// The config's name for the bundle, and the filename stem every format is
    /// written under.
    #[cfg(feature = "pdf")]
    key: &'a str,
    format: BundleFormat,
    #[cfg(feature = "pdf")]
    cfg: &'a BundleConfig,
    /// The pages, in order, under their title.
    selection: Selection<'a>,
    /// Root-relative, e.g. `/guide.pdf`.
    url: String,
}

impl<'a> Bundle<'a> {
    /// Every bundle this config asks for, over `pages`: one per named bundle,
    /// per format it names, per built language. A format the binary cannot
    /// write is dropped here, having been reported once by the feature gate.
    pub(in crate::engine) fn planned(config: &'a Config, pages: &'a [Page]) -> Vec<Self> {
        let mut out = Vec::new();
        for (key, cfg) in &config.artifacts.bundles {
            for selection in Selection::planned(key, cfg, config, pages) {
                for format in cfg.active() {
                    out.push(Self {
                        #[cfg(feature = "pdf")]
                        key,
                        format,
                        #[cfg(feature = "pdf")]
                        cfg,
                        url: selection.url(config, format.ext()),
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

    /// The cache id: the bundle, its language, and its format, since two
    /// formats of one selection are two files and two entries.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn id(&self) -> String {
        format!("{}.{}", self.selection.id, self.format.ext())
    }

    /// What this bundle is called in the summary.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn label(&self) -> String {
        self.url.trim_start_matches('/').to_owned()
    }

    /// The module's file id, the label its compile errors carry, and the noun
    /// the summary counts.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) const KIND: &'static str = "bundle";

    /// Whether this one is written by the paged compile; the other formats are
    /// built from the rendered pages instead.
    #[cfg(feature = "pdf")]
    pub(in crate::engine) fn typeset(&self) -> bool {
        self.format == BundleFormat::Pdf
    }

    /// Where the file lands under `dist`. Read by the prune too, so a bundle an
    /// earlier build wrote is kept rather than swept.
    pub(in crate::engine) fn path(&self, config: &Config) -> PathBuf {
        config.file(&self.url)
    }

    /// Every file the bundles this site asks for occupy, whatever writes them.
    ///
    /// Read from the config and the page set rather than from what a build
    /// produced, because neither producer writes on every build: a typeset
    /// bundle can be reused from the cache, and an emitted one can be claimed
    /// by a static file. Either way the sweep must keep it.
    pub(in crate::engine) fn claimed(config: &'a Config, pages: &'a [Page]) -> Vec<PathBuf> {
        Self::planned(config, pages)
            .iter()
            .map(|bundle| bundle.path(config))
            .collect()
    }

    pub(in crate::engine) fn format(&self) -> BundleFormat {
        self.format
    }

    pub(in crate::engine) fn selection(&self) -> &Selection<'a> {
        &self.selection
    }

    /// The synthetic module: the template, one frontmatter import per page, and
    /// every page's body included in order. Its text is the bundle's cache
    /// fingerprint, so reordering pages invalidates it even though no file any
    /// of them names has changed. Where a page's frontmatter and body come from
    /// must match what `Prepare::bound` decides for that page's own compile, or
    /// it reads one way on screen and another on paper. A markdown page's body
    /// is spliced in as markup, its file being nothing typst could `#include`.
    #[cfg(feature = "pdf")]
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
                #[cfg(feature = "markdown")]
                Data::Lowered { dict, .. } => (dict.clone(), format!("[{}]", page.body)),
                _ => ("(:)".to_owned(), format!("include {}", Str(&vpath))),
            };
            entries.push(format!(
                "(page: {}, body: {body})",
                prepare.dict(page, &frontmatter)?
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
    #[cfg(feature = "pdf")]
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
    #[cfg(feature = "pdf")]
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

/// The generated module.
#[cfg(feature = "pdf")]
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

#[cfg(feature = "pdf")]
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

#[cfg(all(test, feature = "pdf"))]
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
