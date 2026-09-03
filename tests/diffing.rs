//! Semantic diff: what changed in behaviour, not in bytes.
#![allow(clippy::needless_raw_string_hashes)] // JSON fixtures read better with one delimiter

use blazingly_aasa::{CompiledAasa, SemanticChange, Service};

fn compile(json: &str) -> CompiledAasa {
    CompiledAasa::parse(json.as_bytes()).expect("fixture should parse")
}

const BASE: &str = r##"{"applinks":{"details":[{
    "appIDs": ["ABCDE12345.com.example.app"],
    "components": [
        { "/": "/help/website/*", "exclude": true },
        { "/": "/help/*" },
        { "/": "/buy/*" }
    ]
}]}}"##;

#[test]
fn a_document_equals_itself() {
    let aasa = compile(BASE);
    let diff = aasa.semantic_diff(&aasa);
    assert!(diff.is_equivalent(), "{diff}");
    assert!(diff.changes().is_empty());
}

#[test]
fn whitespace_and_key_order_are_not_changes() {
    let compact = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/x","exclude":false}]}]}}"#,
    );
    let reordered = compile(
        r#"{
            "applinks": {
                "details": [ { "components": [ { "exclude": false, "/": "/x" } ], "appIDs": [ "A.b" ] } ]
            }
        }"#,
    );
    assert!(compact.semantic_equal(&reordered));
}

#[test]
fn moving_a_default_up_a_level_is_not_a_change() {
    let inline = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[
            {"/":"/a/*","caseSensitive":false},
            {"/":"/b/*","caseSensitive":false}
        ]}]}}"#,
    );
    let hoisted = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],"defaults":{"caseSensitive":false},
            "components":[{"/":"/a/*"},{"/":"/b/*"}]}]}}"#,
    );
    let domain_level = compile(
        r#"{"applinks":{"defaults":{"caseSensitive":false},"details":[{"appIDs":["A.b"],
            "components":[{"/":"/a/*"},{"/":"/b/*"}]}]}}"#,
    );

    assert!(
        inline.semantic_equal(&hoisted),
        "{}",
        inline.semantic_diff(&hoisted)
    );
    assert!(hoisted.semantic_equal(&domain_level));
    // ...but the files are not textually the same.
    assert!(!inline.structural_equal(&hoisted));
}

#[test]
fn changing_a_setting_is_reported_as_one_change_not_two() {
    let before =
        compile(r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/help/*"}]}]}}"#);
    let after = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/help/*","caseSensitive":false}]}]}}"#,
    );

    let diff = before.semantic_diff(&after);
    assert_eq!(diff.changes().len(), 1, "{diff}");
    match &diff.changes()[0] {
        SemanticChange::RuleChanged {
            app_id,
            left,
            right,
            ..
        } => {
            assert_eq!(app_id, "A.b");
            assert!(left.case_sensitive);
            assert!(!right.case_sensitive);
        }
        other => panic!("expected RuleChanged, got {other}"),
    }
}

#[test]
fn reordering_rules_is_a_change_because_the_first_match_wins() {
    let swapped = compile(
        r##"{"applinks":{"details":[{
        "appIDs": ["ABCDE12345.com.example.app"],
        "components": [
            { "/": "/help/*" },
            { "/": "/help/website/*", "exclude": true },
            { "/": "/buy/*" }
        ]
    }]}}"##,
    );

    let diff = compile(BASE).semantic_diff(&swapped);
    assert!(!diff.is_equivalent());
    assert!(
        diff.changes()
            .iter()
            .any(|change| matches!(change, SemanticChange::RuleMoved { .. })),
        "a reorder should read as a move:\n{diff}"
    );
}

#[test]
fn inserting_a_rule_does_not_report_every_later_rule_as_changed() {
    let extended = compile(
        r##"{"applinks":{"details":[{
        "appIDs": ["ABCDE12345.com.example.app"],
        "components": [
            { "/": "/new/*" },
            { "/": "/help/website/*", "exclude": true },
            { "/": "/help/*" },
            { "/": "/buy/*" }
        ]
    }]}}"##,
    );

    let diff = compile(BASE).semantic_diff(&extended);
    assert_eq!(diff.changes().len(), 1, "an insert is one change:\n{diff}");
    assert!(matches!(
        &diff.changes()[0],
        SemanticChange::RuleAdded { index: 0, .. }
    ));
}

#[test]
fn service_and_app_membership_changes_are_reported() {
    let origin = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/*"}]}]},
        "webcredentials":{"apps":["A.b"]}}"#,
    );
    let cdn = compile(
        r#"{"applinks":{"details":[
            {"appIDs":["A.b"],"components":[{"/":"/*"}]},
            {"appIDs":["A.c"],"components":[{"/":"/*"}]}
        ]},
        "appclips":{"apps":["A.b.Clip"]}}"#,
    );

    let diff = origin.semantic_diff(&cdn);
    let changes = diff.changes();

    assert!(changes.iter().any(|change| matches!(
        change,
        SemanticChange::ServiceRemoved {
            service: Service::WebCredentials
        }
    )));
    assert!(changes.iter().any(|change| matches!(
        change,
        SemanticChange::ServiceAdded {
            service: Service::AppClips
        }
    )));
    assert!(changes.iter().any(|change| matches!(
        change,
        SemanticChange::AppAdded { service: Service::AppLinks, app_id } if app_id == "A.c"
    )));
    assert!(changes.iter().any(|change| matches!(
        change,
        SemanticChange::AppRemoved { service: Service::WebCredentials, app_id } if app_id == "A.b"
    )));
}

#[test]
fn substitution_changes_are_reported() {
    let before = compile(
        r#"{"applinks":{"substitutionVariables":{"food":["pizza"]},
        "details":[{"appIDs":["A.b"],"components":[{"/":"/$(food)/*"}]}]}}"#,
    );
    let after = compile(
        r#"{"applinks":{"substitutionVariables":{"food":["pizza","sushi"]},
        "details":[{"appIDs":["A.b"],"components":[{"/":"/$(food)/*"}]}]}}"#,
    );

    let diff = before.semantic_diff(&after);
    assert!(diff
        .changes()
        .iter()
        .any(|change| matches!(change, SemanticChange::SubstitutionChanged { name, .. } if name == "food")));
}

#[test]
fn legacy_and_modern_forms_are_not_claimed_to_be_equivalent() {
    // The two files behave the same for these URLs, but this crate will not assert equivalence
    // across formats it cannot prove equal. Under-claiming is the safe direction.
    let legacy = compile(r#"{"applinks":{"details":[{"appID":"A.b","paths":["/buy/*"]}]}}"#);
    let modern =
        compile(r#"{"applinks":{"details":[{"appID":"A.b","components":[{"/":"/buy/*"}]}]}}"#);
    assert!(!legacy.semantic_equal(&modern));
}

#[test]
fn the_diff_renders_something_a_human_can_read() {
    let before =
        compile(r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/help/*"}]}]}}"#);
    let after = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/help/*","caseSensitive":false}]}]}}"#,
    );
    let rendered = before.semantic_diff(&after).to_string();
    assert!(rendered.contains("RULE_CHANGED"), "{rendered}");
    assert!(rendered.contains("caseSensitive=true"), "{rendered}");
    assert!(rendered.contains("caseSensitive=false"), "{rendered}");
}

#[test]
fn normalized_json_resolves_defaults_and_keeps_rule_order() {
    let aasa = compile(
        r#"{"applinks":{"defaults":{"caseSensitive":false},"details":[{"appIDs":["A.b"],
            "components":[{"/":"/b/*"},{"/":"/a/*"}]}]}}"#,
    );
    let normalized = aasa.to_normalized_json();
    assert!(
        normalized.contains("\"case_sensitive\": false"),
        "{normalized}"
    );
    let first = normalized.find("/b/*").expect("first rule present");
    let second = normalized.find("/a/*").expect("second rule present");
    assert!(first < second, "rule order must survive normalization");
}

/// The comparison is sound but not complete, and the documentation now says so. This pins the
/// direction: it must never call two differing documents equivalent, and it is allowed to report a
/// difference that no URL witnesses.
#[test]
fn the_comparison_is_conservative_not_behavioural() {
    // Two rules whose patterns can never both match. Reordering them changes no decision.
    let one = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],
        "components":[{"/":"/foo/*"},{"/":"/bar/*"}]}]}}"#,
    );
    let other = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],
        "components":[{"/":"/bar/*"},{"/":"/foo/*"}]}]}}"#,
    );

    for url in [
        "https://example.com/foo/x",
        "https://example.com/bar/x",
        "https://example.com/baz/x",
    ] {
        assert_eq!(
            one.decide("example.com", "A.b", url).unwrap(),
            other.decide("example.com", "A.b", url).unwrap(),
            "{url} decides the same in both"
        );
    }

    // ...and the comparison still reports it. That is the documented, conservative direction.
    assert!(
        !one.semantic_equal(&other),
        "a reorder is reported even when no URL witnesses it; if this ever becomes true, the \
         module documentation about completeness needs revisiting"
    );

    // A substitution variable no pattern uses is reported for the same reason.
    let unused = compile(
        r#"{"applinks":{"substitutionVariables":{"unused":["x"]},
        "details":[{"appIDs":["A.b"],"components":[{"/":"/foo/*"},{"/":"/bar/*"}]}]}}"#,
    );
    assert!(!one.semantic_equal(&unused));
}

/// The direction that must never break: equivalent has to mean identical decisions.
#[test]
fn equivalent_documents_never_decide_differently() {
    let inline = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[
        {"/":"/a/*","caseSensitive":false},{"/":"/b/*","caseSensitive":false}]}]}}"#,
    );
    let hoisted = compile(
        r#"{"applinks":{"details":[{"appIDs":["A.b"],
        "defaults":{"caseSensitive":false},"components":[{"/":"/a/*"},{"/":"/b/*"}]}]}}"#,
    );
    assert!(inline.semantic_equal(&hoisted));

    for url in [
        "https://example.com/a/x",
        "https://example.com/A/x",
        "https://example.com/b/x",
        "https://example.com/c/x",
    ] {
        assert_eq!(
            inline.decide("example.com", "A.b", url).unwrap(),
            hoisted.decide("example.com", "A.b", url).unwrap(),
            "{url}"
        );
    }
}
