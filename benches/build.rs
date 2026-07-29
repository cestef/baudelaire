//! End-to-end build timings on one fixed-size site, each group reported once
//! per [`common::Shape`] so the two compile-input branches never hide behind
//! one number:
//! - `templated`: pages bound to `templates/layout.typ`, which is what every
//!   authored page on a real site is. The build pays for wrapper generation and
//!   compiles one shared template into every page.
//! - `bare`: pages with no template, the early return in `Prepare::input`. Only
//!   generated listing pages take it in practice, so read it as the floor, not
//!   as the headline.
//!
//! The groups themselves:
//! - `full_build`: an empty cache, so every stage runs: typst compile, render,
//!   asset processing, search index, serialization. The headline number.
//! - `incremental_build`: a warm cache, so compile is skipped and the timing is
//!   dependency re-hashing, asset reprocessing, and search-index rebuild alone.

mod common;

use std::fs;
use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

use baudelaire::engine::{Engine, Mode};
use baudelaire::ui::{Level, Ui};

use common::Shape;

/// Both build groups run on a site this size, big enough to be compile-bound,
/// small enough that a cold build stays a practical benchmark.
const PAGES: usize = 64;

fn full_build_cold(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_build");
    group.sample_size(10);
    for shape in Shape::ALL {
        let (_dir, cfg) = shape.site(PAGES);
        group.bench_function(BenchmarkId::new(shape.label(), PAGES), |b| {
            b.iter_batched(
                // Untimed: wipe the cache and prior output so the build is truly cold.
                || {
                    let _ = fs::remove_dir_all(&cfg.cache.dir);
                    let _ = fs::remove_dir_all(&cfg.paths.dist);
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
    }
    group.finish();
}

fn incremental_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_build");
    group.sample_size(20);
    for shape in Shape::ALL {
        let (_dir, cfg) = shape.site(PAGES);
        // Warm the cache once so the timed rebuilds are all cache hits.
        Engine::new(cfg.clone(), Mode::Build)
            .unwrap()
            .build(&Ui::new(Level::Silent))
            .unwrap();
        group.bench_function(BenchmarkId::new(shape.label(), PAGES), |b| {
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
    }
    group.finish();
}

criterion_group!(benches, full_build_cold, incremental_build);
criterion_main!(benches);
