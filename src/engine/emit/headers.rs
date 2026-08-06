//! `_headers`: what the host serving the built files is told about them, the
//! `Cache-Control` policy and the `Content-Security-Policy` both.

use std::fmt;

use super::csp::{Digests, Policy};
use super::line::Lines;
use super::{Emit, Processor, Site};
use crate::config::Config;
use crate::error::Result;

/// Emits a `_headers` rule file from the site's `caching` and `security`
/// policies.
///
/// The caching half is the same policy the S3 uploader sets per object, said
/// once to a host that reads it from the publish directory (Netlify,
/// Cloudflare Pages). Both are written from `caching { }` rather than each
/// carrying its own, so a site that deploys to a bucket *and* ships `_headers`
/// cannot state two different answers to one question.
pub(super) struct Headers;

impl Headers {
    /// The rule file name. Netlify and Cloudflare Pages spell it the same.
    const FILE: &'static str = "_headers";
}

impl Processor for Headers {
    /// Needs the file, and something to put in it. `generate { headers }` alone
    /// would emit an empty rule file, which reads as "no policy" and is what the
    /// host already assumed. A rule the site wrote itself is something to put in
    /// it, as much as either derived policy is.
    fn enabled(&self, config: &Config) -> bool {
        let headers = &config.generate.headers;
        headers.enabled
            && (config.caching.enabled || config.security.csp.enabled || !headers.rules.is_empty())
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let config = site.config;
        let mut body = Lines::default();
        // The site's own rules lead. They are the only ones whose order the
        // author controls, and the two derived rules below end in a catch-all
        // that would otherwise be answered first.
        for (pattern, headers) in &config.generate.headers.rules {
            Self::rule(&mut body, &config.prefixed(pattern), headers);
        }
        // Most specific first: a host matches rules in order, so the catch-all
        // has to come last or it would claim the asset paths too.
        //
        // The asset rule only earns its place when this build content-addresses
        // the names under that prefix. Without `assets { fingerprint }` an asset
        // keeps its authored name across builds and is exactly as mutable as a
        // page, which is the same call `CacheControl::header` makes per object.
        if config.caching.enabled && config.assets.fingerprint {
            let prefix = config.prefixed(&format!("{}/*", config.asset_prefix()));
            let immutable = [("Cache-Control", &config.caching.immutable)];
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
        if config.caching.enabled {
            headers.push(("Cache-Control", config.caching.default.clone()));
        }
        if config.security.csp.enabled {
            let digests: Digests = site.outputs.iter().map(|out| out.inline).collect();
            let policy = Policy::new(&config.security.csp, &digests);
            headers.push((policy.header(), policy.to_string()));
        }
        headers
    }

    /// One rule: the path pattern on its own line, then each header indented
    /// beneath it, then the blank line that ends the record. The format both
    /// hosts read.
    ///
    /// The name is a value here rather than a literal, because a site's own
    /// rules name their own headers; the derived ones pass a literal that
    /// happens to satisfy the same signature.
    fn rule(body: &mut Lines, pattern: &str, headers: &[(impl fmt::Display, impl fmt::Display)]) {
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

    /// Both halves are required. A rule file with no policy in it says nothing
    /// the host did not already assume, and a policy with nowhere to go is what
    /// the S3 uploader is for.
    #[test]
    fn needs_a_policy_as_well_as_the_file() {
        assert!(!Headers.enabled(&config("generate { headers #true }")));
        assert!(!Headers.enabled(&config("caching { }")));
        assert!(Headers.enabled(&config("generate { headers #true }\ncaching { }")));
    }

    /// A policy is enough on its own: a site can state one without saying
    /// anything about caching.
    #[test]
    fn a_policy_alone_earns_the_file() {
        assert!(Headers.enabled(&config("generate { headers #true }\nsecurity { csp { } }")));
        let body = body(&config("generate { headers #true }\nsecurity { csp { } }"));
        assert!(
            body.contains("Content-Security-Policy: default-src 'self'"),
            "{body}"
        );
        assert!(!body.contains("Cache-Control"), "{body}");
    }

    /// A rule the site wrote itself earns the file on its own: a header that is
    /// neither a cache policy nor a security one is still a header, and this
    /// used to be the case that had to be written by hand after the build.
    #[test]
    fn a_rule_of_the_sites_own_earns_the_file() {
        let text = "generate {\n  headers {\n    \"/v*/*\" {\n      X-Robots-Tag \"noindex\"\n    }\n  }\n}";
        assert!(Headers.enabled(&config(text)));

        let body = body(&config(text));
        assert!(body.contains("/v*/*\n  X-Robots-Tag: noindex\n"), "{body}");
        assert!(!body.contains("Cache-Control"), "{body}");
    }

    /// The site's rules lead, because the derived ones end in a catch-all: a
    /// host reads the file in order and the first match wins.
    #[test]
    fn the_sites_own_rules_precede_the_derived_ones() {
        let body = body(&config(
            "caching { }\ngenerate {\n  headers {\n    \"/private/*\" {\n      X-Robots-Tag \"noindex\"\n    }\n  }\n}",
        ));
        let own = body.find("/private/*").expect("no rule of its own");
        let catchall = body.rfind("/*\n").expect("no catch-all");
        assert!(own < catchall, "{body}");
    }

    /// Base-path prefixed like every other pattern in the file: a site served
    /// under a subdirectory states paths relative to itself.
    #[test]
    fn a_rule_is_written_under_the_base_path() {
        let body = body(&config(
            "url \"https://e.xyz/docs/\"\ngenerate {\n  headers {\n    \"/private/*\" {\n      X-Robots-Tag \"noindex\"\n    }\n  }\n}",
        ));
        assert!(body.contains("/docs/private/*"), "{body}");
    }

    /// A header name is the author's text, so it goes through the same filter
    /// every other value does. Written raw, a name carrying a line break would
    /// open a record of its own.
    #[test]
    fn a_header_name_cannot_open_a_line_of_its_own() {
        let mut config = config("generate {\n  headers #true\n}");
        config.generate.headers.rules = vec![(
            "/*".to_owned(),
            vec![("X-A\nX-B".to_owned(), "v".to_owned())],
        )];
        let body = body(&config);
        assert!(body.contains("  X-AX-B: v\n"), "{body}");
    }

    /// The asset rule is only true where the names are content-addressed, and
    /// it has to precede the catch-all, which would otherwise claim those paths
    /// first.
    #[test]
    fn only_a_fingerprinted_build_declares_its_assets_immutable() {
        let hashed = body(&config(
            "generate { headers #true }\ncaching { }\nassets { fingerprint #true }",
        ));
        let assets = hashed.find("/assets/*").expect("no asset rule");
        let catchall = hashed.rfind("/*\n").expect("no catch-all");
        assert!(
            assets < catchall,
            "the catch-all precedes the asset rule: {hashed}"
        );
        assert!(hashed.contains("immutable"), "{hashed}");

        let plain = body(&config("generate { headers #true }\ncaching { }"));
        assert!(!plain.contains("/assets/*"), "{plain}");
        assert!(plain.contains("must-revalidate"), "{plain}");
    }
}
