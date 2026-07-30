//! `_headers`: the `Cache-Control` policy, stated to the host that serves the
//! built files.

use super::{Emit, Processor, Site};
use crate::config::Config;
use crate::error::Result;

/// Emits a `_headers` rule file from the site's `caching` policy.
///
/// The same policy the S3 uploader sets per object, said once to a host that
/// reads it from the publish directory (Netlify, Cloudflare Pages). Both are
/// written from `caching { }` rather than each carrying its own, so a site that
/// deploys to a bucket *and* ships `_headers` cannot state two different
/// answers to one question.
pub(super) struct Headers;

impl Headers {
    /// The rule file name. Netlify and Cloudflare Pages spell it the same.
    const FILE: &'static str = "_headers";
}

impl Processor for Headers {
    /// Needs both halves: the file to write, and a policy to write into it.
    /// `generate { headers }` without `caching { }` would emit an empty rule
    /// file, which reads as "no policy" and is what the host already assumed.
    fn enabled(&self, config: &Config) -> bool {
        config.generate.headers && config.caching.enabled
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
        if config.assets.fingerprint {
            let prefix = config.prefixed(&format!("{}/*", config.asset_prefix()));
            body.push_str(&Self::rule(&prefix, &config.caching.immutable));
        }
        body.push_str(&Self::rule(&config.prefixed("/*"), &config.caching.default));
        out.file(&site.dist(&[Self::FILE]), &body)?;
        out.note(format_args!("wrote {}", Self::FILE));
        Ok(())
    }
}

impl Headers {
    /// One rule: the path pattern on its own line, then each header indented
    /// beneath it. The format both hosts read.
    fn rule(pattern: &str, value: &str) -> String {
        format!("{pattern}\n  Cache-Control: {value}\n\n")
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
