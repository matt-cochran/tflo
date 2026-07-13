//! Performance benchmarks for tflow.

#![allow(clippy::arithmetic_side_effects)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use tflo_core::prelude::*;

#[derive(Clone)]
struct Tick {
    ts: i64,
    price: f64,
}

fn generate_ticks(count: usize) -> Vec<Tick> {
    (0..count)
        .map(|i| Tick {
            ts: (i as i64) * 100,
            price: 100.0 + (i as f64 * 0.01).sin(),
        })
        .collect()
}

fn bench_map_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("map_f64");

    for size in [1000, 10_000, 100_000] {
        let ticks = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements(size as u64));
        let _ = group.bench_with_input(
            BenchmarkId::new("stateless_unary", size),
            &ticks,
            |b, ticks| {
                b.iter(|| {
                    let result: Vec<f64> = ticks
                        .iter()
                        .cloned()
                        .tflo(|t| {
                            let _ = t.timestamp(|x| x.ts);
                            let price = t.prop(|x| x.price);
                            price.map_f64(|x| x * 2.0 + 1.0)
                        })
                        .collect();
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

fn bench_scan_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("scan_f64");

    for size in [1000, 10_000, 100_000] {
        let ticks = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements(size as u64));
        let _ = group.bench_with_input(
            BenchmarkId::new("stateful_scan", size),
            &ticks,
            |b, ticks| {
                b.iter(|| {
                    let result: Vec<f64> = ticks
                        .iter()
                        .cloned()
                        .tflo(|t| {
                            let _ = t.timestamp(|x| x.ts);
                            let price = t.prop(|x| x.price);
                            price.scan_f64(
                                || 0.0_f64,
                                |s, x| {
                                    *s = 0.9 * *s + 0.1 * x;
                                    *s
                                },
                            )
                        })
                        .collect();
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

fn bench_map2_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("map2_f64");

    for size in [1000, 10_000, 100_000] {
        let ticks = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements(size as u64));
        let _ = group.bench_with_input(
            BenchmarkId::new("binary_transform", size),
            &ticks,
            |b, ticks| {
                b.iter(|| {
                    let result: Vec<f64> = ticks
                        .iter()
                        .cloned()
                        .tflo(|t| {
                            let _ = t.timestamp(|x| x.ts);
                            let price = t.prop(|x| x.price);
                            let scaled = price.map_f64(|x| x * 2.0);
                            price.map2_f64(&scaled, |a, b| a + b)
                        })
                        .collect();
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

fn bench_merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge");

    for size in [1000, 10_000] {
        let stream1 = generate_ticks(size);
        let stream2 = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements((size * 2) as u64));
        let _ = group.bench_with_input(
            BenchmarkId::new("two_streams", size),
            &(stream1.clone(), stream2.clone()),
            |b, (s1, s2)| {
                b.iter(|| {
                    let result: Vec<Tick> = merge_by_timestamp(
                        vec![s1.clone().into_iter(), s2.clone().into_iter()],
                        |t| t.ts,
                    )
                    .collect();
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_map_f64,
    bench_scan_f64,
    bench_map2_f64,
    bench_merge
);
criterion_main!(benches);
