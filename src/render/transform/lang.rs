//! Sets the document language on `<html>`.

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
use crate::render::transform::{Cx, ElementExt, Exempt, Transform};

/// Stamps `<html lang="..">` (and `dir="rtl"` for a right-to-left language)
/// from the page's language, correcting the fixed `lang="en"` typst emits.
pub(super) struct Lang;

impl Lang {
    /// How [`Transform::after`] names this pass.
    pub(super) const NAME: &'static str = "lang";
}

impl Transform for Lang {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn after(&self) -> &'static [&'static str] {
        &[Exempt::NAME]
    }

    fn enabled(&self, _config: &Config) -> bool {
        true
    }

    fn apply(&self, doc: &mut HtmlDocument, cx: &mut Cx<'_>) {
        let root = doc.root_mut();
        root.set(attr::lang, &cx.page.lang);
        if let Some(dir) = cx.config.dir(&cx.page.lang) {
            root.set(attr::dir, dir);
        }
    }
}
