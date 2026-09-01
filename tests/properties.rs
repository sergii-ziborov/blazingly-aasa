//! Property tests: invariants that must hold for inputs nobody thought to write down.
//!
//! The most valuable one here checks the bitset matcher against a deliberately naive recursive
//! reference implementation. The production engine is fast and non-backtracking; the reference is
//! obviously correct and exponential. On small inputs they must agree exactly.
#![allow(clippy::needless_raw_string_hashes)] // JSON fixtures read better with one delimiter

use blazingly_aasa::{AasaDocument, CompiledAasa};
use proptest::prelude::*;

/// The obvious, exponential way to match Apple's wildcards. Correct by inspection.
fn reference_match(pattern: &[char], input: &[char]) -> bool {
    match pattern.first() {
        None => input.is_empty(),
        Some('*') => (0..=input.len()).any(|skip| reference_match(&pattern[1..], &input[skip..])),
        Some('?') => !input.is_empty() && reference_match(&pattern[1..], &input[1..]),
        Some(literal) => {
            !input.is_empty() && input[0] == *literal && reference_match(&pattern[1..], &input[1..])
        }
    }
}

/// Expands `$(v)` into every alternative, producing a set of plain glob patterns.
///
/// Substitution values may themselves contain `?` and `*` but may not nest, so one round of
/// expansion is exact. Matching the original is then "does any expansion match", which is a
/// reference the bitset NFA can be checked against.
fn expand(pattern: &str, values: &[&str]) -> Vec<String> {
    let Some((before, rest)) = pattern.split_once("$(v)") else {
        return vec![pattern.to_owned()];
    };
    let mut out = Vec::new();
    for tail in expand(rest, values) {
        for value in values {
            out.push(format!("{before}{value}{tail}"));
        }
    }
    out
}

fn matches_via_substitution(pattern: &str, values: &[&str], path: &str) -> bool {
    let list = values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        r#"{{"applinks":{{"substitutionVariables":{{"v":[{list}]}},
        "details":[{{"appID":"A.b","components":[{{"/":"/{pattern}"}}]}}]}}}}"#
    );
    let aasa = CompiledAasa::parse(json.as_bytes()).expect("generated document should parse");
    aasa.match_url("e.test", "A.b", &format!("https://e.test/{path}"))
        .expect("generated URL should parse")
        .is_match()
}

fn matches_via_crate(pattern: &str, path: &str) -> bool {
    let json = format!(
        r#"{{"applinks":{{"details":[{{"appID":"A.b","components":[{{"/":"/{pattern}"}}]}}]}}}}"#
    );
    let aasa = CompiledAasa::parse(json.as_bytes()).expect("generated document should parse");
    aasa.match_url("e.test", "A.b", &format!("https://e.test/{path}"))
        .expect("generated URL should parse")
        .is_match()
}

/// 400 cases by default; `PROPTEST_CASES=20000 cargo test --release --test properties` for a deep
/// run. A hard-coded `with_cases` silently ignores the environment variable, which makes a
/// "deep run" that proves nothing -- easy to do by accident, so it is spelled out here.
fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(400);
    ProptestConfig {
        cases,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// The fast engine and the naive engine must never disagree.
    #[test]
    fn matcher_agrees_with_the_naive_reference(
        pattern in "[ab?*]{0,10}",
        input in "[ab]{0,10}",
    ) {
        let expected = reference_match(
            &format!("/{pattern}").chars().collect::<Vec<_>>(),
            &format!("/{input}").chars().collect::<Vec<_>>(),
        );
        prop_assert_eq!(matches_via_crate(&pattern, &input), expected,
            "pattern `/{}` against `/{}`", pattern, input);
    }

    /// The bitset NFA -- the engine used whenever a pattern carries a multi-character substitution
    /// set -- must agree with expanding the alternatives by hand and matching each one.
    #[test]
    fn substitution_engine_agrees_with_expansion(
        head in "[ab]{0,3}",
        tail in "[ab*?]{0,3}",
        values in proptest::collection::vec("[ab*?]{0,3}", 1..4),
        input in "[ab]{0,8}",
    ) {
        let pattern = format!("{head}$(v){tail}");
        let values: Vec<&str> = values.iter().map(String::as_str).collect();

        let expected = expand(&format!("/{pattern}"), &values).iter().any(|concrete| {
            reference_match(
                &concrete.chars().collect::<Vec<_>>(),
                &format!("/{input}").chars().collect::<Vec<_>>(),
            )
        });
        prop_assert_eq!(
            matches_via_substitution(&pattern, &values, &input),
            expected,
            "pattern `/{}` with {:?} against `/{}`", pattern, values, input
        );
    }

    /// A pattern with no wildcards matches exactly one string: itself.
    #[test]
    fn a_literal_pattern_matches_only_itself(literal in "[ab]{0,10}", other in "[ab]{0,10}") {
        prop_assert_eq!(matches_via_crate(&literal, &literal), true);
        if literal != other {
            prop_assert_eq!(matches_via_crate(&literal, &other), false);
        }
    }

    /// Parsing arbitrary bytes may fail, but must never panic.
    #[test]
    fn parsing_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        if let Ok(document) = AasaDocument::parse(&bytes) {
            let compiled = document.compile();
            let _ = compiled.validate();
            let _ = compiled.to_normalized_json();
        }
    }

    /// Parsing arbitrary JSON-shaped text must never panic either.
    #[test]
    fn parsing_arbitrary_json_text_never_panics(text in r#"\{("[a-z]{1,6}"\s*:\s*(\{\}|\[\]|"[a-z*/?]{0,8}"|true|null|3)\s*,?){0,6}\}"#) {
        if let Ok(document) = AasaDocument::parse_str(&text) {
            let _ = document.compile().validate();
        }
    }

    /// Matching arbitrary URL text must never panic.
    #[test]
    fn matching_arbitrary_urls_never_panics(url in ".{0,80}") {
        let aasa = CompiledAasa::parse(
            br#"{"applinks":{"details":[{"appID":"A.b","components":[{"/":"/a/*","?":{"q":"?*"}}]}]}}"#,
        ).unwrap();
        let _ = aasa.match_url("e.test", "A.b", &url);
    }

    /// A document is always semantically equal to itself, and the diff is empty.
    #[test]
    fn diff_of_a_document_with_itself_is_empty(
        pattern in "[a-z/*?]{0,12}",
        exclude in any::<bool>(),
        case_sensitive in any::<bool>(),
    ) {
        let json = format!(
            r#"{{"applinks":{{"details":[{{"appID":"A.b","components":[
                {{"/":"/{pattern}","exclude":{exclude},"caseSensitive":{case_sensitive}}}
            ]}}]}}}}"#
        );
        let aasa = CompiledAasa::parse(json.as_bytes()).unwrap();
        let diff = aasa.semantic_diff(&aasa);
        prop_assert!(diff.is_equivalent(), "{}", diff);
        prop_assert!(aasa.structural_equal(&aasa));
    }

    /// Semantic equality is symmetric.
    #[test]
    fn semantic_equality_is_symmetric(left in "[a-z/*]{0,8}", right in "[a-z/*]{0,8}") {
        let build = |pattern: &str| {
            let json = format!(
                r#"{{"applinks":{{"details":[{{"appID":"A.b","components":[{{"/":"/{pattern}"}}]}}]}}}}"#
            );
            CompiledAasa::parse(json.as_bytes()).unwrap()
        };
        let a = build(&left);
        let b = build(&right);
        prop_assert_eq!(a.semantic_equal(&b), b.semantic_equal(&a));
    }

    /// The trace-free fast path must never disagree with the trace-building one.
    #[test]
    fn decide_and_match_url_always_agree(
        pattern in "[ab/*?]{0,10}",
        path in "[ab/]{0,10}",
        exclude in any::<bool>(),
        case_sensitive in any::<bool>(),
        percent_encoded in any::<bool>(),
    ) {
        let json = format!(
            r##"{{"applinks":{{"details":[{{"appID":"A.b","components":[
                {{"/":"/{pattern}","exclude":{exclude},"caseSensitive":{case_sensitive},
                 "percentEncoded":{percent_encoded}}},
                {{"/":"/fallback/*"}}
            ]}}]}}}}"##
        );
        let aasa = CompiledAasa::parse(json.as_bytes()).unwrap();
        let url = format!("https://e.test/{path}?q=1&q=2#frag");
        let traced = aasa.match_url("e.test", "A.b", &url).unwrap();
        let fast = aasa.decide("e.test", "A.b", &url).unwrap();
        prop_assert_eq!(fast, traced.decision, "\n{}", traced);
    }

    /// The reverse query must never disagree with the per-app decision.
    #[test]
    fn apps_for_url_agrees_with_decide(
        pattern in "[ab/*?]{0,8}",
        path in "[ab/]{0,8}",
        exclude in any::<bool>(),
    ) {
        let json = format!(
            r#"{{"applinks":{{"details":[
                {{"appIDs":["T.a","T.b"],"components":[{{"/":"/{pattern}","exclude":{exclude}}}]}},
                {{"appIDs":["T.b","T.c"],"components":[{{"/":"/x/*"}}]}}
            ]}}}}"#
        );
        let aasa = CompiledAasa::parse(json.as_bytes()).unwrap();
        let url = format!("https://e.test/{path}");
        let listed = aasa.apps_for_url("e.test", &url).unwrap();

        for app in ["T.a", "T.b", "T.c", "T.absent"] {
            let direct = aasa.decide("e.test", app, &url).unwrap();
            let reverse = listed
                .iter()
                .find(|(candidate, _)| candidate == app)
                .map_or(blazingly_aasa::MatchDecision::NoMatch, |(_, d)| *d);
            prop_assert_eq!(direct, reverse, "{} at {}", app, url);
        }
    }

    /// Compiling is deterministic: the same bytes always produce the same normalized form.
    #[test]
    fn compilation_is_deterministic(pattern in "[a-z/*?]{0,12}") {
        let json = format!(
            r#"{{"applinks":{{"details":[{{"appID":"A.b","components":[{{"/":"/{pattern}"}}]}}]}}}}"#
        );
        let first = CompiledAasa::parse(json.as_bytes()).unwrap();
        let second = CompiledAasa::parse(json.as_bytes()).unwrap();
        prop_assert_eq!(first.to_normalized_json(), second.to_normalized_json());
        prop_assert_eq!(first.validate(), second.validate());
    }
}

#[test]
fn a_pathological_pattern_finishes_promptly() {
    // The classic regex-killer. With a backtracking engine this takes exponential time; the
    // bitset engine is linear in positions.
    let pattern = "*a".repeat(24) + "*b";
    let json = format!(
        r#"{{"applinks":{{"details":[{{"appID":"A.b","components":[{{"/":"{pattern}"}}]}}]}}}}"#
    );
    let aasa = CompiledAasa::parse(json.as_bytes()).unwrap();
    let url = format!("https://e.test/{}", "a".repeat(2048));

    let started = std::time::Instant::now();
    let result = aasa.match_url("e.test", "A.b", &url).unwrap();
    let elapsed = started.elapsed();

    assert!(!result.is_match());
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "pathological pattern took {elapsed:?}"
    );
}
