//! Generation of taxonomy index pages.
//!
//! For each configured taxonomy with `listing`, this builds a [`Listing`]
//! per term (plus one listing every term), which lower to synthetic pages in
//! the normal compile pipeline.

use std::collections::BTreeMap;

use crate::config::Permalink;
use crate::config::{Config, TaxonomyConfig};
use crate::content::generate::{Generate, PlanCtx};
use crate::content::listing::{Item, Listing, Titlecase};
use crate::content::pagination::Paged;
use crate::content::{Page, Slug, Strings};
use crate::error::{ContentError, Result};

/// Builds the taxonomy index pages for a site.
pub struct Taxonomy;

impl Generate for Taxonomy {
    /// Generate index + term pages for every configured taxonomy that requests
    /// an index, drawing terms from the planned pages' frontmatter. Terms are
    /// grouped per language: a French and an English `rust` tag are separate
    /// `/fr/tags/rust/` and `/tags/rust/` pages, never a merged one.
    fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        for group in Self::groups(ctx.config, ctx.pages) {
            group.build(&mut out)?;
        }
        Ok(out)
    }
}

impl Taxonomy {
    /// Every indexed taxonomy's terms, one [`Group`] per taxonomy per language.
    ///
    /// The single grouping rule behind both the generated term pages and the
    /// per-term feeds, so a term that has a page always has a feed at the same
    /// URL and neither can drift from the other's idea of what a term contains.
    pub(crate) fn groups<'a>(config: &'a Config, pages: &'a [Page]) -> Vec<Group<'a>> {
        config
            .content
            .taxonomies
            .iter()
            .filter(|(_, cfg)| cfg.listing)
            .flat_map(|(name, cfg)| {
                config
                    .langs()
                    .into_iter()
                    .map(move |lang| Group::new(name, cfg, pages, lang, config))
            })
            .collect()
    }
}

/// One taxonomy's terms and the pages under each, within a single language.
pub(crate) struct Group<'a> {
    /// Taxonomy name, e.g. `tags`; also its URL prefix and section id.
    name: &'a str,
    /// Optional user template for the generated pages.
    template: Option<String>,
    /// Members per term page; `None` puts every member on one.
    paginate: Option<usize>,
    /// Path segment before a term page's number.
    prefix: String,
    /// term -> member pages, each term's members in the taxonomy's own order.
    terms: BTreeMap<String, Vec<&'a Page>>,
    /// The language whose pages this group indexes; localizes every URL.
    lang: &'a str,
    config: &'a Config,
}

impl<'a> Group<'a> {
    fn new(
        name: &'a str,
        cfg: &TaxonomyConfig,
        pages: &'a [Page],
        lang: &'a str,
        config: &'a Config,
    ) -> Self {
        let mut terms: BTreeMap<String, Vec<&Page>> = BTreeMap::new();
        for page in pages.iter().filter(|p| p.lang == lang && p.listed(config)) {
            if let Some(values) = page.frontmatter.taxonomies.get(&cfg.key) {
                for term in values {
                    terms.entry(term.clone()).or_default().push(page);
                }
            }
        }
        // The collection comparator, on the taxonomy's own declared order: one
        // rule, so a term page and a collection index listing the same posts
        // cannot come in two orders.
        for members in terms.values_mut() {
            members.sort_by(|a, b| Page::compare(cfg.sort, a, b));
            if cfg.reverse {
                members.reverse();
            }
        }
        Self {
            name,
            template: cfg.template.clone(),
            paginate: cfg.paginate,
            prefix: cfg.prefix.clone(),
            terms,
            lang,
            config,
        }
    }

    /// A localized URL from already-slugged segments.
    fn url(&self, segments: &[&str]) -> String {
        self.config.localize(self.lang, &Permalink::join(segments))
    }

    /// Emit the index listing and one listing per term. Resolves every term's
    /// slug up front so an empty slug or a collision (`C++`/`C--` -> `c`) is a
    /// precise error, not a silent `/tags//` or overwrite.
    fn build(&self, out: &mut Vec<Page>) -> Result<()> {
        if self.terms.is_empty() {
            return Ok(());
        }
        let resolved = self.resolve()?;
        out.push(self.index(&resolved).into_page(self.config));
        for term in &resolved {
            self.term(term, out);
        }
        Ok(())
    }

    /// The language this group indexes, so a consumer can title and localize
    /// its output the same way the term pages are.
    pub(crate) fn lang(&self) -> &'a str {
        self.lang
    }

    /// Each term paired with its localized URL, checked for empty slugs and
    /// collisions (per language, so identical terms across languages never
    /// clash).
    pub(crate) fn resolve(&self) -> Result<Vec<Term<'_>>> {
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        let mut resolved = Vec::with_capacity(self.terms.len());
        for (name, members) in &self.terms {
            let slug = Slug::require(name)?.into_string();
            if let Some(prev) = seen.insert(slug.clone(), name) {
                return Err(ContentError::term_collision(self.name, &slug, prev, name).into());
            }
            resolved.push(Term {
                url: self.url(&[self.name, &slug]),
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
            Listing::INDEX,
            self.url(&[self.name]),
            Titlecase(self.name).to_string(),
        )
        .items(items)
        .template(self.template.clone())
        .lang(self.lang)
    }

    /// How a term is titled wherever it is presented: its listing page, and the
    /// feed that sits beside it.
    pub(crate) fn title(&self, term: &Term<'_>) -> String {
        format!("{}: {}", Titlecase(self.name), term.name)
    }

    /// The `/{name}/{term}/` listing of the pages under `term`, chunked when
    /// the taxonomy sets a `paginate` count.
    ///
    /// Through the same [`Paged`] a collection index uses, so `/tags/rust/` and
    /// `/blog/` name page 2 the same way. A term used to list every member on
    /// one page whatever its size, which on a blog with three years of `#rust`
    /// posts meant all 400 of them.
    fn term(&self, term: &Term<'_>, out: &mut Vec<Page>) {
        let root = [self.name, term.slug.as_str()];
        let paged = Paged::new(
            &root,
            None,
            &self.prefix,
            &term.slug,
            self.paginate.unwrap_or(term.members.len()),
            self.lang,
            self.config,
        );
        let title = self.title(term);
        let strings = Strings::new(self.config, self.lang);
        let chunks = paged.chunks(term.members);
        for (index, members) in chunks.iter().enumerate() {
            let number = index + 1;
            let items = members
                .iter()
                .map(|member| Item::of(member, &strings))
                .collect();
            let listing = Listing::new(
                self.name,
                paged.slug(number),
                paged.url(number),
                paged.title(&title, number),
            )
            .items(items)
            .nav(paged.nav(number, chunks.len()))
            .template(self.template.clone())
            .lang(self.lang);
            out.push(listing.into_page(self.config));
        }
    }
}

/// A taxonomy term with its resolved, collision-checked URL. `members` is a
/// covariant slice so it borrows the group's page vectors without fighting the
/// invariance of `&Vec`.
pub(crate) struct Term<'a> {
    pub(crate) name: &'a str,
    slug: String,
    /// The term listing's localized URL (`/fr/tags/rust/`), which a term feed
    /// also sits under.
    pub(crate) url: String,
    pub(crate) members: &'a [&'a Page],
}
