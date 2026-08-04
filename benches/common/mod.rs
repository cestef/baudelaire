//! Shared fixtures for the hotpath benchmarks.
//!
//! Every bench target compiles this module in, so some helpers are unused from
//! any single target's point of view, hence the crate-level `dead_code` allow.
//!
//! A fixture site is always built for one [`Shape`], because the compile input
//! a page gets depends on whether it has a template and the two branches cost
//! very different amounts. A bench that does not say which shape it measures is
//! measuring whichever one the fixture happened to produce.
#![allow(dead_code)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use baudelaire::config::Config;
use baudelaire::engine::Mode;
use baudelaire::world::Project;

/// Page counts every scaling group sweeps over, so `discover`, `plan`, and the
/// build groups all report on the same axis.
pub const PAGE_COUNTS: [usize; 3] = [16, 64, 256];

/// A [`Project`] for a test config: module evaluation needs the real world,
/// theme included: a package theme is served through it.
pub fn project(cfg: &Config) -> Project {
    let theme = baudelaire::theme::Theme::of(cfg).expect("theme");
    Project::new(cfg, Mode::Build, theme.as_ref()).expect("project")
}

/// Which branch of `Prepare::input` a fixture site's pages take.
///
/// The fork is on `Page::template`, resolved as frontmatter-else-collection
/// default. Both branches are live, so both are worth a number:
///
/// - [`Shape::Templated`] is what every authored page on a real site takes. The
///   page compiles through a generated wrapper (template import, frontmatter
///   binding, nav, translations, strings); the wrapper text is what the cache
///   fingerprints, and the body reaches typst through `#include`, so a body
///   edit invalidates as a dependency-file hash instead.
/// - [`Shape::Bare`] is the early return: no wrapper, the body is compiled and
///   fingerprinted directly. Generated listing pages take it (`Listing::new`
///   leaves `template: None` and renders the default body), so it is not dead
///   code, just not what authored pages do.
///
/// The two fixtures are identical apart from the collection's `template=`
/// binding, so a difference between their numbers is the fork and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Pages bound to `templates/layout.typ` through the collection.
    Templated,
    /// Pages with no template at all.
    Bare,
}

impl Shape {
    /// Both shapes, for a group that reports one curve per shape.
    pub const ALL: [Self; 2] = [Self::Templated, Self::Bare];

    /// The site's one layout, written for both shapes so the fixtures differ
    /// only in whether the collection binds it.
    ///
    /// A template file exports a function named after its stem and receives
    /// `(page, body)`. It reads the injected wrapper values (frontmatter, nav)
    /// rather than ignoring them, so the templated numbers include what a real
    /// site's chrome costs. It must not emit html/head/body: typst-html owns
    /// those, and a template that emitted them would drop the generated head.
    const LAYOUT: &'static str = r#"#import "@baudelaire/html:0.1.0": h

#let layout(page, body) = {
  set document(title: page.frontmatter.title)
  h("header", h("a", href: "/", "Bench"))
  h("main", h("article", {
    h("h1", page.frontmatter.title)
    body
  }))
  h("nav", {
    if page.nav.prev != none { h("a", href: page.nav.prev.url, page.nav.prev.title) }
    if page.nav.next != none { h("a", href: page.nav.next.url, page.nav.next.title) }
  })
}
"#;

    /// The shape's name in a benchmark id.
    pub fn label(self) -> &'static str {
        match self {
            Self::Templated => "templated",
            Self::Bare => "bare",
        }
    }

    /// The collection attribute that binds (or does not bind) the layout: the
    /// single difference between the two fixtures.
    fn binding(self) -> &'static str {
        match self {
            Self::Templated => " template=\"layout.typ\"",
            Self::Bare => "",
        }
    }

    /// A synthetic site under `dir`: `n` cross-linked posts with tags plus a
    /// small asset set, returning a config whose paths are rebased into `dir`.
    pub fn mksite(self, dir: &Path, n: usize) -> Config {
        let kdl = format!(
            r#"site "Bench"
url "https://bench.example"
paths {{ content "content"; dist "public"; assets "assets"; templates "templates" }}
prune #true
content {{ taxonomies {{ tags }}; collections {{ posts{binding} }} }}
assets {{ minify #true; fingerprint #true; bundle #true }}
generate {{
  feed {{ formats rss atom }}
  search {{ formats json }}
}}
"#,
            binding = self.binding(),
        );
        fs::write(dir.join("config.kdl"), &kdl).unwrap();
        fs::create_dir_all(dir.join("content/posts")).unwrap();
        fs::write(dir.join("content/_shared.typ"), "#let badge(x) = [*#x*]\n").unwrap();
        // Written for both shapes so the fixtures differ only in the binding.
        fs::create_dir_all(dir.join("templates")).unwrap();
        fs::write(dir.join("templates/layout.typ"), Self::LAYOUT).unwrap();
        for i in 0..n {
            // Sibling links so resolution actually hits the filesystem (and the map).
            let mut links = String::new();
            for k in 0..4 {
                let j = (i * 7 + k * 13) % n;
                writeln!(links, "#link(\"p{j}.typ\")[see {j}]").unwrap();
            }
            let body = format!(
                "#let frontmatter = (title: \"Page {i}\", date: datetime(year: 2024, month: 1, day: {}), tags: (\"t{}\", \"t{}\",))\n\
                 #import \"/content/_shared.typ\": badge\n\
                 #badge(\"hi {i}\")\n\
                 Lorem ipsum dolor sit amet {i}, consectetur adipiscing elit.\n{links}",
                (i % 28) + 1,
                i % 40,
                (i + 1) % 40,
            );
            fs::write(dir.join(format!("content/posts/p{i}.typ")), body).unwrap();
        }
        fs::create_dir_all(dir.join("assets")).unwrap();
        fs::write(
            dir.join("assets/style.css"),
            "body{background:url(bg.png);color:red}\n.x{margin:0}\n",
        )
        .unwrap();
        fs::write(dir.join("assets/bg.png"), [0x89u8, 0x50, 0x4e, 0x47]).unwrap();
        fs::write(
            dir.join("assets/main.js"),
            "export const x = 1;\nconsole.log(x);\n",
        )
        .unwrap();

        let mut cfg = Config::parse(&kdl).unwrap();
        // The project root is what typst virtualizes every source path against
        // and what a template import is expressed relative to, so it moves into
        // the temp dir with everything else.
        cfg.root = dir.to_path_buf();
        cfg.paths.content = dir.join(&cfg.paths.content);
        cfg.paths.dist = dir.join(&cfg.paths.dist);
        cfg.paths.assets = dir.join(&cfg.paths.assets);
        cfg.paths.templates = dir.join(&cfg.paths.templates);
        cfg.cache.dir = dir.join(&cfg.cache.dir);
        cfg
    }

    /// A live `n`-page site of this shape in a fresh temp dir. The [`TempDir`]
    /// is returned so the caller keeps it alive for the whole benchmark;
    /// dropping it deletes the site.
    pub fn site(self, n: usize) -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let cfg = self.mksite(dir.path(), n);
        (dir, cfg)
    }
}

/// A page-sized HTML document with `sections` repeats of chrome, prose, entities,
/// and a raw element, the shape [`baudelaire::engine::text::Text::extract`] sees.
pub fn html_doc(sections: usize) -> String {
    let mut s = String::from(
        "<html><head><style>.x{color:red}</style></head><body>\
         <nav>Home About Contact</nav><main>",
    );
    for i in 0..sections {
        write!(
            s,
            "<h2>Section {i}</h2><p>Lorem ipsum &amp; dolor &lt;sit&gt; amet, \
             consectetur &#39;adipiscing&#39; elit. Sed do eiusmod tempor incididunt.</p>"
        )
        .unwrap();
    }
    s.push_str(
        "<script>console.log('ignored')</script></main><footer>copyright</footer></body></html>",
    );
    s
}
