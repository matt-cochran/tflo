//! Performance benchmarks for tflow.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
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

fn bench_sma(c: &mut Criterion) {
    let mut group = c.benchmark_group("sma");

    for size in [1000, 10_000, 100_000] {
        let ticks = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements(size as u64));
        let _ = group.bench_with_input(BenchmarkId::new("time_based", size), &ticks, |b, ticks| {
            b.iter(|| {
                let result: Vec<f64> = ticks
                    .iter()
                    .cloned()
                    .tflo(|t| {
                        let _ = t.timestamp(|x| x.ts);
                        let price = t.prop(|x| x.price);
                        price.sma(1_u64.secs())
                    })
                    .collect();
                black_box(result)
            });
        });

        let _ =
            group.bench_with_input(BenchmarkId::new("count_based", size), &ticks, |b, ticks| {
                b.iter(|| {
                    let result: Vec<f64> = ticks
                        .iter()
                        .cloned()
                        .tflo(|t| {
                            let _ = t.timestamp(|x| x.ts);
                            let price = t.prop(|x| x.price);
                            price.sma(20usize)
                        })
                        .collect();
                    black_box(result)
                });
            });
    }

    group.finish();
}

fn bench_ema(c: &mut Criterion) {
    let mut group = c.benchmark_group("ema");

    for size in [1000, 10_000, 100_000] {
        let ticks = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements(size as u64));
        let _ = group.bench_with_input(BenchmarkId::new("time_based", size), &ticks, |b, ticks| {
            b.iter(|| {
                let result: Vec<f64> = ticks
                    .iter()
                    .cloned()
                    .tflo(|t| {
                        let _ = t.timestamp(|x| x.ts);
                        let price = t.prop(|x| x.price);
                        price.ema(500_u64.ms())
                    })
                    .collect();
                black_box(result)
            });
        });
    }

    group.finish();
}

fn bench_cross_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross");

    for size in [1000, 10_000, 100_000] {
        let ticks = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements(size as u64));
        let _ =
            group.bench_with_input(BenchmarkId::new("cross_above", size), &ticks, |b, ticks| {
                b.iter(|| {
                    let result: Vec<ThresholdCrossEventMode> = ticks
                        .iter()
                        .cloned()
                        .tflo(|t| {
                            let _ = t.timestamp(|x| x.ts);
                            let price = t.prop(|x| x.price);
                            let sma_fast = price.sma(100_u64.ms());
                            let sma_slow = price.sma(500_u64.ms());
                            sma_fast.cross_builder().above(&sma_slow)
                        })
                        .collect();
                    black_box(result)
                });
            });
    }

    group.finish();
}

fn bench_complex_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("complex");

    for size in [1000, 10_000, 100_000] {
        let ticks = generate_ticks(size);

        let _ = group.throughput(Throughput::Elements(size as u64));
        let _ = group.bench_with_input(
            BenchmarkId::new("full_pipeline", size),
            &ticks,
            |b, ticks| {
                b.iter(|| {
                    let result: Vec<(f64, f64, f64, ThresholdCrossEventMode)> = ticks
                        .iter()
                        .cloned()
                        .tflo(|t| {
                            let _ = t.timestamp(|x| x.ts);
                            let price = t.prop(|x| x.price);

                            let sma_fast = price.sma(100_u64.ms());
                            let sma_slow = price.sma(500_u64.ms());
                            let std = price.std(500_u64.ms());
                            let zscore = (&price - &sma_fast) / &std;
                            let cross = sma_fast.clone().cross_builder().above(&sma_slow);

                            (sma_fast, sma_slow, zscore, cross)
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
    bench_sma,
    bench_ema,
    bench_cross_detection,
    bench_complex_pipeline,
    bench_merge
);
criterion_main!(benches);
