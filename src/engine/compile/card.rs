//! Generated social cards: the image a link to a page unfurls into.
//!
//! The template is compiled to a *paged* document and rasterized to PNG, so it
//! is ordinary Typst and cannot share code with a page layout, where
//! `html.elem` is what draws. A card is rendered only for a page that does not
//! already name its own `image`.

use std::fmt;
use std::path::PathBuf;

use typst::syntax::RootedPath;

use crate::codegen::{Import, Typst, Value};
use crate::config::Config;
use crate::content::entities::{Credit, Vocabulary};
use crate::content::{Data, Iso, Page};
use crate::error::Result;

use super::paged::Laid;
use super::sidecar::{Cx, Sidecar};
use crate::content::Frontmatter;

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

    /// A card takes the page's data flat, ignoring the layout bindings [`Cx`]
    /// also carries.
    fn source(&self, cx: &Cx<'_>, page: &Page, rooted: &RootedPath) -> Result<String> {
        let template = &cx.config.generate.cards.template;
        Ok(Self::module(cx, page, rooted, &cx.prepare.dir(template)))
    }

    fn encode(&self, laid: &Laid, page: &Page) -> Result<Vec<u8>> {
        Self::rasterize(&laid.document, page)
    }
}

impl Card {
    /// The file id of the synthetic module, the label its compile errors carry,
    /// and the noun the summary counts.
    pub(in crate::engine) const NAME: &'static str = "card";

    /// The first page as PNG, at one pixel per point, so the configured size in
    /// pixels is also the page size the template is given in points and a card
    /// is never resampled. A template that overflowed onto a second page is an
    /// error rather than a silently truncated card.
    fn rasterize(document: &typst_layout::PagedDocument, page: &Page) -> Result<Vec<u8>> {
        let [first] = document.pages() else {
            return Err(
                crate::error::CardError::pages(&page.permalink, document.pages().len()).into(),
            );
        };
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
    /// then the template applied to the page's data. The page rule is set
    /// *before* the import so a template that wants a different size can still
    /// say so. `dir` is the import root the template is loaded from, resolved
    /// by [`Prepare::dir`](crate::engine::compile::prepare) so a theme's
    /// `card.typ` is found too.
    fn module(cx: &Cx<'_>, page: &Page, rooted: &RootedPath, dir: &str) -> String {
        let config = cx.config;
        Template {
            import: format!("{dir}/{}", config.generate.cards.template),
            func: std::path::Path::new(&config.generate.cards.template)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("card")
                .to_owned(),
            width: config.generate.cards.width,
            height: config.generate.cards.height,
            data: Typst(&Self::data(cx, page)).to_string(),
            frontmatter: matches!(page.data, Data::Export)
                .then(|| format!("/{}", rooted.vpath().get_without_slash())),
        }
        .to_string()
    }

    /// What the template is handed, kept flat and small so little invalidates
    /// every card.
    fn data(cx: &Cx<'_>, page: &Page) -> Value {
        let config = cx.config;
        Value::dict([
            ("title", Value::str(page.title())),
            ("url", Value::str(&page.permalink)),
            ("lang", Value::str(&page.lang)),
            ("collection", Value::str(&page.collection)),
            ("site", Value::str(config.title(&page.lang))),
            (
                Credit::Author
                    .spelling(Vocabulary::Document)
                    .unwrap_or("author"),
                Value::opt(cx.prepare.byline(page).line(Credit::Author)),
            ),
            (
                "date",
                Value::opt(page.frontmatter.date.map(|d| Iso(d).to_string())),
            ),
            ("taxonomies", page.taxonomies()),
        ])
    }
}

/// The generated module.
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
        writeln!(f, "{}", Import::new(&self.import, &self.func, "__card"))?;
        let extra = match &self.frontmatter {
            Some(page) => {
                writeln!(
                    f,
                    "{}",
                    Import::new(page, Frontmatter::EXPORT, Frontmatter::ALIAS)
                )?;
                Frontmatter::ALIAS
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
