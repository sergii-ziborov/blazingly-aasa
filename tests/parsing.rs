//! Parsing is lenient on purpose: only genuinely unusable input fails.

use blazingly_aasa::{AasaDocument, DiagnosticCode, ParseErrorKind, ParseOptions};

#[test]
fn invalid_json_fails_with_a_location() {
    let error = AasaDocument::parse(br#"{"applinks": }"#).unwrap_err();
    assert_eq!(error.kind(), &ParseErrorKind::Json);
    assert!(
        !error.message().is_empty(),
        "the underlying JSON error should be surfaced"
    );
}

#[test]
fn a_non_object_root_fails() {
    let error = AasaDocument::parse(b"[]").unwrap_err();
    assert_eq!(error.kind(), &ParseErrorKind::RootNotObject);
}

#[test]
fn an_oversized_payload_fails_before_parsing() {
    let payload = format!(
        r#"{{"applinks":{{"details":[],"pad":"{}"}}}}"#,
        "x".repeat(4096)
    );
    let options = ParseOptions::new().max_bytes(512);
    let error = AasaDocument::parse_with(payload.as_bytes(), &options).unwrap_err();
    assert!(matches!(
        error.kind(),
        ParseErrorKind::TooLarge { limit: 512, .. }
    ));

    // The same payload is fine under the default ceiling.
    assert!(AasaDocument::parse(payload.as_bytes()).is_ok());
}

#[test]
fn unknown_top_level_keys_are_kept_rather_than_rejected() {
    let document = AasaDocument::parse(
        br#"{"applinks":{"details":[]},"somethingAppleAddedLater":{"apps":[]}}"#,
    )
    .expect("an unfamiliar service must not break parsing");

    assert_eq!(document.unknown_keys, vec!["somethingAppleAddedLater"]);
    let report = document.validate();
    assert!(report.contains(DiagnosticCode::UnknownTopLevelKey));
    assert!(
        !report.has_errors(),
        "an unknown key is informational, not fatal:\n{report}"
    );
}

#[test]
fn a_bad_field_type_is_reported_without_losing_the_rest_of_the_file() {
    let document = AasaDocument::parse(
        br#"{
            "applinks": { "details": [
                { "appIDs": ["A.good"], "components": [{ "/": "/ok/*" }] },
                { "appID": 42, "components": [{ "/": "/broken/*" }] }
            ]}
        }"#,
    )
    .expect("one broken entry must not sink the document");

    let compiled = document.compile();
    // The healthy entry still matches.
    let result = compiled
        .match_url("example.com", "A.good", "https://example.com/ok/1")
        .unwrap();
    assert!(result.is_match());

    let report = compiled.validate();
    assert!(report.contains(DiagnosticCode::FieldTypeMismatch));
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|d| d.code == DiagnosticCode::FieldTypeMismatch)
        .unwrap();
    assert_eq!(diagnostic.path, "applinks.details[1].appID");
}

#[test]
fn both_app_id_forms_are_preserved_for_the_validator() {
    let document =
        AasaDocument::parse(br#"{"applinks":{"details":[{"appID":"A.one","appIDs":["A.two"]}]}}"#)
            .unwrap();
    let detail = &document.applinks.as_ref().unwrap().details[0];
    assert_eq!(detail.app_id.as_deref(), Some("A.one"));
    assert_eq!(detail.app_ids.as_deref(), Some(&["A.two".to_owned()][..]));

    // Both are honoured when matching, and the contradiction is reported.
    let compiled = document.compile();
    assert!(compiled.has_applink_app("A.one"));
    assert!(compiled.has_applink_app("A.two"));
    assert!(compiled
        .validate()
        .contains(DiagnosticCode::DetailHasBothAppIdForms));
}

#[test]
fn byte_length_is_recorded() {
    let bytes = br#"{"applinks":{"details":[]}}"#;
    assert_eq!(AasaDocument::parse(bytes).unwrap().byte_len(), bytes.len());
}

#[test]
fn an_explicit_null_service_is_treated_as_absent() {
    let document = AasaDocument::parse(br#"{"applinks":null,"webcredentials":null}"#).unwrap();
    assert!(document.applinks.is_none());
    assert!(document.webcredentials.is_none());
}

#[test]
fn deeply_nested_junk_does_not_panic() {
    let payload = format!("{}{}", "[".repeat(2000), "]".repeat(2000));
    // Whatever the depth limit does, it must be an error rather than a crash.
    let _ = AasaDocument::parse(payload.as_bytes());
}
