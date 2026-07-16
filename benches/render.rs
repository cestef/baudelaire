//! Internal-link resolution: one classify per `href`, each a `canonicalize`
//! against the page map. Blog nav and cross-refs hit the same targets often, so
//! the workload deliberately repeats hits.

mod common;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use baudelaire::content::{Page, discover};
use baudelaire::render::LinkMap;

fn link_classify(c: &mut Criterion) {
    let (dir, cfg) = common::site(50);
    let pages: Vec<Page> = discover(&cfg, &common::project(&cfg))
        .unwrap()
        .into_iter()
        .flat_map(|col| col.pages)
        .collect();
    let map = LinkMap::new(&pages, dir.path());
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

criterion_group!(benches, link_classify);
criterion_main!(benches);
