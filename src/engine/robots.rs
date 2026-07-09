//! `robots.txt` generation.

use std::fmt::Write;

use super::process::{Emit, Processor, Site};
use crate::config::Config;
use crate::error::Result;

/// Emits a `robots.txt` when a `robots` block is configured: a single
/// `User-agent: *` group with the configured disallow rules, plus a `Sitemap:`
/// line when a base `url` and the sitemap are both enabled.
pub(super) struct Robots;

impl Processor for Robots {

    fn enabled(&self, config: &Config) -> bool {
        config.robots.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let mut body = String::from("User-agent: *\n");
        if site.config.robots.disallow.is_empty() {
            body.push_str("Disallow:\n");
        } else {
            for path in &site.config.robots.disallow {
                let _ = writeln!(body, "Disallow: {path}");
            }
        }
        if let Some(base) = site.base_url().filter(|_| site.config.sitemap) {
            let _ = writeln!(body, "Sitemap: {}/sitemap.xml", base.trim_end_matches('/'));
        }
        out.file(&site.config.dist.join("robots.txt"), &body)?;
        out.note(format_args!("wrote robots.txt"))
    }
}
