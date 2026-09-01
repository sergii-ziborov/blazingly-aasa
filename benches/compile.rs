//! Parsing and compiling an association file, against the `serde_json` + `regex` way of doing it.

#![allow(missing_docs)]

mod support;

use blazingly_aasa::{AasaDocument, CompiledAasa};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use support::{corpus, RegexAasa};

fn sizes() -> Vec<(&'static str, String)> {
    vec![
        ("1_detail_8_rules", corpus(1, 8)),
        ("8_details_16_rules", corpus(8, 16)),
        ("32_details_32_rules", corpus(32, 32)),
    ]
}

fn parse_and_compile(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("compile");
    for (name, json) in sizes() {
        let bytes = json.as_bytes();
        group.throughput(Throughput::Bytes(bytes.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("blazingly-aasa", name),
            &bytes,
            |bencher, bytes| {
                bencher.iter(|| black_box(CompiledAasa::parse(black_box(bytes)).unwrap()));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("serde_json+regex", name),
            &bytes,
            |bencher, bytes| bencher.iter(|| black_box(RegexAasa::parse(black_box(bytes)))),
        );
        // Parsing alone, to show how much of the cost is JSON and how much is compilation.
        group.bench_with_input(
            BenchmarkId::new("blazingly-aasa/parse-only", name),
            &bytes,
            |bencher, bytes| {
                bencher.iter(|| black_box(AasaDocument::parse(black_box(bytes)).unwrap()));
            },
        );
    }
    group.finish();
}

/// The JSON layer on its own.
///
/// Without this, the `compile` group is easy to misread: most of the gap there is the `regex`
/// compiler, not the JSON parser. This isolates how much of the difference the JSON engine
/// actually accounts for.
fn json_only(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("json-only");
    for (name, json) in sizes() {
        let bytes = json.as_bytes();
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("blazingly-json", name),
            &bytes,
            |bencher, bytes| {
                bencher.iter(|| {
                    black_box(
                        blazingly_json::from_slice::<blazingly_json::Value>(black_box(bytes))
                            .unwrap(),
                    )
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("serde_json", name),
            &bytes,
            |bencher, bytes| {
                bencher.iter(|| {
                    black_box(
                        serde_json::from_slice::<serde_json::Value>(black_box(bytes)).unwrap(),
                    )
                });
            },
        );
    }
    group.finish();
}

fn validating(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("validate");
    for (name, json) in sizes() {
        let compiled = CompiledAasa::parse(json.as_bytes()).unwrap();
        group.throughput(Throughput::Bytes(json.len() as u64));
        group.bench_function(BenchmarkId::new("blazingly-aasa", name), |bencher| {
            bencher.iter(|| black_box(compiled.validate()));
        });
    }
    group.finish();
}

criterion_group!(benches, parse_and_compile, json_only, validating);
criterion_main!(benches);
