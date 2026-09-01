//! End-to-end matching throughput on a compiled document.

#![allow(missing_docs)]

mod support;

use blazingly_aasa::{CompiledAasa, UrlParts};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use support::{corpus, urls, RegexAasa};

const APP: &str = "ABCDE12345.com.example.app4";

fn per_url(criterion: &mut Criterion) {
    let json = corpus(8, 16);
    let ours = CompiledAasa::parse(json.as_bytes()).unwrap();
    let theirs = RegexAasa::parse(json.as_bytes());
    let urls = urls();
    let parsed: Vec<UrlParts> = urls
        .iter()
        .map(|url| UrlParts::parse(url).unwrap())
        .collect();

    let mut group = criterion.benchmark_group("match/batch");
    group.throughput(Throughput::Elements(urls.len() as u64));

    group.bench_function("blazingly-aasa/decide", |bencher| {
        bencher.iter(|| {
            for url in &urls {
                black_box(ours.decide("example.com", APP, black_box(url)).unwrap());
            }
        });
    });
    group.bench_function("blazingly-aasa/decide_parts", |bencher| {
        bencher.iter(|| {
            for parts in &parsed {
                black_box(ours.decide_parts("example.com", APP, black_box(parts)));
            }
        });
    });
    group.bench_function("blazingly-aasa/match_url+trace", |bencher| {
        bencher.iter(|| {
            for url in &urls {
                black_box(ours.match_url("example.com", APP, black_box(url)).unwrap());
            }
        });
    });
    group.bench_function("serde_json+regex/decide", |bencher| {
        bencher.iter(|| {
            for parts in &parsed {
                black_box(theirs.decide(APP, black_box(parts)));
            }
        });
    });
    group.finish();
}

fn compiled_versus_one_shot(criterion: &mut Criterion) {
    let json = corpus(8, 16);
    let bytes = json.as_bytes();
    let ours = CompiledAasa::parse(bytes).unwrap();
    let url = "https://example.com/help2/topic?articleNumber=4815";

    let mut group = criterion.benchmark_group("match/single");
    group.throughput(Throughput::Elements(1));
    group.bench_function("reuse compiled handle", |bencher| {
        bencher.iter(|| black_box(ours.decide("example.com", APP, black_box(url)).unwrap()));
    });
    group.bench_function("reparse every call", |bencher| {
        bencher.iter(|| {
            black_box(
                blazingly_aasa::match_url(black_box(bytes), "example.com", APP, black_box(url))
                    .unwrap(),
            );
        });
    });
    group.finish();
}

fn scaling(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("match/scaling");
    for details in [1usize, 8, 32] {
        let json = corpus(details, 16);
        let ours = CompiledAasa::parse(json.as_bytes()).unwrap();
        let theirs = RegexAasa::parse(json.as_bytes());
        let parts = UrlParts::parse("https://example.com/nothing/here").unwrap();
        let app = format!("ABCDE12345.com.example.app{}", details - 1);

        group.bench_with_input(
            BenchmarkId::new("blazingly-aasa", details),
            &details,
            |bencher, _| bencher.iter(|| black_box(ours.decide_parts("example.com", &app, &parts))),
        );
        group.bench_with_input(
            BenchmarkId::new("serde_json+regex", details),
            &details,
            |bencher, _| bencher.iter(|| black_box(theirs.decide(&app, &parts))),
        );
    }
    group.finish();
}

criterion_group!(benches, per_url, compiled_versus_one_shot, scaling);
criterion_main!(benches);
