//! Matching semantics beyond Apple's own examples: the defaults hierarchy, both query forms,
//! rule ordering, and the host check.
#![allow(clippy::needless_raw_string_hashes)] // JSON fixtures read better with one delimiter

mod common;

use blazingly_aasa::MatchDecision::{Exclude, Match, NoMatch};
use blazingly_aasa::{CompiledAasa, ComponentReason, StopReason, UrlComponent};
use common::expect;

const APP: &str = "ABCDE12345.com.example.app";

fn compile(json: &str) -> CompiledAasa {
    CompiledAasa::parse(json.as_bytes()).expect("fixture should parse")
}

#[test]
fn unspecified_components_default_to_matching_everything() {
    // Apple: "The pattern to match with the URL path component. The default is *".
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"#":"promo"}]}]}}"##,
    );

    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/a/b?c=d#promo",
        Match,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/#promo",
        Match,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/a/b#other",
        NoMatch,
    );
}

#[test]
fn an_absent_component_reads_as_the_empty_string() {
    // Apple's own example relies on this: `"#": "*"` matches a URL with no fragment at all.
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/a","?":"*","#":"*"}]}]}}"##,
    );
    expect(&aasa, "example.com", APP, "https://example.com/a", Match);

    let strict = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/a","#":"?*"}]}]}}"##,
    );
    expect(
        &strict,
        "example.com",
        APP,
        "https://example.com/a",
        NoMatch,
    );
    expect(
        &strict,
        "example.com",
        APP,
        "https://example.com/a#x",
        Match,
    );
}

#[test]
fn defaults_cascade_from_domain_to_app_to_rule() {
    let aasa = compile(
        r##"{"applinks":{
        "defaults": { "caseSensitive": false },
        "details": [{
            "appIDs": ["ABCDE12345.com.example.app"],
            "defaults": { "caseSensitive": true },
            "components": [
                { "/": "/Loose/*", "caseSensitive": false },
                { "/": "/Strict/*" }
            ]
        }]
    }}"##,
    );

    // Rule level beats app level.
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/loose/1",
        Match,
    );
    // App level beats domain level.
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/strict/1",
        NoMatch,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/Strict/1",
        Match,
    );
}

#[test]
fn a_case_only_failure_is_explained_as_such() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/Strict/*"}]}]}}"##,
    );
    let result = aasa
        .match_url("example.com", APP, "https://example.com/strict/1")
        .unwrap();
    assert_eq!(result.decision, NoMatch);

    let closest = result
        .trace
        .closest_failure
        .expect("a near miss was recorded");
    let path = closest
        .components
        .iter()
        .find(|component| component.component == UrlComponent::Path)
        .unwrap();
    assert_eq!(path.reason, ComponentReason::CaseMismatch);
}

#[test]
fn whole_query_and_query_dictionary_are_different_constraints() {
    let whole = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":"a=1"}]}]}}"##,
    );
    expect(
        &whole,
        "example.com",
        APP,
        "https://example.com/x?a=1",
        Match,
    );
    // The whole query string must match, so an extra item breaks it.
    expect(
        &whole,
        "example.com",
        APP,
        "https://example.com/x?a=1&b=2",
        NoMatch,
    );

    let items = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":{"a":"1"}}]}]}}"##,
    );
    expect(
        &items,
        "example.com",
        APP,
        "https://example.com/x?a=1",
        Match,
    );
    // A dictionary constrains only the items it names.
    expect(
        &items,
        "example.com",
        APP,
        "https://example.com/x?a=1&b=2",
        Match,
    );
    expect(
        &items,
        "example.com",
        APP,
        "https://example.com/x?b=2",
        NoMatch,
    );
    expect(
        &items,
        "example.com",
        APP,
        "https://example.com/x?a=2",
        NoMatch,
    );
}

#[test]
fn a_missing_query_item_counts_as_empty() {
    // swcutil treats an item the URL does not carry as present with an empty value, so a pattern
    // that accepts the empty string is satisfied by its absence. Confirmed against the oracle;
    // Apple documents none of this.
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":{"a":"1","b":"*"}}]}]}}"##,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?a=1&b=anything",
        Match,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?a=1",
        Match,
    );

    // `?*` needs at least one character, so an absent item does not satisfy it.
    let strict = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":{"b":"?*"}}]}]}}"##,
    );
    expect(
        &strict,
        "example.com",
        APP,
        "https://example.com/x?a=1",
        NoMatch,
    );
    expect(
        &strict,
        "example.com",
        APP,
        "https://example.com/x?b=1",
        Match,
    );
}

#[test]
fn a_query_item_without_a_value_reads_as_empty() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":{"flag":""}}]}]}}"##,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?flag",
        Match,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?flag=",
        Match,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?flag=1",
        NoMatch,
    );
}

#[test]
fn every_occurrence_of_a_repeated_query_item_must_match() {
    // swcutil requires *all* occurrences to match, not any: `?id=7&id=42` fails the pattern `42`
    // whichever position the target sits in, while `?id=7&id=7` passes the pattern `7`.
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":{"id":"42"}}]}]}}"##,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?id=42",
        Match,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?id=7&id=42",
        NoMatch,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?id=42&id=7",
        NoMatch,
    );

    let identical = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":{"id":"7"}}]}]}}"##,
    );
    expect(
        &identical,
        "example.com",
        APP,
        "https://example.com/x?id=7&id=7",
        Match,
    );
}

/// A non-string predicate does not disable just itself: swcutil discards the whole `?` dictionary,
/// so every constraint beside it stops applying. `AASA150` reports it as an error for that reason.
#[test]
fn a_non_string_predicate_discards_the_whole_query_dictionary() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"?":{"a":"1","flag":true}}]}]}}"##,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?a=1",
        Match,
    );
    // `a=2` violates the string predicate, and it matches anyway.
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/x?a=2",
        Match,
    );
    assert!(aasa
        .validate()
        .contains(blazingly_aasa::DiagnosticCode::UnsupportedQueryPredicate));

    // The human-facing explanation has to describe *widening*. An earlier release said the
    // predicate "can never match", which is the opposite of what swcutil does, and no test
    // caught it because the matcher was already right. This pins the wording's direction.
    let help = aasa
        .validate()
        .diagnostics()
        .iter()
        .find(|d| d.code == blazingly_aasa::DiagnosticCode::UnsupportedQueryPredicate)
        .and_then(|d| d.help.clone())
        .expect("AASA150 carries help");
    assert!(
        !help.contains("never match"),
        "help must not claim the predicate cannot match: {help}"
    );
    assert!(
        help.contains("ignores the entire query dictionary"),
        "help must say the whole dictionary is discarded: {help}"
    );

    // The trace marks the component as matched, so its reason must agree.
    let trace = aasa
        .match_url("example.com", APP, "https://example.com/x?a=2")
        .expect("url parses");
    let reasons: Vec<_> = trace
        .trace
        .details
        .iter()
        .flat_map(|d| d.rules.iter())
        .flat_map(|r| r.components.iter())
        .filter(|c| c.reason == blazingly_aasa::ComponentReason::UnsupportedPredicate)
        .collect();
    assert!(!reasons.is_empty(), "the ignored dictionary is traced");
    for component in reasons {
        assert_eq!(
            component.matched,
            component.reason.is_match(),
            "matched and reason.is_match() must not disagree"
        );
    }
}

/// A trailing slash is insignificant at both ends, and a leading slash is optional in the pattern.
#[test]
fn slashes_at_the_ends_of_a_path_are_insignificant() {
    let star = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/buy/*"}]}]}}"##,
    );
    expect(&star, "example.com", APP, "https://example.com/buy", Match);
    expect(&star, "example.com", APP, "https://example.com/buy/", Match);
    expect(
        &star,
        "example.com",
        APP,
        "https://example.com/buy/42",
        Match,
    );
    expect(
        &star,
        "example.com",
        APP,
        "https://example.com/buyer",
        NoMatch,
    );

    let bare = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"buy/*"}]}]}}"##,
    );
    expect(
        &bare,
        "example.com",
        APP,
        "https://example.com/buy/42",
        Match,
    );

    // But a wildcard still counts characters: `????` is four, and `481` plus a slash is not four.
    let counted = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/id/????"}]}]}}"##,
    );
    expect(
        &counted,
        "example.com",
        APP,
        "https://example.com/id/4815",
        Match,
    );
    expect(
        &counted,
        "example.com",
        APP,
        "https://example.com/id/481",
        NoMatch,
    );
}

#[test]
fn a_missing_query_item_is_named_in_the_trace() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/help/*","?":{"articleNumber":"????"}}]}]}}"##,
    );
    let result = aasa
        .match_url("example.com", APP, "https://example.com/help/1")
        .unwrap();
    let closest = result.trace.closest_failure.unwrap();
    let item = closest
        .components
        .iter()
        .find(|component| component.component == UrlComponent::QueryItem("articleNumber".into()))
        .expect("the missing item is traced by name");
    assert_eq!(item.reason, ComponentReason::MissingQueryItem);
}

#[test]
fn exclusion_stops_the_scan_instead_of_falling_through() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[
            {"/":"/a/private/*","exclude":true},
            {"/":"/a/*"}
        ]}]}}"##,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/a/public",
        Match,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://example.com/a/private/x",
        Exclude,
    );

    // Reversing the order changes the answer, because the first match wins.
    let reversed = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[
            {"/":"/a/*"},
            {"/":"/a/private/*","exclude":true}
        ]}]}}"##,
    );
    expect(
        &reversed,
        "example.com",
        APP,
        "https://example.com/a/private/x",
        Match,
    );
}

#[test]
fn details_are_scanned_in_order() {
    let aasa = compile(
        r##"{"applinks":{"details":[
        {"appIDs":["ABCDE12345.com.example.app"],"components":[{"/":"/shared/*","exclude":true}]},
        {"appIDs":["ABCDE12345.com.example.app"],"components":[{"/":"/shared/*"}]}
    ]}}"##,
    );
    let result = aasa
        .match_url("example.com", APP, "https://example.com/shared/1")
        .unwrap();
    assert_eq!(result.decision, Exclude);
    assert_eq!(result.trace.selected_detail, Some(0));
}

#[test]
fn a_host_that_is_not_the_served_domain_never_matches() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/*"}]}]}}"##,
    );
    let result = aasa
        .match_url("example.com", APP, "https://evil.test/anything")
        .unwrap();
    assert_eq!(result.decision, NoMatch);
    assert!(matches!(
        result.trace.stop_reason,
        StopReason::HostMismatch { .. }
    ));

    // Host comparison is case-insensitive, as RFC 3986 requires.
    expect(&aasa, "example.com", APP, "https://EXAMPLE.COM/x", Match);
    // An empty domain skips the check, for testing a file in isolation.
    expect(&aasa, "", APP, "https://anywhere.test/x", Match);
}

#[test]
fn a_non_https_url_still_matches_but_is_flagged() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/*"}]}]}}"##,
    );
    let result = aasa
        .match_url("example.com", APP, "http://example.com/x")
        .unwrap();
    assert_eq!(result.decision, Match);
    assert!(
        result.notes.iter().any(|note| note.contains("https")),
        "the scheme should be called out: {:?}",
        result.notes
    );
}

#[test]
fn a_document_without_applinks_never_matches() {
    let aasa = compile(r##"{"webcredentials":{"apps":["ABCDE12345.com.example.app"]}}"##);
    let result = aasa
        .match_url("example.com", APP, "https://example.com/x")
        .unwrap();
    assert_eq!(result.trace.stop_reason, StopReason::NoAppLinksSection);
    assert!(aasa.has_webcredential_app(APP));
}

#[test]
fn an_unparseable_url_is_an_error_not_a_no_match() {
    let aasa = compile(r##"{"applinks":{"details":[]}}"##);
    assert!(aasa.match_url("example.com", APP, "not a url").is_err());
    assert!(aasa
        .match_url("example.com", APP, "https:///nohost")
        .is_err());
}

#[test]
fn userinfo_and_ports_do_not_confuse_the_host_check() {
    let aasa = compile(
        r##"{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],
        "components":[{"/":"/x"}]}]}}"##,
    );
    expect(
        &aasa,
        "example.com",
        APP,
        "https://user:pw@example.com:8443/x",
        Match,
    );

    let result = aasa
        .match_url("example.com", APP, "https://example.com:8443/x")
        .unwrap();
    assert!(result.notes.iter().any(|note| note.contains("8443")));
}
