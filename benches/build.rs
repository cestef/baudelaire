//! End-to-end build timings on one fixed-size site:
//! - `full_build/cold` — an empty cache, so every stage runs: typst compile,
//!   render, asset processing, search index, serialization. The headline number.
//! - `incremental_build` — a warm cache, so compile is skipped and the timing is
//!   dependency re-hashing, asset reprocessing, and search-index rebuild alone.

mod common;

use std::fs;
use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use baudelaire::engine::{Engine, Mode};
use baudelaire::ui::{Level, Ui};

/// Both build groups run on a site this size — big enough to be compile-bound,
/// small enough that a cold build stays a practical benchmark.
const PAGES: usize = 64;

fn full_build_cold(c: &mut Criterion) {
    let (_dir, cfg) = common::site(PAGES);
    let mut group = c.benchmark_group("full_build");
    group.sample_size(10);
    group.bench_function("cold_64", |b| {
        b.iter_batched(
            // Untimed: wipe the cache and prior output so the build is truly cold.
            || {
                let _ = fs::remove_dir_all(&cfg.cache.dir);
                let _ = fs::remove_dir_all(&cfg.dist);
                Ui::new(Level::Silent)
            },
            |ui| {
                let stats = Engine::new(cfg.clone(), Mode::Build)
                    .unwrap()
                    .build(&ui)
                    .unwrap();
                black_box(stats);
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn incremental_build(c: &mut Criterion) {
    let (_dir, cfg) = common::site(PAGES);
    // Warm the cache once so the timed rebuilds are all cache hits.
    Engine::new(cfg.clone(), Mode::Build)
        .unwrap()
        .build(&Ui::new(Level::Silent))
        .unwrap();

    let mut group = c.benchmark_group("incremental_build");
    group.sample_size(20);
    group.bench_function("rebuild_64_cached", |b| {
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

criterion_group!(benches, full_build_cold, incremental_build);
criterion_main!(benches);
