//! Content-assembly hotpaths, both swept by page count with throughput in pages:
//! - `discover` — the content walk plus per-page frontmatter module evaluation.
//! - `plan`     — everything `discover` does, then the full page-set assembly
//!   (siblings, taxonomy, pagination, permalink-collision check). The gap
//!   between the two curves is the pure assembly cost.

mod common;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use baudelaire::content::{discover, plan};

use common::PAGE_COUNTS;

/// One live site per page count, kept alive for the whole run.
fn sites() -> Vec<(tempfile::TempDir, baudelaire::config::Config)> {
    PAGE_COUNTS.into_iter().map(common::site).collect()
}

fn discover_pages(c: &mut Criterion) {
    let sites = sites();
    let mut group = c.benchmark_group("discover");
    for (n, (dir, cfg)) in PAGE_COUNTS.into_iter().zip(&sites) {
        let project = common::project(cfg);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &dir, |b, _| {
            b.iter(|| black_box(discover(cfg, &project).unwrap()));
        });
    }
    group.finish();
}

fn plan_pages(c: &mut Criterion) {
    let sites = sites();
    let mut group = c.benchmark_group("plan");
    for (n, (dir, cfg)) in PAGE_COUNTS.into_iter().zip(&sites) {
        let project = common::project(cfg);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &dir, |b, _| {
            b.iter(|| black_box(plan(cfg, &project).unwrap()));
        });
    }
    group.finish();
}

criterion_group!(benches, discover_pages, plan_pages);
criterion_main!(benches);
