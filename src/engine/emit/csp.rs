//! The generated `Content-Security-Policy`: the source lists the site
//! configured, plus the digest of every inline script and style this build
//! produced, unioned over the whole site.

use std::collections::BTreeSet;
use std::fmt;

use super::line::Plain;
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
        if self.config.enforce {
            "Content-Security-Policy"
        } else {
            "Content-Security-Policy-Report-Only"
        }
    }

    /// The directives, in the order they are written, destructured so a
    /// directive added to the config cannot be silently left out.
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
        push(
            "script-src",
            Self::sources(
                script.as_deref(),
                default.as_deref(),
                &self.digests.scripts,
                &[],
            )
            .as_ref(),
        );
        let unsafe_hashes: &[&str] = if self.digests.attrs.is_empty() {
            &[]
        } else {
            &["'unsafe-hashes'"]
        };
        let styles = &self.digests.styles | &self.digests.attrs;
        push(
            "style-src",
            Self::sources(style.as_deref(), default.as_deref(), &styles, unsafe_hashes).as_ref(),
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
        configured: Option<&str>,
        default: Option<&str>,
        digests: &BTreeSet<String>,
        keywords: &[&str],
    ) -> Option<String> {
        if digests.is_empty() {
            return configured.map(str::to_owned);
        }
        let base = configured.or(default);
        let sources = base
            .map(str::to_owned)
            .into_iter()
            .chain(keywords.iter().map(|&keyword| keyword.to_owned()))
            .chain(digests.iter().map(|digest| format!("'{digest}'")));
        Some(sources.collect::<Vec<_>>().join(" "))
    }
}

/// `default-src 'self'; script-src 'self' 'sha256-..'`, each source list
/// written through [`Plain`] since it becomes one line of `_headers`.
impl fmt::Display for Policy<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let directives = self.directives();
        for (index, (name, value)) in directives.iter().enumerate() {
            if index > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{name} {}", Plain(value.as_str()))?;
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

    #[test]
    fn a_silent_block_restricts_everything_to_the_site() {
        assert_eq!(policy("security { csp { } }", &[]), "default-src 'self'");
    }

    #[test]
    fn an_inline_digest_extends_the_directive_it_belongs_to() {
        let mut page = Inline::default();
        page.script("console.log(1)");
        let value = policy("security { csp { } }", &[page]);
        assert!(value.contains("script-src 'self' 'sha256-"), "{value}");
        assert!(!value.contains("style-src"), "{value}");
    }

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

    #[test]
    fn a_style_element_alone_needs_no_keyword() {
        let mut page = Inline::default();
        page.style("body{}");
        let value = policy("security { csp { } }", &[page]);
        assert!(value.contains("style-src 'self' 'sha256-"), "{value}");
        assert!(!value.contains("unsafe-hashes"), "{value}");
    }

    #[test]
    fn a_configured_source_cannot_break_the_line_it_is_written_on() {
        let mut config = config("security { csp { } }");
        config.security.csp.default = Some("'self'\nX-Frame-Options: ALLOWALL".into());
        let digests = Digests::default();
        let value = Policy::new(&config.security.csp, &digests).to_string();
        assert_eq!(value, "default-src 'self'X-Frame-Options: ALLOWALL");
    }

    #[test]
    fn report_only_is_a_different_header_and_the_same_policy() {
        let config = config("security { csp { enforce #false } }");
        let digests = Digests::default();
        let policy = Policy::new(&config.security.csp, &digests);
        assert_eq!(policy.header(), "Content-Security-Policy-Report-Only");
        assert_eq!(policy.to_string(), "default-src 'self'");
    }
}
