//! A PDF of every page, laid out beside its HTML.
//!
//! The same source, the other target: where the page's own compile builds a
//! DOM, this one lays the document out on paper and exports it. It is the thing
//! a Typst-native generator can do that a Markdown one cannot, so it is
//! deliberately the *page*, template and all, and not a stripped print view.
//!
//! The template is paged, so it is separate from the layout for the reason
//! written in [`super::card`]: `html.elem` draws nothing on this target. It is
//! handed the same `page` dict the layout gets, though, built once by
//! [`Prepare::bind`], so `page.strings` and `page.date` mean the same thing on
//! paper as on screen.

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
        config.file(&config.generate.pdf.pages.url(&page.permalink))
    }

    /// The page bound to the paged template, exactly as it is bound to its
    /// layout for the HTML compile.
    fn source(&self, cx: &Cx<'_>, page: &Page, rooted: &RootedPath) -> Result<String> {
        let template = &cx.config.generate.pdf.pages.template;
        Ok(cx
            .prepare
            .bind(page, rooted, &cx.prepare.template_root(template), template))
    }

    /// Identified by the page's permalink: stable across builds, and unique on
    /// a site where two pages may share a title.
    fn encode(&self, laid: &Laid, page: &Page) -> Result<Vec<u8>> {
        laid.pdf(Self::NAME, &page.permalink)
    }
}

impl Pdf {
    /// The kind's name: the file id of its synthetic module, the label its
    /// export errors carry, and the noun the summary counts.
    pub(in crate::engine) const NAME: &'static str = "pdf";
}
