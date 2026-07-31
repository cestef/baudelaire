//! Generated social cards: the image a link to a page unfurls into.
//!
//! The card template is compiled to a *paged* document, not an HTML one, and
//! rasterized to PNG. That split matters: `html.elem` does not exist on the
//! paged target and page layout does, so a card template is ordinary Typst,
//! written the way a poster is, and cannot share code with a page layout.
//!
//! A card is rendered only for a page that does not already name its own
//! `image`, so an author who has a real screenshot keeps it.
//!
//! One [`Sidecar`] among however many the flavor registers; everything around
//! the compile lives there.

use std::fmt;
use std::path::PathBuf;

use typst::syntax::RootedPath;

use crate::codegen::{Str, Typst, Value};
use crate::config::Config;
use crate::content::{Data, Iso, Page};
use crate::error::Result;

use super::paged::Laid;
use super::sidecar::{Cx, Sidecar};

/// The social-card sidecar.
pub(in crate::engine) struct Card;

impl Sidecar for Card {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn wanted(&self, config: &Config, page: &Page) -> bool {
        page.wants_card(config)
    }

    fn path(&self, config: &Config, page: &Page) -> PathBuf {
        config.file(&config.generate.cards.url(&page.permalink))
    }

    /// A card is a poster of its own shape, so it takes the page's data flat
    /// and ignores the layout bindings [`Cx`] also carries.
    fn source(&self, cx: &Cx<'_>, page: &Page, rooted: &RootedPath) -> Result<String> {
        Self::module(cx.config, page, rooted)
    }

    fn encode(&self, laid: &Laid, page: &Page) -> Result<Vec<u8>> {
        Self::rasterize(&laid.document, page)
    }
}

impl Card {
    /// The kind's name: the file id of its synthetic module, the label its
    /// compile errors carry, and the noun the summary counts.
    pub(in crate::engine) const NAME: &'static str = "card";

    /// The first page as PNG, at one pixel per point (so the configured size in
    /// pixels is also the page size the template is given in points).
    ///
    /// A template that overflowed onto a second page would silently ship only
    /// its first, so the extra pages are reported rather than dropped.
    fn rasterize(document: &typst_layout::PagedDocument, page: &Page) -> Result<Vec<u8>> {
        let [first] = document.pages() else {
            return Err(
                crate::error::CardError::pages(&page.permalink, document.pages().len()).into(),
            );
        };
        // One pixel per point, so the configured pixel size is also the page
        // size the template was handed, and a card is never resampled.
        let options = typst_render::RenderOptions {
            pixel_per_pt: typst::utils::Scalar::new(1.0),
            render_bleed: false,
        };
        let pixmap = typst_render::render(first, &options);
        pixmap
            .encode_png()
            .map_err(|e| crate::error::CardError::encode(&page.permalink, e).into())
    }

    /// The synthetic module compiled for a card: the page size baudelaire owns,
    /// then the template applied to the page's data.
    ///
    /// The page rule is set *before* the import so a template that wants a
    /// different size can still say so, and after nothing else, so the default
    /// is exactly the configured card.
    fn module(config: &Config, page: &Page, rooted: &RootedPath) -> Result<String> {
        let templates = config
            .paths
            .templates
            .strip_prefix(&config.root)
            .unwrap_or(&config.paths.templates);
        Ok(Template {
            import: format!(
                "/{}/{}",
                templates.display(),
                config.generate.cards.template
            ),
            func: std::path::Path::new(&config.generate.cards.template)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("card")
                .to_owned(),
            width: config.generate.cards.width,
            height: config.generate.cards.height,
            data: Typst(&Self::data(config, page)).to_string(),
            frontmatter: matches!(page.data, Data::Export)
                .then(|| format!("/{}", rooted.vpath().get_without_slash())),
        }
        .to_string())
    }

    /// What the template is handed. Deliberately flat and small: a card shows a
    /// title, maybe a date and a site name, and nothing a card can render is
    /// worth invalidating every card over.
    fn data(config: &Config, page: &Page) -> Value {
        Value::dict([
            ("title", Value::str(page.title())),
            ("url", Value::str(&page.permalink)),
            ("lang", Value::str(&page.lang)),
            ("collection", Value::str(&page.collection)),
            ("site", Value::str(config.title(&page.lang))),
            ("author", Value::opt(config.author(&page.lang))),
            (
                "date",
                Value::opt(page.frontmatter.date.map(|d| Iso(d).to_string())),
            ),
            ("taxonomies", page.taxonomies()),
        ])
    }
}

/// The generated module, rendered through [`fmt::Display`] like every other
/// piece of Typst this build writes.
struct Template {
    import: String,
    func: String,
    width: u32,
    height: u32,
    /// The data dict, already Typst source.
    data: String,
    /// The page module to import `frontmatter` from, when it exports one.
    frontmatter: Option<String>,
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "#set page(width: {}pt, height: {}pt, margin: 0pt)",
            self.width, self.height
        )?;
        writeln!(f, "#import {}: {} as __card", Str(&self.import), self.func)?;
        let extra = match &self.frontmatter {
            Some(page) => {
                writeln!(f, "#import {}: frontmatter as __data", Str(page))?;
                "__data"
            }
            None => "(:)",
        };
        write!(f, "#__card({} + (frontmatter: {extra}))", self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_sets_the_page_size_and_applies_the_template() {
        let out = Template {
            import: "/templates/card.typ".into(),
            func: "card".into(),
            width: 1200,
            height: 630,
            data: "(title: \"A\")".into(),
            frontmatter: Some("/content/a.typ".into()),
        }
        .to_string();
        assert_eq!(
            out,
            "#set page(width: 1200pt, height: 630pt, margin: 0pt)\n\
             #import \"/templates/card.typ\": card as __card\n\
             #import \"/content/a.typ\": frontmatter as __data\n\
             #__card((title: \"A\") + (frontmatter: __data))"
        );
    }

    /// A page with no `frontmatter` export still renders, with an empty dict
    /// rather than a compile error about a missing import.
    #[test]
    fn a_page_without_frontmatter_gets_an_empty_dict() {
        let out = Template {
            import: "/templates/card.typ".into(),
            func: "card".into(),
            width: 800,
            height: 400,
            data: "(:)".into(),
            frontmatter: None,
        }
        .to_string();
        assert!(out.ends_with("#__card((:) + (frontmatter: (:)))"), "{out}");
        assert!(!out.contains("frontmatter as __data"), "{out}");
    }
}
