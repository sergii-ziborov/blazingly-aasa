//! Runs `conformance/cases.json`.
//!
//! The same file is executed by `bindings/wasm/tests/conformance.mjs` through WebAssembly, and is
//! published so any other implementation can be held to it. Adding a case here is how a semantic
//! question gets settled once for every consumer instead of once per language.

use blazingly_aasa::{CompiledAasa, MatchDecision};
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    matching: Vec<MatchCase>,
    validation: Vec<ValidationCase>,
}

#[derive(Deserialize)]
struct MatchCase {
    name: String,
    feature: String,
    status: String,
    aasa: serde_json::Value,
    domain: String,
    #[serde(rename = "appId")]
    app_id: String,
    url: String,
    expect: String,
}

#[derive(Deserialize)]
struct ValidationCase {
    name: String,
    aasa: serde_json::Value,
    #[serde(rename = "expectCodes")]
    expect_codes: Vec<String>,
}

fn corpus() -> Corpus {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/cases.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&bytes).expect("the corpus should be valid JSON")
}

fn decision_name(decision: MatchDecision) -> &'static str {
    match decision {
        MatchDecision::Match => "match",
        MatchDecision::Exclude => "exclude",
        MatchDecision::NoMatch => "no_match",
    }
}

#[test]
fn every_matching_case_holds() {
    let corpus = corpus();
    assert_eq!(corpus.version, 2, "unexpected corpus version");

    let mut failures = Vec::new();
    for case in &corpus.matching {
        let bytes = serde_json::to_vec(&case.aasa).expect("case document should serialize");
        let compiled = match CompiledAasa::parse(&bytes) {
            Ok(compiled) => compiled,
            Err(error) => {
                failures.push(format!("{}: document failed to parse: {error}", case.name));
                continue;
            }
        };
        let result = match compiled.match_url(&case.domain, &case.app_id, &case.url) {
            Ok(result) => result,
            Err(error) => {
                failures.push(format!("{}: URL failed to parse: {error}", case.name));
                continue;
            }
        };
        let actual = decision_name(result.decision);
        if actual != case.expect {
            failures.push(format!(
                "{} [{}]\n  expected {}, got {}\n{}",
                case.name, case.feature, case.expect, actual, result
            ));
        }

        // The trace-free path must reach the same conclusion.
        let fast = compiled
            .decide(&case.domain, &case.app_id, &case.url)
            .expect("decide should accept a URL match_url accepted");
        assert_eq!(
            decision_name(fast),
            actual,
            "{}: decide disagreed with match_url",
            case.name
        );
    }

    assert!(
        failures.is_empty(),
        "{} of {} conformance cases failed:\n\n{}",
        failures.len(),
        corpus.matching.len(),
        failures.join("\n\n")
    );
}

#[test]
fn every_validation_case_holds() {
    let corpus = corpus();
    let mut failures = Vec::new();

    for case in &corpus.validation {
        let bytes = serde_json::to_vec(&case.aasa).expect("case document should serialize");
        let report = match CompiledAasa::parse(&bytes) {
            Ok(compiled) => compiled.validate(),
            Err(error) => {
                failures.push(format!("{}: document failed to parse: {error}", case.name));
                continue;
            }
        };
        let reported: Vec<&str> = report
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        for expected in &case.expect_codes {
            if !reported.contains(&expected.as_str()) {
                failures.push(format!(
                    "{}: expected {expected}, got {reported:?}",
                    case.name
                ));
            }
        }
        if case.expect_codes.is_empty() && !report.is_empty() {
            failures.push(format!(
                "{}: expected a silent report, got {reported:?}",
                case.name
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The corpus is a public artifact, so its own shape is worth asserting.
#[test]
fn the_corpus_is_well_formed() {
    let corpus = corpus();
    assert!(corpus.matching.len() >= 140, "corpus shrank unexpectedly");

    let mut names: Vec<&str> = corpus
        .matching
        .iter()
        .map(|case| case.name.as_str())
        .chain(corpus.validation.iter().map(|case| case.name.as_str()))
        .collect();
    let count = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), count, "duplicate case names");

    for case in &corpus.matching {
        assert!(
            matches!(case.status.as_str(), "documented" | "decided" | "oracle"),
            "{}: unknown status {}",
            case.name,
            case.status
        );
        assert!(
            matches!(case.expect.as_str(), "match" | "exclude" | "no_match"),
            "{}: unknown expectation {}",
            case.name,
            case.expect
        );
    }

    // Every feature named in the parity table should be exercised.
    let features: std::collections::BTreeSet<&str> = corpus
        .matching
        .iter()
        .map(|case| case.feature.as_str())
        .collect();
    for required in [
        "rule-order",
        "wildcards",
        "query",
        "percent-encoding",
        "substitutions",
        "defaults-hierarchy",
        "legacy-paths",
        "host",
        "path-slashes",
    ] {
        assert!(features.contains(required), "no case covers {required}");
    }
}
