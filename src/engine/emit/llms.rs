//! `llms.txt` generation: a Markdown index of the site for LLMs.
//!
//! Follows [llmstxt.org]: an H1 site title, an optional blockquote summary, then
//! one `##` section per collection listing its pages as Markdown links.
//!
//! [llmstxt.org]: https://llmstxt.org

use std::fmt::Write;

use super::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::error::Result;
use crate::error::warning::BaseUrlMissing;

/// Emits `llms.txt` when an `llms` block is configured.
pub(super) struct Llms;

impl Processor for Llms {
    fn enabled(&self, config: &Config) -> bool {
        config.generate.llms.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let base = site.warn_missing_base(
            out,
            BaseUrlMissing {
                feature: "llms.txt",
                effect: "emitted with relative links",
            },
        );
        // One file per language, beside that language's feeds and search index.
        // A single flat file interleaved every language under one `## blog`
        // heading, and split taxonomies into `## tags` and `## fr/tags` because
        // generated listings carry the scoped collection id.
        for lang in site.config.langs() {
            let scope = site.config.scope(lang, "");
            let pages: Vec<&Page> = site
                .pages
                .iter()
                .filter(|p| p.lang == lang && p.listed(site.config))
                .collect();
            if pages.is_empty() {
                continue;
            }
            let mut md = format!("# {}\n", site.config.title(lang));
            // Its own `summary` first, then the site's `description`: the two
            // answer the same question, and a site that stated one should not
            // have to state it twice to fill this line.
            if let Some(summary) = site
                .config
                .generate
                .llms
                .summary
                .as_deref()
                .or_else(|| site.config.description(lang))
            {
                let _ = write!(md, "\n> {summary}\n");
            }
            for (collection, pages) in Self::sections(&pages) {
                let _ = write!(md, "\n## {collection}\n\n");
                for page in pages {
                    let link = BaseUrl::resolve(base.as_ref(), &page.permalink);
                    let _ = writeln!(md, "- [{}]({link})", page.title());
                }
            }
            let path = site.dist(&[&scope, Self::FILE]);
            out.file(&path, &md)?;
            out.note(format_args!("wrote {}", path.display()));
        }
        Ok(())
    }
}

impl Llms {
    const FILE: &'static str = "llms.txt";

    /// Group pages by collection, preserving first-seen order for both the
    /// sections and the pages within them.
    ///
    /// A generated listing's collection is the language-scoped section
    /// (`fr/tags`), which [`Page::section`] is what strips: within one
    /// language's file the scope is noise, and it split one section into two
    /// headings.
    fn sections<'a>(pages: &[&'a Page]) -> Vec<(&'a str, Vec<&'a Page>)> {
        let mut sections: Vec<(&str, Vec<&Page>)> = Vec::new();
        for page in pages {
            let name = page.section();
            match sections.iter_mut().find(|(seen, _)| *seen == name) {
                Some((_, list)) => list.push(page),
                None => sections.push((name, vec![page])),
            }
        }
        sections
    }
}
