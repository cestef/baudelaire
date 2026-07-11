//! Redirect stubs: a minimal HTML page that forwards a stale URL to its new one.

use std::fmt;

use super::process::{Emit, Processor, Site};
use super::text::Escaped;
use crate::cli::output::Count;
use crate::error::Result;

/// Emits a redirect stub for every `redirect` old-path in a page's
/// frontmatter, forwarding it to that page's permalink.
pub(super) struct Redirects;

impl Processor for Redirects {
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let mut count = 0usize;
        for page in site.pages {
            for old in &page.frontmatter.redirect {
                out.file(
                    &site.config.destination(old),
                    &Redirect::new(&page.permalink).to_string(),
                )?;
                count += 1;
            }
        }
        if count > 0 {
            out.note(format_args!("wrote {}", Count::redirects(count)))?;
        }
        Ok(())
    }
}

/// A client-side redirect to `target`, rendered as a tiny meta-refresh page
/// with a canonical link and a manual fallback anchor.
pub(super) struct Redirect<'a> {
    target: &'a str,
}

impl<'a> Redirect<'a> {
    pub(super) fn new(target: &'a str) -> Self {
        Self { target }
    }
}

impl fmt::Display for Redirect<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let target = Escaped(self.target);
        write!(
            f,
            "<!DOCTYPE html>\n\
             <meta charset=\"utf-8\">\n\
             <meta http-equiv=\"refresh\" content=\"0; url={target}\">\n\
             <link rel=\"canonical\" href=\"{target}\">\n\
             <title>Redirecting…</title>\n\
             <a href=\"{target}\">Redirecting…</a>\n"
        )
    }
}
