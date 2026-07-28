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
        config.llms.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let base = site.warn_missing_base(
            out,
            BaseUrlMissing {
                feature: "llms.txt",
                effect: "emitted with relative links",
            },
        )?;
        // One file per language, beside that language's feeds and search index.
        // A single flat file interleaved every language under one `## blog`
        // heading, and split taxonomies into `## tags` and `## fr/tags` because
        // generated listings carry the scoped collection id.
        for lang in site.config.langs() {
            let scope = site.config.scope(lang, "");
            let pages: Vec<&Page> = site.pages.iter().filter(|p| p.lang == lang).collect();
            if pages.is_empty() {
                continue;
            }
            let mut md = format!("# {}\n", site.config.title(lang));
            if let Some(summary) = &site.config.llms.summary {
                let _ = write!(md, "\n> {summary}\n");
            }
            for (collection, pages) in Self::sections(&pages, lang) {
                let _ = write!(md, "\n## {collection}\n\n");
                for page in pages {
                    let link = BaseUrl::resolve(base.as_ref(), &page.permalink);
                    let _ = writeln!(md, "- [{}]({link})", page.title());
                }
            }
            let path = site.config.dist.join(&scope).join(Self::FILE);
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
    /// (`fr/tags`), so the scope is stripped here: within one language's file
    /// it is noise, and it split what is one section into two headings.
    fn sections<'a>(pages: &[&'a Page], lang: &str) -> Vec<(&'a str, Vec<&'a Page>)> {
        let prefix = format!("{lang}/");
        let mut sections: Vec<(&str, Vec<&Page>)> = Vec::new();
        for page in pages {
            let name = page
                .collection
                .strip_prefix(&prefix)
                .unwrap_or(&page.collection);
            match sections.iter_mut().find(|(seen, _)| *seen == name) {
                Some((_, list)) => list.push(page),
                None => sections.push((name, vec![page])),
            }
        }
        sections
    }
}
