//! blake3 content hashing — the cache's invalidation primitive, run over every
//! source and processed asset each build. Swept by payload size (throughput in
//! bytes) so per-byte hashing cost is visible independent of file count.

mod common;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use baudelaire::graph::Hash;

/// Payload sizes: a small source file, a page of HTML, a bundled asset.
const SIZES: [usize; 3] = [1 << 10, 64 << 10, 1 << 20];

fn hash_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash/of_bytes");
    for size in SIZES {
        // Non-uniform bytes so nothing short-circuits on a trivial pattern.
        let bytes: Vec<u8> = (0..size).map(|i| (i * 31 + 7) as u8).collect();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &bytes, |b, bytes| {
            b.iter(|| black_box(Hash::of_bytes(black_box(bytes))));
        });
    }
    group.finish();
}

criterion_group!(benches, hash_bytes);
criterion_main!(benches);
