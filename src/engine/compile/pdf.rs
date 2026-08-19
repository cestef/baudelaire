//! A PDF of every page, laid out beside its HTML.
//!
//! The template is paged and so separate from the layout, but is handed the
//! same `page` dict, built once by `Prepare::bind`.

use std::path::PathBuf;

use typst::syntax::RootedPath;

use crate::config::Config;
use crate::content::Page;
use crate::error::Result;

use super::paged::Laid;
use super::sidecar::{Cx, Sidecar};

/// The per-page PDF sidecar.
pub(in crate::engine) struct Pdf;

impl Sidecar for Pdf {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn wanted(&self, config: &Config, page: &Page) -> bool {
        page.wants_pdf(config)
    }

    fn path(&self, config: &Config, page: &Page) -> PathBuf {
        config.file(&config.artifacts.pdf.pages.url(&page.permalink))
    }

    /// The page bound to the paged template, exactly as it is bound to its
    /// layout for the HTML compile.
    fn source(&self, cx: &Cx<'_>, page: &Page, rooted: &RootedPath) -> Result<String> {
        let template = &cx.config.artifacts.pdf.pages.template;
        Ok(cx
            .prepare
            .bind(page, rooted, &cx.prepare.dir(template), template))
    }

    /// Identified by the page's permalink, which is stable across builds and
    /// unique where a title is not.
    fn encode(&self, laid: &Laid, page: &Page) -> Result<Vec<u8>> {
        laid.pdf(Self::NAME, &page.permalink)
    }
}

impl Pdf {
    /// The file id of the synthetic module, the label its export errors carry,
    /// and the noun the summary counts.
    pub(in crate::engine) const NAME: &'static str = "pdf";
}
