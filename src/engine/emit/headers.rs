//! `_headers`: what the host serving the built files is told about them, the
//! `Cache-Control` policy and the `Content-Security-Policy` both.

use super::csp::{Digests, Policy};
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
    /// host already assumed.
    fn enabled(&self, config: &Config) -> bool {
        config.generate.headers && (config.caching.enabled || config.security.csp.enabled)
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let config = site.config;
        let mut body = String::new();
        // Most specific first: a host matches rules in order, so the catch-all
        // has to come last or it would claim the asset paths too.
        //
        // The asset rule only earns its place when this build content-addresses
        // the names under that prefix. Without `assets { fingerprint }` an asset
        // keeps its authored name across builds and is exactly as mutable as a
        // page, which is the same call `CacheControl::header` makes per object.
        if config.caching.enabled && config.assets.fingerprint {
            let prefix = config.prefixed(&format!("{}/*", config.asset_prefix()));
            body.push_str(&Self::rule(
                &prefix,
                &[("Cache-Control", &config.caching.immutable)],
            ));
        }
        body.push_str(&Self::rule(&config.prefixed("/*"), &Self::catchall(site)));
        out.file(&site.dist(&[Self::FILE]), &body)?;
        out.note(format_args!("wrote {}", Self::FILE));
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
    /// beneath it. The format both hosts read.
    fn rule(pattern: &str, headers: &[(&'static str, impl AsRef<str>)]) -> String {
        let mut rule = format!("{pattern}\n");
        for (name, value) in headers {
            rule.push_str(&format!("  {name}: {}\n", value.as_ref()));
        }
        rule.push('\n');
        rule
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
