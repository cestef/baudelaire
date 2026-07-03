//! Redirect stubs: a minimal HTML page that forwards a stale URL to its new one.

use std::fmt::{self, Write};

use super::Count;
use super::process::{Emit, Processor, Site};
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
        let target = Attr(self.target);
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

/// Escapes a string for safe inclusion in an HTML attribute value.
struct Attr<'a>(&'a str);

impl fmt::Display for Attr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '&' => f.write_str("&amp;")?,
                '"' => f.write_str("&quot;")?,
                '<' => f.write_str("&lt;")?,
                '>' => f.write_str("&gt;")?,
                _ => f.write_char(c)?,
            }
        }
        Ok(())
    }
}
