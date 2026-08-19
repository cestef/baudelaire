//! `llms.txt` generation: a Markdown index of the site for LLMs.
//!
//! Follows [llmstxt.org]: an H1 site title, an optional blockquote summary,
//! then one `##` section per collection listing its pages as Markdown links.
//!
//! [llmstxt.org]: https://llmstxt.org

use std::path::PathBuf;

use super::line::Lines;
use super::{Emit, Processor, Site};
use crate::config::{BaseUrl, Config};
use crate::content::{Page, ROOT};
use crate::error::Result;
use crate::error::warning::BaseUrlMissing;

/// Emits `llms.txt` when an `llms` block is configured.
pub(super) struct Llms;

impl Processor for Llms {
    fn name(&self) -> &'static str {
        "llms.txt"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        config
            .langs()
            .iter()
            .map(|lang| Site::at(config, &[&config.scope(lang, ""), Self::FILE]))
            .collect()
    }

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
            let mut md = Lines::default();
            md.line().lit("# ").value(site.config.title(lang));
            if let Some(summary) = site
                .config
                .generate
                .llms
                .summary
                .as_deref()
                .or_else(|| site.config.description(lang))
            {
                md.blank();
                md.line().lit("> ").value(summary);
            }
            for (collection, pages) in Self::sections(&pages) {
                md.blank();
                if collection != ROOT {
                    md.line().lit("## ").value(collection);
                    md.blank();
                }
                for page in pages {
                    let link = BaseUrl::resolve(base.as_ref(), &page.permalink);
                    md.line()
                        .lit("- [")
                        .linked(page.title())
                        .lit("](")
                        .linked(link)
                        .lit(")");
                }
            }
            let path = site.dist(&[&scope, Self::FILE]);
            out.file(&path, &md.finish())?;
            out.wrote(&path);
        }
        Ok(())
    }
}

impl Llms {
    const FILE: &'static str = "llms.txt";

    /// Group pages by collection, preserving first-seen order for both the
    /// sections and the pages within them, with the unsectioned pages leading
    /// since a heading-less list after a `##` would read as part of it.
    ///
    /// Keyed by [`Page::section`], which strips the language scope a generated
    /// listing's collection carries (`fr/tags`) and would otherwise split one
    /// section into two headings.
    fn sections<'a>(pages: &[&'a Page]) -> Vec<(&'a str, Vec<&'a Page>)> {
        let mut sections: Vec<(&str, Vec<&Page>)> = Vec::new();
        for page in pages {
            let name = page.section();
            match sections.iter_mut().find(|(seen, _)| *seen == name) {
                Some((_, list)) => list.push(page),
                None => sections.push((name, vec![page])),
            }
        }
        sections.sort_by_key(|(name, _)| *name != ROOT);
        sections
    }
}

#[cfg(test)]
mod tests {
    use super::Llms;
    use crate::config::Config;
    use crate::content::{Data, Frontmatter, Page, PageId, Siblings};
    use crate::engine::emit::{Processor, Recorder, Site};
    use std::path::PathBuf;

    fn page(slug: &str, title: &str) -> Page {
        Page {
            id: PageId::new("posts", slug),
            source: PathBuf::from(format!("content/{slug}.typ")),
            frontmatter: Frontmatter {
                title: Some(title.to_owned()),
                ..Frontmatter::default()
            },
            body: String::new(),
            data: Data::Empty,
            collection: "posts".into(),
            permalink: format!("/{slug}/"),
            output: PathBuf::new(),
            template: None,
            lang: "en".into(),
            siblings: Siblings::default(),
            translations: Vec::new(),
        }
    }

    /// The index a set of pages produces.
    fn index(pages: &[Page]) -> String {
        let mut config = Config::default();
        config.generate.llms.enabled = true;
        let site = Site {
            entities: crate::content::Registries::none(),
            config: &config,
            pages,
            outputs: &[],
        };
        let mut rec = Recorder::default();
        Llms.run(&site, &mut rec).unwrap();
        rec.files
            .first()
            .map(|(_, text)| text.clone())
            .expect("no llms.txt")
    }

    #[test]
    fn every_page_is_a_link_under_its_collection() {
        let md = index(&[page("a", "A"), page("b", "B")]);
        assert!(md.contains("\n## posts\n\n- ["), "{md}");
        assert!(md.ends_with("- [A](/a/)\n- [B](/b/)\n"), "{md}");
    }

    #[test]
    fn a_link_cannot_be_ended_early_by_either_half() {
        let md = index(&[page("a", "A [draft] note")]);
        assert!(md.contains(r"- [A \[draft\] note](/a/)"), "{md}");
    }

    #[test]
    fn a_title_cannot_break_out_of_its_bullet() {
        let md = index(&[page("a", "Two\nLines")]);
        assert!(md.ends_with("- [TwoLines](/a/)\n"), "{md}");
        let bullets = md.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(bullets, 1, "{md}");
    }
}
