//! Generation of taxonomy index pages.
//!
//! For each configured taxonomy with `index=true`, this builds a [`Listing`]
//! per term (plus one listing every term), which lower to synthetic pages in
//! the normal compile pipeline.

use std::collections::BTreeMap;

use crate::config::{Config, TaxonomyConfig};
use crate::content::generate::{Generate, PlanCtx};
use crate::content::listing::{Item, Listing, Titlecase};
use crate::content::{Page, Permalink, Slug};
use crate::error::{ContentError, Result};

/// Builds the taxonomy index pages for a site.
pub struct Taxonomy;

impl Generate for Taxonomy {
    /// Generate index + term pages for every configured taxonomy that requests
    /// an index, drawing terms from the planned pages' frontmatter.
    fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        for (name, cfg) in &ctx.config.taxonomies {
            if cfg.index {
                Group::new(name, cfg, ctx.pages).build(ctx.config, &mut out)?;
            }
        }
        Ok(out)
    }
}

/// One taxonomy's terms and the pages under each.
struct Group<'a> {
    /// Taxonomy name, e.g. `tags`; also its URL prefix and section id.
    name: &'a str,
    /// Optional user template for the generated pages.
    template: Option<String>,
    /// term -> member pages, sorted by term then title.
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

    /// Emit the index listing and one listing per term. Resolves every term's
    /// slug up front so an empty slug or a collision (`C++`/`C--` -> `c`) is a
    /// precise error, not a silent `/tags//` or overwrite.
    fn build(&self, config: &Config, out: &mut Vec<Page>) -> Result<()> {
        if self.terms.is_empty() {
            return Ok(());
        }
        let resolved = self.resolve()?;
        out.push(self.index(&resolved).into_page(config));
        for term in &resolved {
            out.push(self.term(term).into_page(config));
        }
        Ok(())
    }

    /// Each term paired with its URL, checked for empty slugs and collisions.
    fn resolve(&self) -> Result<Vec<Term<'_>>> {
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        let mut resolved = Vec::with_capacity(self.terms.len());
        for (name, members) in &self.terms {
            let slug = Slug::require(name)?.into_string();
            if let Some(prev) = seen.insert(slug.clone(), name) {
                return Err(ContentError::term_collision(self.name, &slug, prev, name).into());
            }
            resolved.push(Term {
                url: Permalink::join(&[self.name, &slug]),
                name,
                slug,
                members: members.as_slice(),
            });
        }
        Ok(resolved)
    }

    /// The `/{name}/` listing of every term with its member count.
    fn index(&self, terms: &[Term<'_>]) -> Listing {
        let items = terms
            .iter()
            .map(|t| Item::noted(t.url.clone(), t.name, t.members.len().to_string()))
            .collect();
        Listing::new(
            self.name,
            "index",
            Permalink::join(&[self.name]),
            Titlecase(self.name).to_string(),
        )
        .items(items)
        .template(self.template.clone())
    }

    /// The `/{name}/{term}/` listing of the pages under `term`.
    fn term(&self, term: &Term<'_>) -> Listing {
        let items = term.members.iter().map(|member| Item::of(member)).collect();
        let title = format!("{}: {}", Titlecase(self.name), term.name);
        Listing::new(self.name, term.slug.clone(), term.url.clone(), title)
            .items(items)
            .template(self.template.clone())
    }
}

/// A taxonomy term with its resolved, collision-checked URL. `members` is a
/// covariant slice so it borrows the group's page vectors without fighting the
/// invariance of `&Vec`.
struct Term<'a> {
    name: &'a str,
    slug: String,
    url: String,
    members: &'a [&'a Page],
}
