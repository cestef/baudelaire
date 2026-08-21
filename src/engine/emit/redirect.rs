//! Redirect stubs: a minimal HTML page forwarding a stale URL to its new one.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::Config;

use super::line::Lines;
use super::xml::Xml;
use super::{Emit, Processor, Reads, Site, WROTE, Warn};
use crate::content::Strings;
use crate::error::Result;
use crate::error::warning::{RedirectCollision, RedirectsShadowed};
use crate::ui::Count;

/// Emits a redirect stub for every `redirect` old-path in a page's
/// frontmatter, forwarding it to that page's permalink, and for every literal
/// pair the config declares.
pub(super) struct Redirects;

/// One declared redirect, whatever declared it.
struct Rule<'a> {
    /// The old path, localized if a page declared it.
    old: String,
    /// Where it forwards to, base-path prefixed.
    target: String,
    /// The language whose strings the stub is written in.
    lang: &'a str,
    /// The page that declared it; `None` for a config pair.
    source: Option<&'a PathBuf>,
    /// What the rule file tells the host to answer with; permanent unless a
    /// config line says otherwise.
    status: u16,
}

impl Processor for Redirects {
    fn name(&self) -> &'static str {
        "the redirect rules"
    }

    /// The rule file alone: a stub is written at a path a page's own
    /// `redirect` names, and those are claimed by the page that names them.
    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        config
            .redirects
            .file
            .then(|| Site::at(config, &[Self::RULES]))
            .into_iter()
            .collect()
    }

    /// Never: whether a rule lands in the rules file is decided by the run, not
    /// by the config.
    ///
    /// `redirects { file }` is only the request. A static `_redirects` shadows
    /// it, and then each rule is written as a stub of its own at a destination
    /// the page set decides, which [`Processor::claims`] names none of. A skip
    /// would keep the rules file the static copy already provides and let the
    /// sweep take every stub.
    fn inputs(&self, _config: &Config) -> Option<&'static [Reads]> {
        None
    }

    /// A rule file and a stub cannot coexist: a host serves a static file in
    /// preference to a redirect rule, so the stub would win at the old path and
    /// the rule would never fire.
    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let mut rules: Vec<(String, String, u16)> = Vec::new();
        let path = site.dist(&[Self::RULES]);
        let mut rules_wanted = site.config.redirects.file;
        if rules_wanted && out.claimed(&path) {
            out.warn(RedirectsShadowed { path: path.clone() });
            rules_wanted = false;
        }
        let mut claimed: BTreeMap<PathBuf, Option<&PathBuf>> = BTreeMap::new();
        let mut patterns = 0;
        for rule in Self::declared(site) {
            if crate::config::Config::wildcard(&rule.old) {
                if rules_wanted {
                    rules.push((site.config.prefixed(&rule.old), rule.target, rule.status));
                    patterns += 1;
                }
                continue;
            }
            let destination = site.config.destination(&rule.old);
            if let Some(kept) = claimed.get(&destination) {
                if let (Some(kept), Some(dropped)) = (kept, rule.source) {
                    out.warn(RedirectCollision {
                        old: rule.old.clone(),
                        kept: (*kept).clone(),
                        dropped: dropped.clone(),
                    });
                }
                continue;
            }
            if rules_wanted {
                rules.push((site.config.prefixed(&rule.old), rule.target, rule.status));
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
        let declared = claimed.len() + patterns;
        if declared > 0 {
            out.note(format_args!("{WROTE} {}", Count::redirects(declared)));
        }
        Ok(())
    }
}

impl Redirects {
    /// Every redirect the site declares: one per frontmatter `redirect` entry,
    /// then the config's own pairs.
    ///
    /// Pages first, so a config pair can never take an old path out from under
    /// the page that declared it. A page's old path is localized like the
    /// target it forwards to; a config pair is literal on both sides, since the
    /// path it claims is one the author read off an old site.
    fn declared<'a>(site: &'a Site<'a>) -> impl Iterator<Item = Rule<'a>> {
        let pages = site.pages.iter().flat_map(|page| {
            page.frontmatter.redirect.iter().map(|old| Rule {
                old: site.config.localize(&page.lang, old),
                target: site.config.prefixed(&page.permalink),
                lang: &page.lang,
                source: Some(&page.source),
                status: crate::config::RedirectConfig::PERMANENT,
            })
        });
        let config = site.config.redirects.rules.iter().map(|(old, rule)| Rule {
            old: old.clone(),
            target: site.config.prefixed(&rule.target),
            lang: &site.config.lang,
            source: None,
            status: rule.status,
        });
        pages.chain(config)
    }

    /// The rule file Netlify and Cloudflare Pages read from the publish
    /// directory.
    const RULES: &'static str = "_redirects";

    /// The rule file's body: `<old> <new> <status>` per line, in the order the
    /// pages claimed their old paths.
    ///
    /// Both paths are written as *fields*, not as text: the line is read by
    /// splitting on spaces, so a path carrying one would shift every field
    /// after it.
    fn rules(rules: &[(String, String, u16)]) -> String {
        let mut body = Lines::default();
        for (old, new, status) in rules {
            body.line()
                .word(old)
                .lit(" ")
                .word(new)
                .lit(" ")
                .word(status.to_string());
        }
        body.finish()
    }

    /// A client-side redirect to `target`: a meta-refresh with a canonical link
    /// and a manual fallback anchor.
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
    use crate::content::{Data, Frontmatter, Page, PageId};
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
        }
    }

    #[test]
    fn a_duplicate_old_path_warns_and_keeps_the_first() {
        let config = Config::default();
        let pages = [
            page("content/a.typ", "/a/", &["/old/"]),
            page("content/b.typ", "/b/", &["/old/"]),
        ];
        let site = Site {
            entities: crate::content::Registries::none(),
            relations: crate::content::Relations::none(),
            history: crate::git::History::none(),
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
    fn a_rule_keeps_each_path_to_one_field() {
        let rules = [("/old path/".to_owned(), "/new/".to_owned(), 301)];
        assert_eq!(Redirects::rules(&rules), "/oldpath/ /new/ 301\n");
    }

    #[test]
    fn a_rule_writes_the_status_it_carries() {
        let rules = [
            ("/moved/".to_owned(), "/new/".to_owned(), 301),
            ("/temp/".to_owned(), "/elsewhere/".to_owned(), 302),
        ];
        assert_eq!(
            Redirects::rules(&rules),
            "/moved/ /new/ 301\n/temp/ /elsewhere/ 302\n"
        );
    }

    #[test]
    fn distinct_old_paths_each_get_a_stub() {
        let config = Config::default();
        let pages = [
            page("content/a.typ", "/a/", &["/old-a/"]),
            page("content/b.typ", "/b/", &["/old-b/"]),
        ];
        let site = Site {
            entities: crate::content::Registries::none(),
            relations: crate::content::Relations::none(),
            history: crate::git::History::none(),
            config: &config,
            pages: &pages,
            outputs: &[],
        };

        let mut rec = Recorder::default();
        Redirects.run(&site, &mut rec).unwrap();

        assert_eq!(rec.files.len(), 2, "{:?}", rec.files);
        assert!(rec.warns.is_empty(), "{:?}", rec.warns);
    }

    #[test]
    fn a_wildcard_old_path_is_written_as_a_rule() {
        let config = Config::parse(
            "redirects {\n  file #true\n  rules {\n    \"/latest/*\" \"/:splat\"\n  }\n}\n",
        )
        .expect("should parse");
        let site = Site {
            entities: crate::content::Registries::none(),
            relations: crate::content::Relations::none(),
            history: crate::git::History::none(),
            config: &config,
            pages: &[],
            outputs: &[],
        };

        let mut rec = Recorder::default();
        Redirects.run(&site, &mut rec).unwrap();

        assert_eq!(rec.files.len(), 1, "{:?}", rec.files);
        assert!(rec.files[0].0.ends_with("_redirects"), "{:?}", rec.files[0]);
        assert_eq!(rec.files[0].1, "/latest/* /:splat 301\n");
        assert_eq!(rec.notes, ["wrote 1 redirect"]);
    }

    #[test]
    fn a_wildcard_writes_no_stub() {
        let config =
            Config::parse("redirects {\n  rules {\n    \"/latest/*\" \"/:splat\"\n  }\n}\n")
                .expect("should parse");
        let site = Site {
            entities: crate::content::Registries::none(),
            relations: crate::content::Relations::none(),
            history: crate::git::History::none(),
            config: &config,
            pages: &[],
            outputs: &[],
        };

        let mut rec = Recorder::default();
        Redirects.run(&site, &mut rec).unwrap();

        assert!(rec.files.is_empty(), "{:?}", rec.files);
        assert!(rec.notes.is_empty(), "{:?}", rec.notes);
    }
}
