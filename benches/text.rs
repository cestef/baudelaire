//! Plain-text extraction over page HTML — the per-page cost of search indexing.
//! Swept by document size so a regression reads as bytes/s, not a lone total.

mod common;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use baudelaire::engine::text::Text;

/// Section counts spanning a stub page to a long article.
const SECTIONS: [usize; 3] = [10, 100, 400];

fn text_extract(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_extract");
    for sections in SECTIONS {
        let html = common::html_doc(sections);
        group.throughput(Throughput::Bytes(html.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(html.len()), &html, |b, html| {
            b.iter(|| black_box(Text::extract(black_box(html))));
        });
    }
    group.finish();
}

criterion_group!(benches, text_extract);
criterion_main!(benches);
