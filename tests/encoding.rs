//! `percentEncoded` behaviour.
//!
//! Apple documents the key as "whether URLs are percent-encoded", defaulting to `true`, without
//! spelling out the comparison. This crate implements the reading that keeps patterns usable:
//! with `true` the pattern is compared against the URL component exactly as written, and with
//! `false` the URL component is percent-decoded first so a pattern can contain literal spaces and
//! non-ASCII text. These tests pin that choice down; `docs/parity.md` records that it is not yet
//! oracle-verified against `swcutil`.

mod common;

use blazingly_aasa::CompiledAasa;
use blazingly_aasa::MatchDecision::{Match, NoMatch};
use common::expect;

const APP: &str = "ABCDE12345.com.example.app";
const DOMAIN: &str = "example.com";

fn with_path(pattern: &str, percent_encoded: Option<bool>) -> CompiledAasa {
    let flag = match percent_encoded {
        Some(value) => format!(r#","percentEncoded":{value}"#),
        None => String::new(),
    };
    let json = format!(
        r#"{{"applinks":{{"details":[{{"appIDs":["{APP}"],"components":[{{"/":"{pattern}"{flag}}}]}}]}}}}"#
    );
    CompiledAasa::parse(json.as_bytes()).expect("fixture should parse")
}

#[test]
fn by_default_the_pattern_is_compared_against_the_encoded_url() {
    let encoded = with_path("/a%20b", None);
    expect(&encoded, DOMAIN, APP, "https://example.com/a%20b", Match);
    expect(&encoded, DOMAIN, APP, "https://example.com/a b", NoMatch);

    let literal = with_path("/a b", None);
    expect(&literal, DOMAIN, APP, "https://example.com/a%20b", NoMatch);
}

#[test]
fn percent_encoded_false_decodes_the_url_before_comparing() {
    let literal = with_path("/a b", Some(false));
    expect(&literal, DOMAIN, APP, "https://example.com/a%20b", Match);
    expect(&literal, DOMAIN, APP, "https://example.com/a b", Match);
}

#[test]
fn an_encoded_slash_stays_encoded_by_default() {
    // This distinction matters: `%2F` is not a path separator, and a rule written against the
    // decoded form would otherwise accept a URL it never meant to.
    let encoded = with_path("/a/b", None);
    expect(&encoded, DOMAIN, APP, "https://example.com/a/b", Match);
    expect(&encoded, DOMAIN, APP, "https://example.com/a%2Fb", NoMatch);

    // With decoding turned on, the two become indistinguishable.
    let decoded = with_path("/a/b", Some(false));
    expect(&decoded, DOMAIN, APP, "https://example.com/a%2Fb", Match);
}

#[test]
fn non_ascii_paths_work_in_both_directions() {
    let encoded = with_path("/caf%C3%A9/*", None);
    expect(
        &encoded,
        DOMAIN,
        APP,
        "https://example.com/caf%C3%A9/menu",
        Match,
    );

    let literal = with_path("/café/*", Some(false));
    expect(
        &literal,
        DOMAIN,
        APP,
        "https://example.com/caf%C3%A9/menu",
        Match,
    );
    expect(
        &literal,
        DOMAIN,
        APP,
        "https://example.com/café/menu",
        Match,
    );
}

#[test]
fn escape_hex_case_matters_when_comparing_encoded_text() {
    // `%C3%A9` and `%c3%a9` denote the same character but are different strings, so a
    // case-sensitive comparison against the raw URL separates them. Decoding removes the
    // difference.
    let encoded = with_path("/caf%C3%A9", None);
    expect(
        &encoded,
        DOMAIN,
        APP,
        "https://example.com/caf%c3%a9",
        NoMatch,
    );

    let decoded = with_path("/café", Some(false));
    expect(
        &decoded,
        DOMAIN,
        APP,
        "https://example.com/caf%c3%a9",
        Match,
    );
}

#[test]
fn an_invalid_escape_is_left_alone_rather_than_dropped() {
    let decoded = with_path("/100%zz", Some(false));
    expect(&decoded, DOMAIN, APP, "https://example.com/100%zz", Match);

    // A truncated escape at the very end must not panic or eat characters.
    let truncated = with_path("/x%2", Some(false));
    expect(&truncated, DOMAIN, APP, "https://example.com/x%2", Match);
}

#[test]
fn query_and_fragment_follow_the_same_rule() {
    let json = format!(
        r##"{{"applinks":{{"details":[{{"appIDs":["{APP}"],"components":[
            {{"?":{{"q":"a b"}},"#":"a b","percentEncoded":false}}
        ]}}]}}}}"##
    );
    let aasa = CompiledAasa::parse(json.as_bytes()).unwrap();
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/x?q=a%20b#a%20b",
        Match,
    );
}

#[test]
fn percent_decode_is_lossless_for_text_without_escapes() {
    assert_eq!(blazingly_aasa::percent_decode("/plain/path"), "/plain/path");
    assert_eq!(blazingly_aasa::percent_decode("a%2Bb"), "a+b");
    // `+` is not a space outside form encoding, and this crate does not pretend otherwise.
    assert_eq!(blazingly_aasa::percent_decode("a+b"), "a+b");
}
