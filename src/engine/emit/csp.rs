//! The generated `Content-Security-Policy`.
//!
//! Assembled from two halves the site cannot write down by itself: the source
//! lists it configured, and the digest of every inline script and style this
//! build actually produced. [`headers`](super::headers) writes the result; this
//! only knows how to say it.
//!
//! One policy covers the whole site, with the digests of every page unioned and
//! deduplicated. Per-page rules would be tighter, but `_headers` applies every
//! matching rule, so a page carrying its own policy would be served two of them
//! and a browser enforces the intersection: the catch-all, which does not name
//! that page's inline blocks, would block them. The looseness this trades away
//! is small, since a digest allows one exact body and nothing else, and the
//! bodies are the build's own.

use std::collections::BTreeSet;
use std::fmt;

use crate::config::CspConfig;
use crate::render::Inline;

/// The digests a policy names, gathered across the site.
#[derive(Default)]
pub(super) struct Digests {
    scripts: BTreeSet<String>,
    styles: BTreeSet<String>,
    /// The `style=""` attributes, which need `'unsafe-hashes'` alongside them.
    attrs: BTreeSet<String>,
}

impl<'a> FromIterator<&'a Inline> for Digests {
    fn from_iter<I: IntoIterator<Item = &'a Inline>>(pages: I) -> Self {
        let mut digests = Self::default();
        for page in pages {
            digests.scripts.extend(page.scripts.iter().cloned());
            digests.styles.extend(page.styles.iter().cloned());
            digests.attrs.extend(page.attrs.iter().cloned());
        }
        digests
    }
}

/// One policy, ready to be written as a header value.
pub(super) struct Policy<'a> {
    config: &'a CspConfig,
    digests: &'a Digests,
}

impl<'a> Policy<'a> {
    pub(super) fn new(config: &'a CspConfig, digests: &'a Digests) -> Self {
        Self { config, digests }
    }

    /// The header this policy is served under: enforcing, or reporting only.
    pub(super) fn header(&self) -> &'static str {
        match self.config.enforce {
            true => "Content-Security-Policy",
            false => "Content-Security-Policy-Report-Only",
        }
    }

    /// The directives, in the order they are written.
    ///
    /// Destructured, so a directive added to the config cannot be silently left
    /// out of the policy: the compiler asks for it here.
    fn directives(&self) -> Vec<(&'static str, String)> {
        let CspConfig {
            enabled: _,
            enforce: _,
            hashes: _,
            default,
            script,
            style,
            img,
            font,
            connect,
            frame,
            object,
            base,
            form,
            report,
        } = self.config;
        let mut out = Vec::new();
        let mut push = |name, value: Option<&String>| {
            if let Some(value) = value {
                out.push((name, value.clone()));
            }
        };
        push("default-src", default.as_ref());
        // `script-src` and `style-src` are the two a build has something to add
        // to. Each is emitted when it is configured *or* when the build has
        // digests to name, falling back to whatever `default-src` says, since a
        // directive that is present replaces the fallback rather than extending
        // it: naming the digests alone would have dropped `'self'` and blocked
        // every file the page loads.
        push(
            "script-src",
            Self::sources(script, default, self.digests.scripts.clone(), &[]).as_ref(),
        );
        // A style *attribute* is allowed by digest only in the company of
        // `'unsafe-hashes'`, which is what its name says it is: it lets a hash
        // match somewhere the syntax otherwise never looks. It is still an
        // allowlist of exact strings this build produced, and the alternative
        // is `'unsafe-inline'`, which allows every inline style there could be.
        let unsafe_hashes: &[&str] = match self.digests.attrs.is_empty() {
            true => &[],
            false => &["'unsafe-hashes'"],
        };
        push(
            "style-src",
            Self::sources(
                style,
                default,
                &self.digests.styles | &self.digests.attrs,
                unsafe_hashes,
            )
            .as_ref(),
        );
        push("img-src", img.as_ref());
        push("font-src", font.as_ref());
        push("connect-src", connect.as_ref());
        push("frame-src", frame.as_ref());
        push("object-src", object.as_ref());
        push("base-uri", base.as_ref());
        push("form-action", form.as_ref());
        push("report-uri", report.as_ref());
        out
    }

    /// A directive's source list: what the site configured (or inherits from
    /// `default-src`), then `keywords`, then the digests this build produced.
    /// `None` when the directive says nothing the fallback does not already
    /// say.
    fn sources(
        configured: &Option<String>,
        default: &Option<String>,
        digests: BTreeSet<String>,
        keywords: &[&str],
    ) -> Option<String> {
        if digests.is_empty() {
            return configured.clone();
        }
        let base = configured.as_ref().or(default.as_ref());
        let sources = base
            .cloned()
            .into_iter()
            .chain(keywords.iter().map(|&keyword| keyword.to_owned()))
            .chain(digests.iter().map(|digest| format!("'{digest}'")));
        Some(sources.collect::<Vec<_>>().join(" "))
    }
}

/// `default-src 'self'; script-src 'self' 'sha256-..'`, the header value itself.
impl fmt::Display for Policy<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let directives = self.directives();
        for (index, (name, value)) in directives.iter().enumerate() {
            if index > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{name} {value}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Digests, Policy};
    use crate::config::Config;
    use crate::render::Inline;

    fn config(text: &str) -> Config {
        Config::parse(text).expect("should parse")
    }

    fn policy(text: &str, pages: &[Inline]) -> String {
        let config = config(text);
        let digests: Digests = pages.iter().collect();
        Policy::new(&config.security.csp, &digests).to_string()
    }

    /// A bare block is a real policy: everything falls back to `'self'`.
    #[test]
    fn a_silent_block_restricts_everything_to_the_site() {
        assert_eq!(policy("security { csp { } }", &[]), "default-src 'self'");
    }

    /// An inline script's digest joins `script-src`, and the fallback comes
    /// with it: naming the digest alone would have blocked every *file* the
    /// page loads, since a stated directive replaces `default-src` rather than
    /// extending it.
    #[test]
    fn an_inline_digest_extends_the_directive_it_belongs_to() {
        let mut page = Inline::default();
        page.script("console.log(1)");
        let value = policy("security { csp { } }", &[page]);
        assert!(value.contains("script-src 'self' 'sha256-"), "{value}");
        assert!(!value.contains("style-src"), "{value}");
    }

    /// The same body on two pages is one digest, and the whole site's policy is
    /// one header.
    #[test]
    fn digests_are_unioned_across_pages_and_deduplicated() {
        let mut first = Inline::default();
        first.script("shared()");
        let mut second = Inline::default();
        second.script("shared()");
        second.style("body{}");
        let value = policy("security { csp { } }", &[first, second]);
        assert_eq!(value.matches("'sha256-").count(), 2, "{value}");
    }

    /// A configured directive is written verbatim, and the digests extend it
    /// rather than the fallback.
    #[test]
    fn a_configured_directive_is_what_the_digests_extend() {
        let mut page = Inline::default();
        page.script("x()");
        let value = policy(
            "security { csp { default \"'none'\"; script \"'self' https://cdn.example.com\" } }",
            &[page],
        );
        assert!(value.starts_with("default-src 'none'; "), "{value}");
        assert!(
            value.contains("script-src 'self' https://cdn.example.com 'sha256-"),
            "{value}"
        );
    }

    /// A `style` attribute is inline style, and this build emits them without
    /// being asked: typst resolves an element's CSS properties into one. Naming
    /// only `<style>` elements dropped every one of them, silently, in the
    /// browser and nowhere else.
    #[test]
    fn a_style_attribute_is_named_and_takes_unsafe_hashes_with_it() {
        let mut page = Inline::default();
        page.attr("white-space: pre-wrap");
        let value = policy("security { csp { } }", &[page]);
        assert!(
            value.contains("style-src 'self' 'unsafe-hashes' 'sha256-"),
            "{value}"
        );
        assert!(!value.contains("script-src"), "{value}");
    }

    /// `'unsafe-hashes'` is what a style *attribute* needs, so a page with only
    /// a `<style>` element does not get it.
    #[test]
    fn a_style_element_alone_needs_no_keyword() {
        let mut page = Inline::default();
        page.style("body{}");
        let value = policy("security { csp { } }", &[page]);
        assert!(value.contains("style-src 'self' 'sha256-"), "{value}");
        assert!(!value.contains("unsafe-hashes"), "{value}");
    }

    /// Rolling a policy out means reporting without blocking.
    #[test]
    fn report_only_is_a_different_header_and_the_same_policy() {
        let config = config("security { csp { enforce #false } }");
        let digests = Digests::default();
        let policy = Policy::new(&config.security.csp, &digests);
        assert_eq!(policy.header(), "Content-Security-Policy-Report-Only");
        assert_eq!(policy.to_string(), "default-src 'self'");
    }
}
