//! Sets the document language on `<html>`.

use typst_html::{HtmlDocument, attr};

use crate::config::Config;
use crate::render::transform::{Cx, ElementExt, Transform};

/// Stamps `<html lang="..">` (and `dir="rtl"` for a right-to-left language) from
/// the page's language. typst emits a fixed `lang="en"`, so this corrects it for
/// every non-English default and every translation. Always runs.
pub(super) struct Lang;

impl Transform for Lang {
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
