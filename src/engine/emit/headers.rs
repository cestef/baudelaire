//! `_headers`: what the host serving the built files is told about them, the
//! `Cache-Control` policy and the `Content-Security-Policy` both.

use std::path::PathBuf;

use std::fmt;

use super::csp::{Digests, Policy};
use super::line::Lines;
use super::{Emit, Processor, Site};
use crate::config::Config;
use crate::error::Result;

/// Emits a `_headers` rule file from the site's `headers` and `security`
/// policies.
///
/// The caching half is written from `headers { cache { } }`, the same policy
/// the S3 uploader sets per object, so a site that does both cannot state two
/// answers to one question.
pub(super) struct Headers;

impl Headers {
    const FILE: &'static str = "_headers";
}

impl Processor for Headers {
    fn name(&self) -> &'static str {
        "the headers file"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        vec![config.paths.dist.join(Self::FILE)]
    }

    /// Needs the file, and something to put in it: an empty rule file says only
    /// what the host already assumed.
    fn enabled(&self, config: &Config) -> bool {
        let headers = &config.headers;
        headers.file
            && (headers.cache.enabled || config.security.csp.enabled || !headers.rules.is_empty())
    }

    /// Rules are written most specific first, since a host matches them in
    /// order: the site's own lead and the catch-all comes last.
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let config = site.config;
        let mut body = Lines::default();
        for (pattern, headers) in &config.headers.rules {
            Self::rule(&mut body, &config.prefixed(pattern), headers);
        }
        if config.headers.cache.enabled && config.assets.fingerprint {
            let prefix = config.prefixed(&format!("{}/*", config.asset_prefix()));
            let immutable = [("Cache-Control", &config.headers.cache.immutable)];
            Self::rule(&mut body, &prefix, &immutable);
        }
        Self::rule(&mut body, &config.prefixed("/*"), &Self::catchall(site));
        let path = site.dist(&[Self::FILE]);
        out.file(&path, &body.finish())?;
        out.wrote(&path);
        Ok(())
    }
}

impl Headers {
    /// The headers every path gets: the caching default, and the policy this
    /// build's own pages were assembled into.
    fn catchall(site: &Site) -> Vec<(&'static str, String)> {
        let config = site.config;
        let mut headers = Vec::new();
        if config.headers.cache.enabled {
            headers.push(("Cache-Control", config.headers.cache.default.clone()));
        }
        if config.security.csp.enabled {
            let digests: Digests = site.outputs.iter().map(|out| out.inline).collect();
            let policy = Policy::new(&config.security.csp, &digests);
            headers.push((policy.header(), policy.to_string()));
        }
        headers
    }

    /// One rule: the path pattern on its own line, then each header indented
    /// beneath it, then the blank line that ends the record.
    ///
    /// A record with no headers under it is skipped, since a bare pattern is a
    /// rule a host parses and that says nothing.
    fn rule(body: &mut Lines, pattern: &str, headers: &[(impl fmt::Display, impl fmt::Display)]) {
        if headers.is_empty() {
            return;
        }
        body.line().value(pattern);
        for (name, value) in headers {
            body.line().lit("  ").pair(name, value);
        }
        body.blank();
    }
}

#[cfg(test)]
mod tests {
    use super::Headers;
    use crate::config::Config;
    use crate::engine::emit::{Processor, Recorder, Site};

    fn config(text: &str) -> Config {
        Config::parse(text).expect("should parse")
    }

    /// The `_headers` body a config produces.
    fn body(config: &Config) -> String {
        let site = Site {
            entities: crate::content::Registries::none(),
            relations: crate::content::Relations::none(),
            config,
            pages: &[],
            outputs: &[],
        };
        let mut rec = Recorder::default();
        Headers.run(&site, &mut rec).unwrap();
        rec.files
            .iter()
            .find(|(path, _)| path.ends_with("_headers"))
            .map(|(_, text)| text.clone())
            .expect("no _headers")
    }

    #[test]
    fn needs_a_policy_as_well_as_the_file() {
        assert!(!Headers.enabled(&config("headers { }")));
        assert!(!Headers.enabled(&config("headers #false { cache { } }")));
        assert!(Headers.enabled(&config("headers { cache { } }")));
    }

    #[test]
    fn a_policy_alone_earns_the_file() {
        assert!(Headers.enabled(&config("headers { }\nsecurity { csp { } }")));
        let body = body(&config("headers { }\nsecurity { csp { } }"));
        assert!(
            body.contains("Content-Security-Policy: default-src 'self'"),
            "{body}"
        );
        assert!(!body.contains("Cache-Control"), "{body}");
    }

    #[test]
    fn a_rule_of_the_sites_own_earns_the_file() {
        let text =
            "headers {\n  rules {\n    \"/v*/*\" {\n      X-Robots-Tag \"noindex\"\n    }\n  }\n}";
        assert!(Headers.enabled(&config(text)));

        let body = body(&config(text));
        assert!(body.contains("/v*/*\n  X-Robots-Tag: noindex\n"), "{body}");
        assert!(!body.contains("Cache-Control"), "{body}");
    }

    #[test]
    fn the_sites_own_rules_precede_the_derived_ones() {
        let body = body(&config(
            "headers {\n  cache { }\n  rules {\n    \"/private/*\" {\n      X-Robots-Tag \"noindex\"\n    }\n  }\n}",
        ));
        let own = body.find("/private/*").expect("no rule of its own");
        let catchall = body.rfind("/*\n").expect("no catch-all");
        assert!(own < catchall, "{body}");
    }

    #[test]
    fn a_rule_is_written_under_the_base_path() {
        let body = body(&config(
            "url \"https://e.xyz/docs/\"\nheaders {\n  rules {\n    \"/private/*\" {\n      X-Robots-Tag \"noindex\"\n    }\n  }\n}",
        ));
        assert!(body.contains("/docs/private/*"), "{body}");
    }

    #[test]
    fn a_header_name_cannot_open_a_line_of_its_own() {
        let mut config = config("headers { }");
        config.headers.rules = vec![(
            "/*".to_owned(),
            vec![("X-A\nX-B".to_owned(), "v".to_owned())],
        )];
        let body = body(&config);
        assert!(body.contains("  X-AX-B: v\n"), "{body}");
    }

    #[test]
    fn only_a_fingerprinted_build_declares_its_assets_immutable() {
        let hashed = body(&config(
            "headers { cache { } }\nassets { fingerprint #true }",
        ));
        let assets = hashed.find("/assets/*").expect("no asset rule");
        let catchall = hashed.rfind("/*\n").expect("no catch-all");
        assert!(
            assets < catchall,
            "the catch-all precedes the asset rule: {hashed}"
        );
        assert!(hashed.contains("immutable"), "{hashed}");

        let plain = body(&config("headers { cache { } }"));
        assert!(!plain.contains("/assets/*"), "{plain}");
        assert!(plain.contains("must-revalidate"), "{plain}");
    }
}
