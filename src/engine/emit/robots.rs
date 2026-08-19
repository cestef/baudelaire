//! `robots.txt` generation.

use std::path::PathBuf;

use super::line::Lines;
use super::sitemap::SiteMap;
use super::{Emit, Processor, Site};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::BaseUrlMissing;

/// Emits a `robots.txt` when a `robots` block is configured: a single
/// `User-agent: *` group with the configured disallow rules, plus a `Sitemap:`
/// line when a base `url` and the sitemap are both enabled.
pub(super) struct Robots;

impl Robots {
    const FILE: &'static str = "robots.txt";
}

impl Processor for Robots {
    fn name(&self) -> &'static str {
        "the robots file"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        vec![config.paths.dist.join(Self::FILE)]
    }

    fn enabled(&self, config: &Config) -> bool {
        config.generate.robots.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let mut body = Lines::default();
        body.line().field("User-agent", "*");
        if site.config.generate.robots.disallow.is_empty() {
            body.line().lit("Disallow:");
        } else {
            for path in &site.config.generate.robots.disallow {
                body.line().field("Disallow", site.config.prefixed(path));
            }
        }
        if site.config.generate.sitemap
            && let Some(base) = site.warn_missing_base(
                out,
                BaseUrlMissing {
                    feature: "the robots.txt sitemap link",
                    effect: "omitted",
                },
            )
        {
            body.line().field("Sitemap", base.file(SiteMap::FILE));
        }
        let path = site.dist(&[Self::FILE]);
        out.file(&path, &body.finish())?;
        out.wrote(&path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Robots;
    use crate::config::Config;
    use crate::engine::emit::{Processor, Recorder, Site};

    /// The `robots.txt` a config produces.
    fn body(config: &Config) -> String {
        let site = Site {
            entities: crate::content::Registries::none(),
            relations: crate::content::Relations::none(),
            config,
            pages: &[],
            outputs: &[],
        };
        let mut rec = Recorder::default();
        Robots.run(&site, &mut rec).unwrap();
        rec.files
            .first()
            .map(|(_, text)| text.clone())
            .expect("no robots.txt")
    }

    fn config(disallow: &[&str]) -> Config {
        let mut config = Config::default();
        config.generate.robots.enabled = true;
        config.generate.robots.disallow = disallow.iter().map(|p| (*p).to_owned()).collect();
        config
    }

    #[test]
    fn an_empty_block_allows_everything() {
        assert_eq!(body(&config(&[])), "User-agent: *\nDisallow:\n");
    }

    #[test]
    fn each_disallowed_path_is_a_rule() {
        let out = body(&config(&["/a/", "/b/"]));
        assert_eq!(out, "User-agent: *\nDisallow: /a/\nDisallow: /b/\n");
    }

    #[test]
    fn a_disallowed_path_carries_the_base_path() {
        let mut config = config(&["/drafts/"]);
        config.url = Some("https://host.test/docs".to_owned());
        assert_eq!(body(&config), "User-agent: *\nDisallow: /docs/drafts/\n");
    }

    #[test]
    fn a_path_cannot_write_a_rule_of_its_own() {
        let out = body(&config(&["/a/\nAllow: /"]));
        assert_eq!(out, "User-agent: *\nDisallow: /a/Allow: /\n");
    }
}
