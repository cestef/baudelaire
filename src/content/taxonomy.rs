//! Generation of taxonomy index pages.
//!
//! For each configured taxonomy with `index=true`, this builds a [`Listing`]
//! per term (plus one listing every term), which lower to synthetic pages in
//! the normal compile pipeline.

use std::collections::BTreeMap;

use crate::config::{Config, TaxonomyConfig};
use crate::content::listing::{Item, Listing};
use crate::content::Page;

/// Builds the taxonomy index pages for a site.
pub struct Taxonomy;

impl Taxonomy {
    /// Generate index + term pages for every configured taxonomy that requests
    /// an index, drawing terms from `pages`' frontmatter.
    pub fn pages(config: &Config, pages: &[Page]) -> Vec<Page> {
        let mut out = Vec::new();
        for (name, cfg) in &config.taxonomies {
            if cfg.index {
                Group::new(name, cfg, pages).build(config, &mut out);
            }
        }
        out
    }
}

/// One taxonomy's terms and the pages under each.
struct Group<'a> {
    /// Taxonomy name, e.g. `tags` — also its URL prefix and section id.
    name: &'a str,
    /// Optional user template for the generated pages.
    template: Option<String>,
    /// term → member pages, sorted by term then title.
    terms: BTreeMap<String, Vec<&'a Page>>,
}

impl<'a> Group<'a> {
    fn new(name: &'a str, cfg: &TaxonomyConfig, pages: &'a [Page]) -> Self {
        let mut terms: BTreeMap<String, Vec<&Page>> = BTreeMap::new();
        for page in pages {
            if let Some(values) = page.frontmatter.taxonomies.get(&cfg.key) {
                for term in values {
                    terms.entry(term.clone()).or_default().push(page);
                }
            }
        }
        for members in terms.values_mut() {
            members.sort_by(|a, b| a.frontmatter.title.cmp(&b.frontmatter.title));
        }
        Self {
            name,
            template: cfg.template.clone(),
            terms,
        }
    }

    /// Emit the index listing and one listing per term.
    fn build(&self, config: &Config, out: &mut Vec<Page>) {
        if self.terms.is_empty() {
            return;
        }
        out.push(self.index().into_page(config));
        for term in self.terms.keys() {
            out.push(self.term(term).into_page(config));
        }
    }

    /// The `/{name}/` listing of every term with its member count.
    fn index(&self) -> Listing {
        let items = self
            .terms
            .iter()
            .map(|(term, members)| Item::noted(self.url(term), term, members.len().to_string()))
            .collect();
        Listing::new(
            self.name,
            "index",
            format!("/{}/", self.name),
            Listing::titlecase(self.name),
        )
        .items(items)
        .template(self.template.clone())
    }

    /// The `/{name}/{term}/` listing of the pages under `term`.
    fn term(&self, term: &str) -> Listing {
        let items = self.terms[term].iter().map(|member| Item::of(member)).collect();
        let title = format!("{}: {term}", Listing::titlecase(self.name));
        Listing::new(self.name, Self::slug(term), self.url(term), title)
            .items(items)
            .template(self.template.clone())
    }

    fn url(&self, term: &str) -> String {
        format!("/{}/{}/", self.name, Self::slug(term))
    }

    /// A URL-safe slug: lowercase, non-alphanumeric runs collapsed to `-`.
    fn slug(term: &str) -> String {
        let mut slug = String::new();
        let mut dash = false;
        for c in term.chars() {
            if c.is_alphanumeric() {
                if dash && !slug.is_empty() {
                    slug.push('-');
                }
                dash = false;
                slug.extend(c.to_lowercase());
            } else {
                dash = true;
            }
        }
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::Group;

    #[test]
    fn slug_is_url_safe() {
        assert_eq!(Group::slug("Hello World"), "hello-world");
        assert_eq!(Group::slug("C++ & Rust"), "c-rust");
        assert_eq!(Group::slug("plain"), "plain");
    }
}
