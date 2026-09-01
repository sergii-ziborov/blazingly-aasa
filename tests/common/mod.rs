//! Shared helpers for the integration tests.

#![allow(dead_code)]

use blazingly_aasa::{CompiledAasa, MatchDecision};

/// Loads a fixture from `tests/fixtures` and compiles it.
pub fn fixture(relative: &str) -> CompiledAasa {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    CompiledAasa::parse(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Asserts a decision, printing the full trace when it does not hold.
pub fn expect(aasa: &CompiledAasa, domain: &str, app_id: &str, url: &str, expected: MatchDecision) {
    let result = aasa
        .match_url(domain, app_id, url)
        .unwrap_or_else(|error| panic!("{url}: {error}"));
    assert_eq!(
        result.decision, expected,
        "\nURL: {url}\nexpected {expected}, got {}\n\n{result}",
        result.decision
    );
}
