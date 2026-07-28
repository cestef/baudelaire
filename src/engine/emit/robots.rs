//! `robots.txt` generation.

use std::fmt::Write;

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
    /// The output file name, at the `dist` root.
    const FILE: &'static str = "robots.txt";
}

impl Processor for Robots {
    fn enabled(&self, config: &Config) -> bool {
        config.generate.robots.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let mut body = String::from("User-agent: *\n");
        if site.config.generate.robots.disallow.is_empty() {
            body.push_str("Disallow:\n");
        } else {
            for path in &site.config.generate.robots.disallow {
                let _ = writeln!(body, "Disallow: {path}");
            }
        }
        if site.config.generate.sitemap
            && let Some(base) = site.warn_missing_base(
                out,
                BaseUrlMissing {
                    feature: "the robots.txt sitemap link",
                    effect: "omitted",
                },
            )?
        {
            let _ = writeln!(body, "Sitemap: {}", base.file(SiteMap::FILE));
        }
        out.file(&site.dist(&[Self::FILE]), &body)?;
        out.note(format_args!("wrote {}", Self::FILE));
        Ok(())
    }
}
