//! Shared fixtures for the hotpath benchmarks.
//!
//! Every bench target compiles this module in, so some helpers are unused from
//! any single target's point of view, hence the crate-level `dead_code` allow.
#![allow(dead_code)]

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use baudelaire::config::Config;
use baudelaire::engine::Mode;
use baudelaire::world::Project;

/// Page counts every scaling group sweeps over, so `discover`, `plan`, and the
/// build groups all report on the same axis.
pub const PAGE_COUNTS: [usize; 3] = [16, 64, 256];

/// A [`Project`] for a test config: module evaluation needs the real world.
pub fn project(cfg: &Config) -> Project {
    Project::new(cfg, Mode::Build).expect("project")
}

/// A synthetic site under `dir`: `n` cross-linked posts with tags plus a small
/// asset set, returning a config whose paths are rebased into `dir`.
pub fn mksite(dir: &Path, n: usize) -> Config {
    let kdl = "site \"Bench\"\n\
        url \"https://bench.example\"\n\
        paths { content \"content\"; dist \"public\"; assets \"assets\"; templates \"templates\" }\n\
        prune #true\n\
        content { taxonomies { tags } }\n\
        assets { minify #true; fingerprint #true; bundle #true }\n\
        generate {\n\
          feed { formats rss atom }\n\
          search { formats json }\n\
        }\n";
    fs::write(dir.join("config.kdl"), kdl).unwrap();
    fs::create_dir_all(dir.join("content/posts")).unwrap();
    fs::write(dir.join("content/_shared.typ"), "#let badge(x) = [*#x*]\n").unwrap();
    for i in 0..n {
        // Sibling links so resolution actually hits the filesystem (and the map).
        let links: String = (0..4)
            .map(|k| {
                let j = (i * 7 + k * 13) % n;
                format!("#link(\"p{j}.typ\")[see {j}]\n")
            })
            .collect();
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

    let mut cfg = Config::parse(kdl).unwrap();
    cfg.paths.content = dir.join(&cfg.paths.content);
    cfg.paths.dist = dir.join(&cfg.paths.dist);
    cfg.paths.assets = dir.join(&cfg.paths.assets);
    cfg.cache.dir = dir.join(&cfg.cache.dir);
    cfg
}

/// A live `n`-page site in a fresh temp dir. The [`TempDir`] is returned so the
/// caller keeps it alive for the whole benchmark; dropping it deletes the site.
pub fn site(n: usize) -> (TempDir, Config) {
    let dir = TempDir::new().unwrap();
    let cfg = mksite(dir.path(), n);
    (dir, cfg)
}

/// A page-sized HTML document with `sections` repeats of chrome, prose, entities,
/// and a raw element, the shape [`baudelaire::engine::text::Text::extract`] sees.
pub fn html_doc(sections: usize) -> String {
    let mut s = String::from(
        "<html><head><style>.x{color:red}</style></head><body>\
         <nav>Home About Contact</nav><main>",
    );
    for i in 0..sections {
        s.push_str(&format!(
            "<h2>Section {i}</h2><p>Lorem ipsum &amp; dolor &lt;sit&gt; amet, \
             consectetur &#39;adipiscing&#39; elit. Sed do eiusmod tempor incididunt.</p>"
        ));
    }
    s.push_str(
        "<script>console.log('ignored')</script></main><footer>copyright</footer></body></html>",
    );
    s
}
