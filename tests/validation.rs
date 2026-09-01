//! Every diagnostic code has a document that triggers it. Codes are a public contract, so this
//! file doubles as their specification.
#![allow(clippy::needless_raw_string_hashes)] // JSON fixtures read better with one delimiter

use blazingly_aasa::{CompiledAasa, DiagnosticCode, Severity};

fn report(json: &str) -> blazingly_aasa::ValidationReport {
    CompiledAasa::parse(json.as_bytes())
        .expect("fixture should parse")
        .validate()
}

fn assert_code(json: &str, code: DiagnosticCode) {
    let report = report(json);
    assert!(
        report.contains(code),
        "expected {code} ({}) but got:\n{report}",
        code.title()
    );
}

#[test]
fn a_clean_document_reports_nothing() {
    let report = report(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
            "components":[{"/":"/buy/*"},{"#":"x","exclude":true}]}]}}"##,
    );
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn no_recognized_service() {
    assert_code(r#"{}"#, DiagnosticCode::NoRecognizedService);
}

#[test]
fn detail_missing_app_id() {
    assert_code(
        r#"{"applinks":{"details":[{"components":[{"/":"/*"}]}]}}"#,
        DiagnosticCode::DetailMissingAppId,
    );
}

#[test]
fn mixed_components_and_paths() {
    assert_code(
        r#"{"applinks":{"details":[{"appID":"A.b","components":[{"/":"/a"}],"paths":["/b"]}]}}"#,
        DiagnosticCode::MixedComponentsAndPaths,
    );
}

#[test]
fn empty_and_suspicious_app_identifiers() {
    assert_code(
        r#"{"applinks":{"details":[{"appIDs":[""],"components":[{"/":"/*"}]}]}}"#,
        DiagnosticCode::EmptyAppIdentifier,
    );
    assert_code(
        r#"{"applinks":{"details":[{"appIDs":["nodots"],"components":[{"/":"/*"}]}]}}"#,
        DiagnosticCode::SuspiciousAppIdentifier,
    );
}

#[test]
fn substitution_problems() {
    assert_code(
        r#"{"applinks":{"substitutionVariables":{"a$b":["x"]},"details":[]}}"#,
        DiagnosticCode::MalformedSubstitutionName,
    );
    assert_code(
        r#"{"applinks":{"substitutionVariables":{"a":["$(b)"],"b":["x"]},"details":[]}}"#,
        DiagnosticCode::RecursiveSubstitutionValue,
    );
    assert_code(
        r#"{"applinks":{"substitutionVariables":{"a":[]},"details":[
            {"appID":"A.b","components":[{"/":"/$(a)"}]}]}}"#,
        DiagnosticCode::EmptySubstitutionList,
    );
    assert_code(
        r#"{"applinks":{"details":[{"appID":"A.b","components":[{"/":"/$(nope)/*"}]}]}}"#,
        DiagnosticCode::UnknownSubstitutionVariable,
    );
    assert_code(
        r#"{"applinks":{"details":[{"appID":"A.b","components":[{"/":"/$(oops/*"}]}]}}"#,
        DiagnosticCode::UnterminatedSubstitutionReference,
    );
    assert_code(
        r#"{"applinks":{"substitutionVariables":{"digit":["7"]},"details":[]}}"#,
        DiagnosticCode::SubstitutionShadowsPredefined,
    );
}

#[test]
fn an_undefined_variable_never_matches_instead_of_matching_wildly() {
    let aasa = CompiledAasa::parse(
        br#"{"applinks":{"details":[{"appID":"A.b","components":[{"/":"/$(nope)/*"}]}]}}"#,
    )
    .unwrap();
    let result = aasa
        .match_url("example.com", "A.b", "https://example.com/anything/x")
        .unwrap();
    assert!(
        !result.is_match(),
        "a broken pattern must not match:\n{result}"
    );
}

#[test]
fn unsupported_query_predicate() {
    assert_code(
        r#"{"applinks":{"details":[{"appID":"A.b","components":[{"?":{"flag":true}}]}]}}"#,
        DiagnosticCode::UnsupportedQueryPredicate,
    );
}

#[test]
fn duplicate_app_identifier() {
    assert_code(
        r#"{"applinks":{"details":[
            {"appID":"A.b","components":[{"/":"/a"}]},
            {"appID":"A.b","components":[{"/":"/b"}]}
        ]}}"#,
        DiagnosticCode::DuplicateAppIdentifier,
    );
    assert_code(
        r#"{"webcredentials":{"apps":["A.b","A.b"]}}"#,
        DiagnosticCode::DuplicateAppIdentifier,
    );
}

#[test]
fn empty_component_rule_and_unreachable_rules() {
    let json = r#"{"applinks":{"details":[{"appID":"A.b","components":[
        {"comment":"catches everything"},
        {"/":"/never/*"}
    ]}]}}"#;
    assert_code(json, DiagnosticCode::EmptyComponentRule);
    assert_code(json, DiagnosticCode::UnreachableRule);
}

#[test]
fn a_star_path_also_counts_as_a_catch_all() {
    assert_code(
        r#"{"applinks":{"details":[{"appID":"A.b","components":[
            {"/":"*"},
            {"/":"/never/*"}
        ]}]}}"#,
        DiagnosticCode::UnreachableRule,
    );
}

#[test]
fn path_pattern_missing_leading_slash() {
    assert_code(
        r#"{"applinks":{"details":[{"appID":"A.b","components":[{"/":"buy/*"}]}]}}"#,
        DiagnosticCode::PathPatternMissingLeadingSlash,
    );
}

#[test]
fn legacy_apps_key_must_be_empty() {
    assert_code(
        r#"{"applinks":{"apps":["A.b"],"details":[{"appID":"A.b","components":[{"/":"/*"}]}]}}"#,
        DiagnosticCode::LegacyAppsKeyNonEmpty,
    );
}

#[test]
fn applinks_without_details() {
    assert_code(r#"{"applinks":{"details":[]}}"#, DiagnosticCode::NoDetails);
}

#[test]
fn defaults_carrying_pattern_keys_is_informational_only() {
    let report = report(
        r#"{"applinks":{"defaults":{"/":"/scoped/*"},"details":[
            {"appID":"A.b","components":[{"/":"/a"}]}]}}"#,
    );
    assert!(report.contains(DiagnosticCode::DefaultsContainsPatternKeys));
    assert!(!report.has_errors(), "{report}");
}

#[test]
fn severities_and_codes_are_stable() {
    assert_eq!(DiagnosticCode::InvalidJson.as_str(), "AASA001");
    assert_eq!(DiagnosticCode::DetailMissingAppId.as_str(), "AASA110");
    assert_eq!(
        DiagnosticCode::DetailMissingAppId.default_severity(),
        Severity::Error
    );
    assert_eq!(
        DiagnosticCode::MixedComponentsAndPaths.default_severity(),
        Severity::Warning
    );

    // Every code is unique and sorted, so consumers can rely on the numbering.
    let codes: Vec<&str> = DiagnosticCode::all().iter().map(|c| c.as_str()).collect();
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(codes, sorted, "diagnostic codes must be unique and ordered");
}

#[test]
fn errors_sort_ahead_of_warnings() {
    let report = report(r#"{"applinks":{"details":[{"components":[{"/":"buy/*"}]}]}}"#);
    let severities: Vec<Severity> = report.diagnostics().iter().map(|d| d.severity).collect();
    let mut sorted = severities.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(severities, sorted);
    assert!(!report.errors().is_empty());
    assert!(!report.warnings().is_empty());
}
