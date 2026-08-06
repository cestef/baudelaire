//! The site's section tree: content directories nested as they appear under
//! `content/`, each carrying the pages filed directly in it and its child
//! directories. Built once from the planned pages and shared by both consumers
//! of a site nav (the template `page.sections` value and the
//! `baudelaire:sections` JS module) so the two can never disagree.

use crate::codegen::Value;
use crate::config::Config;
use crate::content::Page;

/// A page reduced to what a nav needs: where it lives and what to call it.
pub struct Link {
    pub url: String,
    pub title: String,
}

/// One content directory: its own name, the pages directly under it (in sort
/// order), and its child directories (in first-seen order).
pub struct Section {
    pub id: String,
    pub pages: Vec<Link>,
    pub children: Vec<Self>,
}

impl Section {
    /// The forest of top-level sections built from the authored pages of one
    /// language; generated listings (taxonomy and paginated indexes) and pages
    /// a nav never links ([`Page::listed`]) are excluded. Each page is filed
    /// under its [`Page::section_path`], so a language's nav lists only its own
    /// pages.
    pub fn tree(pages: &[Page], config: &Config, lang: &str) -> Vec<Self> {
        let mut root = Self::new("");
        for page in pages {
            if !page.authored() || page.lang != lang || !page.listed(config) {
                continue;
            }
            let mut node = &mut root;
            for segment in page.section_path(config) {
                node = node.child(&segment);
            }
            node.pages.push(Link {
                url: page.permalink.clone(),
                title: page.title().to_owned(),
            });
        }
        root.children
    }

    fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            pages: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The direct child directory named `id`, created (in first-seen order) if
    /// it does not exist yet.
    fn child(&mut self, id: &str) -> &mut Self {
        if let Some(i) = self.children.iter().position(|c| c.id == id) {
            return &mut self.children[i];
        }
        self.children.push(Self::new(id));
        self.children.last_mut().expect("just pushed")
    }

    /// This section as a [`Value`]: `(id, pages: ((url, title), ..),
    /// children: (..))`. One representation, rendered to Typst for
    /// `page.sections` and to JavaScript for the `baudelaire:sections` module.
    pub fn value(&self) -> Value {
        Value::dict([
            ("id", Value::str(&self.id)),
            (
                "pages",
                Value::array(self.pages.iter().map(|p| {
                    Value::dict([("url", Value::str(&p.url)), ("title", Value::str(&p.title))])
                })),
            ),
            (
                "children",
                Value::array(self.children.iter().map(Self::value)),
            ),
        ])
    }
}
