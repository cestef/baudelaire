//! Redirect stubs: a minimal HTML page that forwards a stale URL to its new one.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use super::xml::Xml;
use super::{Emit, Processor, Site, Warn};
use crate::content::Strings;
use crate::error::Result;
use crate::error::warning::{RedirectCollision, RedirectsShadowed};
use crate::ui::Count;

/// Emits a redirect stub for every `redirect` old-path in a page's
/// frontmatter, forwarding it to that page's permalink.
pub(super) struct Redirects;

impl Processor for Redirects {
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let mut rules: Vec<(String, String)> = Vec::new();
        // A rule file the site publishes itself wins over a generated one, and
        // silently: `static/` is the escape hatch. Asking first is what keeps
        // that from meaning "no redirects at all", since choosing rules turns
        // the stubs off. Shadowed, the stubs come back and the build says so.
        let path = site.dist(&[Self::RULES]);
        let mut rules_wanted = site.config.generate.redirects;
        if rules_wanted && out.claimed(&path) {
            out.warn(RedirectsShadowed { path: path.clone() });
            rules_wanted = false;
        }
        // Two pages claiming one old path used to last-writer-win in silence,
        // and each write also pushed a duplicate path, so the summary counted a
        // file it had overwritten. Keep the first and say so, as the sibling
        // image-collision path does.
        let mut claimed: BTreeMap<PathBuf, &PathBuf> = BTreeMap::new();
        for page in site.pages {
            for old in &page.frontmatter.redirect {
                // Localized like the target it forwards to. Translating a page
                // by copying its frontmatter (the documented workflow) copies
                // the `redirect` list too, so unlocalized old paths made both
                // editions claim one output file and hard-failed the build.
                let old = site.config.localize(&page.lang, old);
                let destination = site.config.destination(&old);
                if let Some(kept) = claimed.get(&destination) {
                    out.warn(RedirectCollision {
                        old: old.clone(),
                        kept: (*kept).clone(),
                        dropped: page.source.clone(),
                    });
                    continue;
                }
                let target = site.config.prefixed(&page.permalink);
                // A rule file and a stub cannot coexist: both hosts that read
                // one serve a static file in preference to a redirect rule, so
                // the stub would win at the old path and the 301 would never
                // fire.
                if rules_wanted {
                    rules.push((site.config.prefixed(&old), target));
                } else {
                    let strings = Strings::new(site.config, &page.lang);
                    out.file(
                        &destination,
                        &Self::stub(&target, strings.get("redirecting"), &page.lang),
                    )?;
                }
                claimed.insert(destination, &page.source);
            }
        }
        if !rules.is_empty() {
            out.file(&path, &Self::rules(&rules))?;
        }
        if !claimed.is_empty() {
            out.note(format_args!("wrote {}", Count::redirects(claimed.len())));
        }
        Ok(())
    }
}

impl Redirects {
    /// The rule file Netlify and Cloudflare Pages read from the publish
    /// directory. One name, since both hosts spell it the same.
    const RULES: &'static str = "_redirects";

    /// The rule file's body: `<old> <new> 301` per line, in the order the pages
    /// claimed their old paths.
    ///
    /// A permanent redirect, because that is what these are: a page moved and
    /// the old URL is not coming back. The meta-refresh stub this replaces
    /// could only ever be a client-side round trip, which passes link equity
    /// worse than a real 301 and costs a page load to do it.
    fn rules(rules: &[(String, String)]) -> String {
        let mut body = String::new();
        for (old, new) in rules {
            let _ = writeln!(body, "{old} {new} 301");
        }
        body
    }

    /// A client-side redirect to `target`: a meta-refresh with a canonical link
    /// and a manual fallback anchor. Every value is attribute-escaped by the
    /// markup builder, so no `format!`-built HTML and no bespoke escaper.
    fn stub(target: &str, label: &str, lang: &str) -> String {
        let mut html = Xml::fragment();
        html.doctype("html");
        html.empty("meta", &[("charset", "utf-8")]);
        html.empty(
            "meta",
            &[("http-equiv", "content-language"), ("content", lang)],
        );
        html.empty(
            "meta",
            &[
                ("http-equiv", "refresh"),
                ("content", &format!("0; url={target}")),
            ],
        );
        html.empty("link", &[("rel", "canonical"), ("href", target)]);
        html.leaf("title", label);
        html.nest("a", &[("href", target)], |x| x.text(label));
        html.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Redirects;
    use crate::config::Config;
    use crate::content::{Data, Frontmatter, Page, PageId, Siblings};
    use crate::engine::emit::{Processor, Recorder, Site};
    use std::path::PathBuf;

    /// A page declaring `redirect: ("/old/",)` at `permalink`, sourced from
    /// `source`.
    fn page(source: &str, permalink: &str, redirect: &[&str]) -> Page {
        Page {
            id: PageId::new("posts", source),
            source: PathBuf::from(source),
            frontmatter: Frontmatter {
                redirect: redirect.iter().map(|s| (*s).to_owned()).collect(),
                ..Frontmatter::default()
            },
            body: String::new(),
            data: Data::Empty,
            collection: "posts".into(),
            permalink: permalink.into(),
            output: PathBuf::new(),
            template: None,
            lang: "en".into(),
            siblings: Siblings::default(),
            translations: Vec::new(),
        }
    }

    /// Two pages claiming one old path keep the first and warn, rather than
    /// silently overwriting each other and counting the file twice.
    #[test]
    fn a_duplicate_old_path_warns_and_keeps_the_first() {
        let config = Config::default();
        let pages = [
            page("content/a.typ", "/a/", &["/old/"]),
            page("content/b.typ", "/b/", &["/old/"]),
        ];
        let site = Site {
            config: &config,
            pages: &pages,
            outputs: &[],
        };

        let mut rec = Recorder::default();
        Redirects.run(&site, &mut rec).unwrap();

        assert_eq!(rec.files.len(), 1, "{:?}", rec.files);
        assert!(rec.files[0].1.contains("/a/"), "{:?}", rec.files[0].1);
        assert_eq!(rec.warns.len(), 1, "{:?}", rec.warns);
        assert!(rec.warns[0].contains("content/b.typ"), "{:?}", rec.warns);
        assert_eq!(rec.notes, ["wrote 1 redirect"]);
    }

    #[test]
    fn distinct_old_paths_each_get_a_stub() {
        let config = Config::default();
        let pages = [
            page("content/a.typ", "/a/", &["/old-a/"]),
            page("content/b.typ", "/b/", &["/old-b/"]),
        ];
        let site = Site {
            config: &config,
            pages: &pages,
            outputs: &[],
        };

        let mut rec = Recorder::default();
        Redirects.run(&site, &mut rec).unwrap();

        assert_eq!(rec.files.len(), 2, "{:?}", rec.files);
        assert!(rec.warns.is_empty(), "{:?}", rec.warns);
    }
}
