//! The wildcard engine on its own: this crate's bitset NFA against the usual `regex` translation.
//!
//! The interesting column is `adversarial`. `*a*a*a...*b` is the classic backtracking bomb; a
//! backtracking engine explodes on it, while both a DFA-based regex and this crate's bitset NFA
//! stay linear. The point of the comparison is not that regex is slow — it is that you pay a
//! regex compiler and a regex-sized dependency for wildcards this simple.

#![allow(missing_docs)]

mod support;

use blazingly_aasa::WildcardPattern;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use support::to_regex;

fn cases() -> Vec<(&'static str, String, String)> {
    vec![
        (
            "literal",
            "/help/website/faq".to_owned(),
            "/help/website/faq".to_owned(),
        ),
        (
            "prefix",
            "/buy/*".to_owned(),
            format!("/buy/{}", "x".repeat(64)),
        ),
        (
            "suffix",
            "*/checkout".to_owned(),
            format!("/{}/checkout", "x".repeat(64)),
        ),
        ("single_char", "/id/????".to_owned(), "/id/4815".to_owned()),
        (
            "mixed",
            "/a/*/b/?*/c".to_owned(),
            "/a/one/b/two/c".to_owned(),
        ),
        (
            "adversarial",
            format!("{}*b", "*a".repeat(16)),
            "a".repeat(512),
        ),
    ]
}

fn matching(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("pattern/match");
    for (name, pattern, input) in cases() {
        let ours = WildcardPattern::compile(&pattern, true).unwrap();
        let theirs = to_regex(&pattern, true);

        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("blazingly-aasa", name),
            &input,
            |bencher, input| bencher.iter(|| black_box(ours.matches(black_box(input)))),
        );
        group.bench_with_input(BenchmarkId::new("regex", name), &input, |bencher, input| {
            bencher.iter(|| black_box(theirs.is_match(black_box(input))));
        });
    }
    group.finish();
}

fn compiling(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("pattern/compile");
    for (name, pattern, _) in cases() {
        group.bench_with_input(
            BenchmarkId::new("blazingly-aasa", name),
            &pattern,
            |bencher, pattern| {
                bencher.iter(|| {
                    black_box(WildcardPattern::compile(black_box(pattern), true).unwrap())
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("regex", name),
            &pattern,
            |bencher, pattern| {
                bencher.iter(|| black_box(to_regex(black_box(pattern), true)));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, matching, compiling);
criterion_main!(benches);
