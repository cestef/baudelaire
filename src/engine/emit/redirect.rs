//! Redirect stubs: a minimal HTML page that forwards a stale URL to its new one.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::line::Lines;
use super::xml::Xml;
use super::{Emit, Processor, Site, WROTE, Warn};
use crate::content::Strings;
use crate::error::Result;
use crate::error::warning::{RedirectCollision, RedirectsShadowed};
use crate::ui::Count;

/// Emits a redirect stub for every `redirect` old-path in a page's
/// frontmatter, forwarding it to that page's permalink, and for every literal
/// pair the config declares.
pub(super) struct Redirects;

/// One declared redirect, whatever declared it: the two sources differ only in
/// where the old path and the target come from, so they are emitted by one
/// loop rather than two that would drift.
struct Rule<'a> {
    /// The old path, localized if a page declared it.
    old: String,
    /// Where it forwards to, base-path prefixed.
    target: String,
    /// The language whose strings the stub is written in.
    lang: &'a str,
    /// The page that declared it; absent for a config pair, which is exactly
    /// the case this exists for and so has no source file to name.
    source: Option<&'a PathBuf>,
}

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
        let mut claimed: BTreeMap<PathBuf, Option<&PathBuf>> = BTreeMap::new();
        for rule in Self::declared(site) {
            let destination = site.config.destination(&rule.old);
            if let Some(kept) = claimed.get(&destination) {
                // Reachable only if [`Claim::unique`] let two claims on one
                // output file through, which it does not. Kept as the last
                // word on which stub survives, and it names the page that lost
                // one whenever a page is what declared the loser.
                if let (Some(kept), Some(dropped)) = (kept, rule.source) {
                    out.warn(RedirectCollision {
                        old: rule.old.clone(),
                        kept: (*kept).clone(),
                        dropped: dropped.clone(),
                    });
                }
                continue;
            }
            // A rule file and a stub cannot coexist: both hosts that read one
            // serve a static file in preference to a redirect rule, so the stub
            // would win at the old path and the 301 would never fire.
            if rules_wanted {
                rules.push((site.config.prefixed(&rule.old), rule.target));
            } else {
                let strings = Strings::new(site.config, rule.lang);
                out.file(
                    &destination,
                    &Self::stub(&rule.target, strings.get("redirecting"), rule.lang),
                )?;
            }
            claimed.insert(destination, rule.source);
        }
        if !rules.is_empty() {
            out.file(&path, &Self::rules(&rules))?;
        }
        if !claimed.is_empty() {
            // A count rather than a path: this is the one processor that
            // writes a file per rule, so there is no single destination for
            // `Emit::wrote` to name.
            out.note(format_args!("{WROTE} {}", Count::redirects(claimed.len())));
        }
        Ok(())
    }
}

impl Redirects {
    /// Every redirect the site declares: one per frontmatter `redirect` entry,
    /// then the config's own pairs.
    ///
    /// Pages first, so a config pair can never take an old path out from under
    /// the page that declared it.
    fn declared<'a>(site: &'a Site<'a>) -> impl Iterator<Item = Rule<'a>> {
        let pages = site.pages.iter().flat_map(|page| {
            page.frontmatter.redirect.iter().map(|old| Rule {
                // Localized like the target it forwards to. Translating a page
                // by copying its frontmatter (the documented workflow) copies
                // the `redirect` list too, so unlocalized old paths made both
                // editions claim one output file and hard-failed the build.
                old: site.config.localize(&page.lang, old),
                target: site.config.prefixed(&page.permalink),
                lang: &page.lang,
                source: Some(&page.source),
            })
        });
        // A config pair is literal on both sides: nobody copied it per
        // language, and the path it claims is one the author read off an old
        // site, so localizing it would claim a path that never existed. The
        // target passes through `prefixed` all the same, since a site under a
        // subdirectory still has to forward within it, and an absolute URL to
        // another host is left alone by that.
        let config = site.config.redirect.iter().map(|(old, new)| Rule {
            old: old.clone(),
            target: site.config.prefixed(new),
            lang: &site.config.lang,
            source: None,
        });
        pages.chain(config)
    }

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
    ///
    /// Both paths are written as *fields*, not as text: the line is read by
    /// splitting on spaces, so a path carrying one would put the status where
    /// the host looks for the target and leave the rule pointing at `301`.
    fn rules(rules: &[(String, String)]) -> String {
        let mut body = Lines::default();
        for (old, new) in rules {
            body.line().word(old).lit(" ").word(new).lit(" 301");
        }
        body.finish()
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

    /// A `_redirects` line is three space-separated fields. A path carrying a
    /// space used to shift the target into the status column, leaving a rule
    /// that forwards to `301`.
    #[test]
    fn a_rule_keeps_each_path_to_one_field() {
        let rules = [("/old path/".to_owned(), "/new/".to_owned())];
        assert_eq!(Redirects::rules(&rules), "/oldpath/ /new/ 301\n");
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
