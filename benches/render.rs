//! Internal-link resolution: one classify per `href`, each a `canonicalize`
//! against the page map. Blog nav and cross-refs hit the same targets often, so
//! the workload deliberately repeats hits.
//!
//! Runs on a [`Shape::Templated`] site, the shape a real site has. Link
//! classification is independent of the compile-input fork; the fixture picks a
//! shape only because every fixture must.

mod common;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use baudelaire::content::{Page, discover};
use baudelaire::render::LinkMap;

use common::Shape;

fn link_classify(c: &mut Criterion) {
    let (dir, cfg) = Shape::Templated.site(50);
    let pages: Vec<Page> = discover(&cfg, &common::project(&cfg))
        .unwrap()
        .into_iter()
        .flat_map(|col| col.pages)
        .collect();
    let map = LinkMap::new(&pages, dir.path(), cfg.sources());
    let from = pages[0].source.clone();
    // Many links, deliberately hitting the same targets repeatedly (nav/cross-refs).
    let links: Vec<String> = (0..200).map(|k| format!("p{}.typ", k % 50)).collect();
    c.bench_function("link_classify_200", |b| {
        b.iter(|| {
            for link in &links {
                black_box(map.classify(black_box(link), &from, None));
            }
        });
    });
}

criterion_group!(benches, link_classify);
criterion_main!(benches);
