//! The published conformance protocol must keep working.
//!
//! `conformance/PROTOCOL.md` invites other implementations to score themselves against the corpus.
//! That invitation is only worth making if the contract does not quietly drift, so the runner, the
//! protocol document, and the reference adapters are checked here rather than trusted.

use std::path::Path;

fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_protocol_document_describes_the_fields_the_runner_sends() {
    let protocol = read("conformance/PROTOCOL.md");
    let runner = read("conformance/run.mjs");

    // Whatever the runner writes, the contract has to name.
    for field in ["id", "aasa", "domain", "appId", "url"] {
        assert!(runner.contains(field), "the runner should send `{field}`");
        assert!(
            protocol.contains(field),
            "PROTOCOL.md must document `{field}`"
        );
    }
    for decision in ["match", "exclude", "no_match"] {
        assert!(
            protocol.contains(decision),
            "PROTOCOL.md must list `{decision}` as a valid answer"
        );
    }
}

#[test]
fn the_reference_adapters_implement_the_protocol() {
    for adapter in [
        "conformance/adapters/wasm.mjs",
        "conformance/adapters/cli.py",
    ] {
        let source = read(adapter);
        assert!(
            source.contains("id") && source.contains("decision"),
            "{adapter} must answer with an id and a decision"
        );
        assert!(
            source.contains("stdin"),
            "{adapter} must read cases from stdin"
        );
    }
}

/// The trivial-pass column is the whole reason a score from this runner can be read at face value.
#[test]
fn the_runner_separates_real_passes_from_accidental_ones() {
    let runner = read("conformance/run.mjs");
    assert!(
        runner.contains("trivial"),
        "the runner must report how many passes were cases expecting no_match"
    );
    assert!(
        runner.contains("expect no_match"),
        "the report must say what makes a pass trivial"
    );
}

/// A failing implementation should be scored on the rest, not abandoned.
#[test]
fn one_bad_answer_does_not_abort_the_run() {
    let runner = read("conformance/run.mjs");
    assert!(
        runner.contains("<no answer>"),
        "a missing answer must count as a failed case rather than crash the runner"
    );
}
