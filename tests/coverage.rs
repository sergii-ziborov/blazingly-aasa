//! Coverage this crate has that the existing AASA tooling does not.
//!
//! Each test names the tool it is contrasted with. `docs/competitors.md` holds the full matrix;
//! this file is the executable half of it, so a regression here is a regression against a claim
//! made in the README.

#![allow(clippy::needless_raw_string_hashes)] // JSON fixtures read better with one delimiter

mod common;

use blazingly_aasa::MatchDecision::{Exclude, Match, NoMatch};
use blazingly_aasa::{split_app_id, CompiledAasa, DiagnosticCode, Service};
use common::expect;

const APP: &str = "ABCDE12345.com.example.app";
const DOMAIN: &str = "example.com";

fn compile(json: &str) -> CompiledAasa {
    CompiledAasa::parse(json.as_bytes()).expect("fixture should parse")
}

/// Every JavaScript and Go tool surveyed declares `substitutionVariables` in its types and then
/// ignores it when matching. `universal-links-test` additionally escapes `$` into its generated
/// regex, so `$(food)` is compared as the literal text `$(food)`.
#[test]
fn substitution_variables_are_actually_expanded() {
    let aasa = compile(
        r#"{"applinks":{"substitutionVariables":{"food":["pizza","sushi"]},
        "details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"/":"/order/$(food)/*"}]}]}}"#,
    );

    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/order/pizza/1",
        Match,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/order/sushi/1",
        Match,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/order/tacos/1",
        NoMatch,
    );
    // The literal text is not what the pattern means.
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/order/$(food)/1",
        NoMatch,
    );
}

/// `$(region)` and `$(lang)` require Foundation's ISO lists. No surveyed tool ships them.
#[test]
fn predefined_locale_variables_resolve_against_real_iso_tables() {
    let aasa = compile(
        r#"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/$(lang)-$(region)/home"}]}]}}"#,
    );

    expect(&aasa, DOMAIN, APP, "https://example.com/en-US/home", Match);
    expect(&aasa, DOMAIN, APP, "https://example.com/ja-JP/home", Match);
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/qq-ZZ/home",
        NoMatch,
    );
}

/// `universal-links-test` carries `// TODO: Handle percentEncoded`; the others never mention it
/// during matching.
#[test]
fn percent_encoded_is_implemented_rather_than_deferred() {
    let strict = compile(
        r#"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/a/b"}]}]}}"#,
    );
    // Under the default, %2F is not a separator.
    expect(&strict, DOMAIN, APP, "https://example.com/a%2Fb", NoMatch);

    let decoded = compile(
        r#"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/a/b","percentEncoded":false}]}]}}"#,
    );
    expect(&decoded, DOMAIN, APP, "https://example.com/a%2Fb", Match);
}

/// No surveyed tool evaluates the legacy `paths` array, though files using it are still served.
#[test]
fn legacy_paths_are_evaluated_not_just_parsed() {
    // `paths` is ordered and first-match-wins, exactly like `components`. That is why the standard
    // advice is to put `NOT` entries *before* the wildcard that would otherwise swallow them.
    let aasa = compile(
        r#"{"applinks":{"apps":[],"details":[{"appID":"ABCDE12345.com.example.app",
        "paths":["NOT /wwdc/internal/*","/wwdc/*"]}]}}"#,
    );
    expect(&aasa, DOMAIN, APP, "https://example.com/wwdc/2015", Match);
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/wwdc/internal/notes",
        Exclude,
    );
    expect(&aasa, DOMAIN, APP, "https://example.com/other", NoMatch);

    // Reversed, the exclusion is unreachable -- and the validator says so rather than leaving it
    // to be discovered in production.
    let shadowed = compile(
        r#"{"applinks":{"apps":[],"details":[{"appID":"ABCDE12345.com.example.app",
        "paths":["/wwdc/*","NOT /wwdc/internal/*"]}]}}"#,
    );
    expect(
        &shadowed,
        DOMAIN,
        APP,
        "https://example.com/wwdc/internal/notes",
        Match,
    );
}

/// `universal-links-test` answers "which apps match this URL" and this crate now does too, using
/// the same first-entry-wins rule as a single-app decision.
#[test]
fn reverse_query_lists_every_app_reached_by_a_url() {
    let aasa = compile(
        r#"{"applinks":{"details":[
            {"appIDs":["T1.com.a","T1.com.b"],"components":[{"/":"/shop/*"}]},
            {"appID":"T1.com.blocked","components":[{"/":"/shop/*","exclude":true}]},
            {"appID":"T1.com.other","components":[{"/":"/news/*"}]}
        ]}}"#,
    );

    let apps = aasa
        .apps_for_url(DOMAIN, "https://example.com/shop/42")
        .unwrap();
    assert_eq!(
        apps,
        vec![
            ("T1.com.a".to_owned(), Match),
            ("T1.com.b".to_owned(), Match),
            ("T1.com.blocked".to_owned(), Exclude),
        ]
    );

    // Apps that do not match are omitted entirely.
    assert!(aasa
        .apps_for_url(DOMAIN, "https://example.com/nothing")
        .unwrap()
        .is_empty());
}

/// The reverse query must never disagree with the single-app decision.
#[test]
fn reverse_query_agrees_with_decide() {
    let aasa = compile(
        r#"{"applinks":{"details":[
            {"appIDs":["T1.com.a"],"components":[{"/":"/x/*","exclude":true}]},
            {"appIDs":["T1.com.a","T1.com.b"],"components":[{"/":"/x/*"}]}
        ]}}"#,
    );
    for url in [
        "https://example.com/x/1",
        "https://example.com/y/1",
        "https://example.com/",
    ] {
        let listed = aasa.apps_for_url(DOMAIN, url).unwrap();
        for app in ["T1.com.a", "T1.com.b"] {
            let direct = aasa.decide(DOMAIN, app, url).unwrap();
            let reverse = listed
                .iter()
                .find(|(candidate, _)| candidate == app)
                .map_or(NoMatch, |(_, decision)| *decision);
            assert_eq!(direct, reverse, "{app} at {url}");
        }
    }
}

/// `yurl` and `@linkforty/aasa-core` take the team prefix and bundle identifier separately.
#[test]
fn team_and_bundle_identifiers_can_be_supplied_separately() {
    let aasa = compile(
        r#"{"applinks":{"details":[{"appID":"ABCDE12345.com.example.app","components":[{"/":"/*"}]}]},
        "webcredentials":{"apps":["FGHIJ67890.com.example.app"]}}"#,
    );

    assert_eq!(
        aasa.services_for_bundle("ABCDE12345", "com.example.app"),
        vec![Service::AppLinks]
    );
    assert_eq!(
        aasa.services_for_bundle("FGHIJ67890", "com.example.app"),
        vec![Service::WebCredentials]
    );
    assert!(aasa
        .services_for_bundle("ZZZZZ00000", "com.example.app")
        .is_empty());

    // Same bundle, two team prefixes: a real symptom of an app moving between teams.
    assert_eq!(
        aasa.app_ids_for_bundle("com.example.app"),
        vec!["ABCDE12345.com.example.app", "FGHIJ67890.com.example.app"]
    );

    assert_eq!(
        split_app_id("ABCDE12345.com.example.app"),
        Some(("ABCDE12345", "com.example.app"))
    );
    assert_eq!(split_app_id("nodots"), None);
    assert_eq!(split_app_id(".com.example"), None);
}

/// `yurl` reads CMS-signed files; the JavaScript tools report them as invalid JSON.
#[test]
fn a_cms_signed_file_is_read_and_flagged() {
    let signed = signed_fixture(br#"{"applinks":{"details":[{"appID":"ABCDE12345.com.example.app","components":[{"/":"/buy/*"}]}]}}"#);

    let aasa = CompiledAasa::parse(&signed).expect("a signed file should still parse");
    expect(&aasa, DOMAIN, APP, "https://example.com/buy/1", Match);

    let report = aasa.validate();
    assert!(
        report.contains(DiagnosticCode::SignedPayload),
        "the signature status must be surfaced:\n{report}"
    );
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|d| d.code == DiagnosticCode::SignedPayload)
        .unwrap();
    assert!(
        diagnostic.message.contains("NOT verified"),
        "reading is not checking: {}",
        diagnostic.message
    );
}

#[test]
fn a_der_blob_with_no_payload_says_so_rather_than_blaming_json() {
    // A well-formed signedData envelope whose encapsulated content is missing.
    const OID_SIGNED: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x02];
    let mut inner = vec![0x06, 9];
    inner.extend_from_slice(OID_SIGNED);
    let mut der = vec![0x30, u8::try_from(inner.len()).unwrap()];
    der.extend_from_slice(&inner);

    let error = CompiledAasa::parse(&der).unwrap_err();
    assert!(
        error.message().contains("signedData"),
        "unhelpful message: {error}"
    );
}

/// The DER SEQUENCE tag `0x30` is also the ASCII digit `0`, so sniffing on the leading byte alone
/// reports a JSON document of `0` as a signing problem. JSON is tried first for exactly this
/// reason.
#[test]
fn a_json_number_is_not_mistaken_for_a_signed_file() {
    for input in ["0", "0.5", "  0  ", "01"] {
        let error = CompiledAasa::parse(input.as_bytes()).unwrap_err();
        assert!(
            !error.message().contains("CMS-signed"),
            "{input:?} was blamed on signing: {error}"
        );
    }
    // A bare `0` is valid JSON, so the complaint should be about the root type.
    assert_eq!(
        CompiledAasa::parse(b"0").unwrap_err().kind(),
        &blazingly_aasa::ParseErrorKind::RootNotObject
    );
}

/// A signed file whose payload is not JSON must blame the payload, not the envelope.
#[test]
fn a_signed_file_with_a_broken_payload_says_which_part_is_broken() {
    let signed = signed_fixture(b"{ this is not json");
    let error = CompiledAasa::parse(&signed).unwrap_err();
    assert!(
        error
            .message()
            .contains("CMS-signed payload is not valid JSON"),
        "unhelpful message: {error}"
    );
}

/// Builds a minimal CMS `SignedData` wrapper around `payload`, mirroring the iOS 9 format.
fn signed_fixture(payload: &[u8]) -> Vec<u8> {
    fn tlv(tag: u8, contents: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        let length = contents.len();
        if length < 0x80 {
            out.push(u8::try_from(length).unwrap());
        } else if length < 0x100 {
            out.extend_from_slice(&[0x81, u8::try_from(length).unwrap()]);
        } else {
            out.extend_from_slice(&[
                0x82,
                u8::try_from(length >> 8).unwrap(),
                u8::try_from(length & 0xff).unwrap(),
            ]);
        }
        out.extend_from_slice(contents);
        out
    }
    const OID_DATA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x01];
    const OID_SIGNED_DATA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x02];

    let mut encap = tlv(0x06, OID_DATA);
    encap.extend_from_slice(&tlv(0xa0, &tlv(0x04, payload)));
    let encap = tlv(0x30, &encap);

    let mut signed_data = tlv(0x02, &[0x01]);
    signed_data.extend_from_slice(&tlv(0x31, &[]));
    signed_data.extend_from_slice(&encap);
    let signed_data = tlv(0x30, &signed_data);

    let mut content_info = tlv(0x06, OID_SIGNED_DATA);
    content_info.extend_from_slice(&tlv(0xa0, &signed_data));
    tlv(0x30, &content_info)
}
