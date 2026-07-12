//! Criterion benchmarks for the build's per-item hotpaths.
//!
//! Three groups, each isolating a scaling cost the profilers flagged:
//! - `text_extract`  — plain-text extraction over page HTML (search indexing).
//! - `link_classify` — internal-link resolution (one `canonicalize` per link).
//! - `incremental_build` — a warm rebuild: compile is skipped, so cache
//!   dependency re-hashing, asset reprocessing, and search-index rebuild are
//!   what the timing actually measures.

use std::fs;
use std::hint::black_box;
use std::path::Path;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use tempfile::TempDir;

use baudelaire::config::Config;
use baudelaire::content::{Page, discover};
use baudelaire::engine::text::Text;
use baudelaire::engine::{Engine, Mode};
use baudelaire::render::LinkMap;
use baudelaire::ui::{Level, Ui};

/// A synthetic site under `dir`: `n` cross-linked posts with tags plus a small
/// asset set, returning a config whose paths are rebased into `dir`.
fn mksite(dir: &Path, n: usize) -> Config {
    let kdl = "site \"Bench\"\n\
        url \"https://bench.example\"\n\
        paths { content \"content\"; dist \"public\"; assets \"assets\"; templates \"templates\" }\n\
        clean #true\n\
        taxonomies { tags }\n\
        output {\n\
          assets { minify #true; fingerprint #true; bundle #true }\n\
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
            "#frontmatter((title: \"Page {i}\", date: datetime(year: 2024, month: 1, day: {}), tags: (\"t{}\", \"t{}\",)))\n\
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
    cfg.content = dir.join(&cfg.content);
    cfg.dist = dir.join(&cfg.dist);
    cfg.assets = dir.join(&cfg.assets);
    cfg.cache.dir = dir.join(&cfg.cache.dir);
    cfg
}

/// A page-sized HTML document with chrome, prose, entities, and a raw element.
fn html_doc() -> String {
    let mut s = String::from(
        "<html><head><style>.x{color:red}</style></head><body>\
         <nav>Home About Contact</nav><main>",
    );
    for i in 0..200 {
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

fn bench_text_extract(c: &mut Criterion) {
    let html = html_doc();
    c.bench_function("text_extract", |b| {
        b.iter(|| black_box(Text::extract(black_box(&html))));
    });
}

fn bench_link_classify(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let cfg = mksite(dir.path(), 50);
    let pages: Vec<Page> = discover(&cfg)
        .unwrap()
        .into_iter()
        .flat_map(|col| col.pages)
        .collect();
    let map = LinkMap::new(&pages);
    let from = pages[0].source.clone();
    // Many links, deliberately hitting the same targets repeatedly (nav/cross-refs).
    let links: Vec<String> = (0..200).map(|k| format!("p{}.typ", k % 50)).collect();
    c.bench_function("link_classify_200", |b| {
        b.iter(|| {
            for link in &links {
                black_box(map.classify(black_box(link), &from));
            }
        });
    });
}

fn bench_incremental_build(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let cfg = mksite(dir.path(), 100);
    // Warm the cache once so the timed rebuilds are all cache hits.
    Engine::new(cfg.clone(), Mode::Build)
        .unwrap()
        .build(&Ui::new(Level::Silent))
        .unwrap();

    let mut group = c.benchmark_group("incremental_build");
    group.sample_size(20);
    group.bench_function("rebuild_100_cached", |b| {
        b.iter_batched(
            || Ui::new(Level::Silent),
            |ui| {
                let stats = Engine::new(cfg.clone(), Mode::Build)
                    .unwrap()
                    .build(&ui)
                    .unwrap();
                black_box(stats);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_text_extract,
    bench_link_classify,
    bench_incremental_build
);
criterion_main!(benches);
