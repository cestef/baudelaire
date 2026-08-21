//! Single-file export: the whole site as one HTML document, every page's body
//! a route in a JSON island that the bundled router swaps in place.

use std::path::PathBuf;

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::Serialize;

use super::script::Script;
use super::spa::ROUTER;
use super::xml::Xml;
use super::{Emit, Processor, Reads, Site, Warn};
use crate::config::{Config, Named, StandaloneConfig};
use crate::content::Page;
use crate::error::warning::{StandaloneEntryMissing, StandaloneLinked};
use crate::error::{Artifact, Result};
use crate::render::Fragments;

/// Emits the whole site as one self-contained HTML file.
pub(super) struct Standalone;

impl Processor for Standalone {
    fn name(&self) -> &'static str {
        "the single-file export"
    }

    fn claims(&self, config: &Config) -> Vec<PathBuf> {
        vec![Site::at(config, &[&config.navigation.standalone.file])]
    }

    fn inputs(&self, _config: &Config) -> Option<&'static [Reads]> {
        Some(&[Reads::Listing, Reads::Rendered])
    }

    fn enabled(&self, config: &Config) -> bool {
        config.navigation.standalone.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let cfg = &site.config.navigation.standalone;
        let entry = site.config.prefixed(cfg.entry());
        let routes = Routes::build(site);
        let Some(shell) = routes.entry(&entry) else {
            out.warn(StandaloneEntryMissing { entry });
            return Ok(());
        };
        if !site.config.html.embed {
            out.warn(StandaloneLinked {
                file: cfg.file.clone(),
            });
        }
        let shared = routes.shared();
        let head = shell.fragments.head.clone() + &shared.concat();
        let document = Document {
            lang: &shell.page.lang,
            dir: site.config.dir(&shell.page.lang),
            head: &head,
            body: &shell.fragments.body,
            routes: &routes.json(&shared)?,
            script: &cfg.script(&entry),
        }
        .render();
        let path = site.dist(&[&cfg.file]);
        out.file(&path, &document)?;
        out.wrote_with(
            &path,
            format_args!("{} routes, {} bytes", routes.len(), document.len()),
        );
        Ok(())
    }
}

/// The `id` of the JSON island the routes are parsed from, shared by the shell
/// and the generated script.
const ROUTES_ID: &str = "baudelaire-routes";

/// One page as the exported file carries it.
struct Route<'a> {
    page: &'a Page,
    fragments: &'a Fragments,
}

impl<'a> Route<'a> {
    /// This route as the router swaps it in: its body, preceded by any resource
    /// the shell does not already carry for the whole document.
    fn swap(&self, shared: &[&str]) -> Swap<'a> {
        let own: Vec<&str> = self
            .fragments
            .resources
            .iter()
            .map(String::as_str)
            .filter(|resource| !shared.contains(resource))
            .collect();
        Swap {
            title: self.page.title(),
            html: if own.is_empty() {
                Cow::Borrowed(self.fragments.body.as_str())
            } else {
                Cow::Owned(own.concat() + &self.fragments.body)
            },
        }
    }
}

/// What the router consumes for a route: everything a swap needs, and nothing
/// the shell already holds.
#[derive(Serialize)]
struct Swap<'a> {
    title: &'a str,
    html: Cow<'a, str>,
}

/// Every built page that can be a route, keyed by the URL its links spell.
struct Routes<'a>(BTreeMap<String, Route<'a>>);

impl<'a> Routes<'a> {
    /// Collect every page whose fragments were captured. A page without them
    /// cannot be a route, so it is skipped rather than emitted empty.
    fn build(site: &Site<'a>) -> Self {
        Self(
            site.outputs
                .iter()
                .filter_map(|output| {
                    let route = Route {
                        page: output.page,
                        fragments: output.fragments?,
                    };
                    Some((site.config.prefixed(&output.page.permalink), route))
                })
                .collect(),
        )
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    /// The route the shell is rendered from, or `None` when the configured
    /// entry names no built page.
    fn entry(&self, url: &str) -> Option<&Route<'a>> {
        self.0.get(url)
    }

    /// The resource elements every route carries, in document order.
    ///
    /// An element present on every route is an element of the document: the
    /// shell states it once and no route repeats it, which is also what keeps
    /// the bundle from running on every navigation.
    fn shared(&self) -> Vec<&str> {
        let mut routes = self.0.values();
        let Some(first) = routes.next() else {
            return Vec::new();
        };
        let mut shared: Vec<&str> = first
            .fragments
            .resources
            .iter()
            .map(String::as_str)
            .collect();
        for route in routes {
            shared.retain(|res| route.fragments.resources.iter().any(|own| own == res));
        }
        shared
    }

    /// The route table, safe to nest inside a `<script>`.
    fn json(&self, shared: &[&str]) -> Result<String> {
        let swaps: BTreeMap<&str, Swap> = self
            .0
            .iter()
            .map(|(url, route)| (url.as_str(), route.swap(shared)))
            .collect();
        let json = Artifact::Standalone.json(&swaps)?;
        Ok(crate::codegen::Island(&json).to_string())
    }
}

/// The exported document, assembled from the entry page's own markup plus the
/// two scripts that make it navigable.
struct Document<'a> {
    lang: &'a str,
    dir: Option<&'a str>,
    /// The entry page's head, plus the resources every route shares.
    head: &'a str,
    /// The entry page's body: the route shown before any navigation, and the
    /// only one that renders with JavaScript off.
    body: &'a str,
    routes: &'a str,
    script: &'a str,
}

impl Document<'_> {
    /// The router goes in as a *classic* script, since module scripts do not
    /// run on `file://`, which is where a single file is usually opened.
    fn render(&self) -> String {
        let mut attrs = vec![("lang", self.lang)];
        if let Some(dir) = self.dir {
            attrs.push(("dir", dir));
        }
        let mut html = Xml::fragment();
        html.doctype("html");
        html.nest("html", &attrs, |x| {
            x.nest("head", &[], |x| {
                x.raw(self.head);
                x.nest(
                    "script",
                    &[("type", "application/json"), ("id", ROUTES_ID)],
                    |x| x.raw(self.routes),
                );
                x.nest("script", &[], |x| x.raw(self.script));
            });
            x.nest("body", &[], |x| x.raw(self.body));
        });
        html.finish()
    }
}

impl StandaloneConfig {
    /// The permalink the shell is built from: the configured entry, else the
    /// site home.
    fn entry(&self) -> &str {
        self.entry.as_deref().unwrap_or("/")
    }

    /// The inlined router: the three constants it closes over, the shared core,
    /// and the bundle adapter. It mounts itself as it is evaluated, so nothing
    /// follows the parts.
    fn script(&self, entry: &str) -> String {
        Script::new(&[
            ("ROUTES_ID", ROUTES_ID),
            ("MODE", self.router.name()),
            ("ENTRY", entry),
        ])
        .part(ROUTER)
        .part(ADAPTER)
        .finish()
    }
}

/// The bundle adapter: route lookup against the JSON island.
const ADAPTER: &str = include_str!("js/standalone.js");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Router;
    use crate::content::{Data, Frontmatter, PageId};
    use crate::engine::emit::{Output, Recorder};
    use std::path::PathBuf;

    fn page(slug: &str, title: &str) -> Page {
        Page {
            id: PageId::new("pages", slug),
            source: PathBuf::from(format!("content/{slug}.typ")),
            frontmatter: Frontmatter {
                title: Some(title.to_owned()),
                ..Frontmatter::default()
            },
            body: String::new(),
            data: Data::Empty,
            collection: "pages".into(),
            permalink: if slug == "index" {
                "/".into()
            } else {
                format!("/{slug}/")
            },
            output: PathBuf::new(),
            template: None,
            lang: "en".into(),
        }
    }

    fn config() -> Config {
        let mut config = Config::default();
        config.navigation.standalone.enabled = true;
        config.html.embed = true;
        config
    }

    /// The stylesheet every fixture page carries, as a real build's pages do.
    const SITE_CSS: &str = r#"<link rel="stylesheet" href="data:text/css,body{}">"#;

    /// A built site: pages and the fragments the render pass captured for them.
    struct Built(Vec<(Page, Fragments)>);

    impl Built {
        fn of(pages: &[(&str, &str, &str)]) -> Self {
            Self(
                pages
                    .iter()
                    .map(|(slug, title, body)| {
                        let fragments = Fragments {
                            head: "<title>ignored</title>".into(),
                            resources: vec![SITE_CSS.to_owned()],
                            body: (*body).to_owned(),
                        };
                        (page(slug, title), fragments)
                    })
                    .collect(),
            )
        }

        /// Give one page a resource of its own, which no other route carries.
        fn plus_link(mut self, slug: &str, link: &str) -> Self {
            let found = self
                .0
                .iter_mut()
                .find(|(page, _)| page.id == PageId::new("pages", slug));
            found
                .expect("a page by that slug")
                .1
                .resources
                .push(link.into());
            self
        }

        fn outputs(&self) -> Vec<Output<'_>> {
            self.0
                .iter()
                .map(|(page, fragments)| Output {
                    fragments: Some(fragments),
                    ..Output::new(page, "")
                })
                .collect()
        }
    }

    /// Run the processor over a built site and hand back everything it emitted.
    fn export(config: &Config, built: &Built) -> Recorder {
        let outputs = built.outputs();
        let site = Site {
            entities: crate::content::Registries::none(),
            relations: crate::content::Relations::none(),
            history: crate::git::History::none(),
            config,
            pages: &[],
            outputs: &outputs,
        };
        let mut rec = Recorder::default();
        Standalone.run(&site, &mut rec).unwrap();
        rec
    }

    /// The exported file's route table: the text between the island's tags,
    /// exactly as the browser's JSON parser is handed it.
    fn island(html: &str) -> &str {
        let after = html.split_once(ROUTES_ID).expect("island present").1;
        let json = after.split_once('>').expect("island opens").1;
        json.split_once("</script>").expect("island closes").0
    }

    #[test]
    fn writes_one_file_carrying_every_route() {
        let built = Built::of(&[
            ("index", "Home", "<p>home</p>"),
            ("about", "About", "<p>about</p>"),
        ]);
        let rec = export(&config(), &built);

        assert_eq!(rec.files.len(), 1, "{:?}", rec.files);
        let (path, html) = &rec.files[0];
        assert!(path.ends_with("site.html"), "{path:?}");
        assert!(html.contains("<p>home</p>"), "entry rendered: {html}");
        assert!(html.contains(r#"const MODE = "hash";"#), "{html}");
        assert!(rec.warns.is_empty(), "{:?}", rec.warns);

        let routes: serde_json::Value = serde_json::from_str(island(html)).unwrap();
        assert_eq!(routes["/"]["title"], "Home");
        assert_eq!(routes["/about/"]["title"], "About");
        assert_eq!(routes["/about/"]["html"], "<p>about</p>");
    }

    #[test]
    fn links_every_route_shares_are_hoisted_into_the_shell() {
        const OWN: &str = r#"<link rel="stylesheet" href="data:text/css,p{}">"#;
        let built = Built::of(&[
            ("index", "Home", "<p>home</p>"),
            ("about", "About", "<p>about</p>"),
        ])
        .plus_link("about", OWN);
        let rec = export(&config(), &built);

        let (_, html) = &rec.files[0];
        assert_eq!(html.matches(SITE_CSS).count(), 1, "hoisted once: {html}");

        let routes: serde_json::Value = serde_json::from_str(island(html)).unwrap();
        assert_eq!(routes["/"]["html"], "<p>home</p>");
        assert_eq!(routes["/about/"]["html"], format!("{OWN}<p>about</p>"));
    }

    #[test]
    fn an_unknown_entry_warns_and_writes_nothing() {
        let mut config = config();
        config.navigation.standalone.entry = Some("/nope/".into());
        let rec = export(&config, &Built::of(&[("about", "About", "<p>about</p>")]));

        assert!(rec.files.is_empty(), "{:?}", rec.files);
        assert_eq!(rec.warns.len(), 1, "{:?}", rec.warns);
        assert!(rec.warns[0].contains("/nope/"), "{:?}", rec.warns);
    }

    #[test]
    fn warns_when_the_pages_only_link_their_assets() {
        let mut config = config();
        config.html.embed = false;
        let rec = export(&config, &Built::of(&[("index", "Home", "<p>home</p>")]));

        assert_eq!(rec.files.len(), 1);
        assert_eq!(rec.warns.len(), 1, "{:?}", rec.warns);
        assert!(rec.warns[0].contains("site.html"), "{:?}", rec.warns);
    }

    #[test]
    fn route_markup_cannot_close_the_json_island() {
        let built = Built::of(&[("index", "Home", "<p>a</p><script>1</script>")]);
        let rec = export(&config(), &built);

        let (_, html) = &rec.files[0];
        let raw = island(html);
        assert!(!raw.contains("</script"), "closes early: {raw}");
        let routes: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(routes["/"]["html"], "<p>a</p><script>1</script>");
    }

    #[test]
    fn a_comment_in_route_markup_cannot_reopen_the_island() {
        let built = Built::of(&[("index", "Home", "<!--<script-->ok")]);
        let rec = export(&config(), &built);

        let (_, html) = &rec.files[0];
        let raw = island(html);
        assert!(!raw.contains('<'), "markup escaped into the island: {raw}");
        let routes: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(routes["/"]["html"], "<!--<script-->ok");
    }

    #[test]
    fn the_inlined_router_never_closes_its_own_element() {
        let script = StandaloneConfig {
            router: Router::History,
            ..StandaloneConfig::default()
        }
        .script("/");
        assert!(!script.contains("</script"), "{script}");
        assert!(script.contains(r#"const MODE = "history";"#), "{script}");

        let hostile = StandaloneConfig {
            router: Router::History,
            ..StandaloneConfig::default()
        }
        .script("</script><script>pwned()</script>");
        assert!(!hostile.contains("</script"), "{hostile}");
    }
}
