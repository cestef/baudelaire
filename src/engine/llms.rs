//! `llms.txt` generation: a Markdown index of the site for LLMs.
//!
//! Follows [llmstxt.org]: an H1 site title, an optional blockquote summary, then
//! one `##` section per collection listing its pages as Markdown links.
//!
//! [llmstxt.org]: https://llmstxt.org

use std::fmt::Write;

use super::process::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config};
use crate::content::Page;
use crate::error::Result;

/// Emits `llms.txt` when an `llms` block is configured.
pub(super) struct Llms;

impl Processor for Llms {

    fn enabled(&self, config: &Config) -> bool {
        config.llms.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let base = site.config.base();
        if base.is_none() {
            out.warn(format_args!("llms.txt has no `url` set — links will be relative"))?;
        }
        let mut md = format!("# {}\n", site.config.label());
        if let Some(summary) = &site.config.llms.summary {
            let _ = write!(md, "\n> {summary}\n");
        }
        for (collection, pages) in Self::by_collection(site.pages) {
            let _ = write!(md, "\n## {collection}\n\n");
            for page in pages {
                let link = BaseUrl::resolve(base.as_ref(), &page.permalink);
                let _ = writeln!(md, "- [{}]({link})", page.title());
            }
        }
        out.file(&site.config.dist.join("llms.txt"), &md)?;
        out.note(format_args!("wrote llms.txt"))
    }
}

impl Llms {
    /// Group pages by collection, preserving first-seen order for both the
    /// sections and the pages within them.
    fn by_collection(pages: &[Page]) -> Vec<(&str, Vec<&Page>)> {
        let mut sections: Vec<(&str, Vec<&Page>)> = Vec::new();
        for page in pages {
            match sections.iter_mut().find(|(name, _)| *name == page.collection) {
                Some((_, list)) => list.push(page),
                None => sections.push((&page.collection, vec![page])),
            }
        }
        sections
    }
}
