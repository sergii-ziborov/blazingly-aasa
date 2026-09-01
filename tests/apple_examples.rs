//! Every example Apple publishes in the `applinks` reference, turned into an executable
//! expectation. If one of these breaks, the crate has diverged from the documentation.

mod common;

use blazingly_aasa::DiagnosticCode;
use blazingly_aasa::MatchDecision::{Exclude, Match, NoMatch};
use common::{expect, fixture};

const APP: &str = "ABCDE12345.com.example.app";
const APP2: &str = "ABCDE12345.com.example.app2";
const DOMAIN: &str = "example.com";

#[test]
fn overview_example_orders_rules_first_match_wins() {
    let aasa = fixture("apple/applinks-overview.json");

    expect(&aasa, DOMAIN, APP, "https://example.com/buy/42", Match);
    expect(&aasa, DOMAIN, APP2, "https://example.com/buy/42", Match);

    // The exclude on `#` comes first in the array, so it wins over the later /buy/* rule.
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/buy/42#no_universal_links",
        Exclude,
    );

    // /help/website/* excludes before the /help/* rule can accept it.
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/help/website/faq?articleNumber=4815",
        Exclude,
    );

    // "a value of exactly 4 characters"
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/help/123?articleNumber=4815",
        Match,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/help/123?articleNumber=481",
        NoMatch,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/help/123?articleNumber=48159",
        NoMatch,
    );
    // The query item has to be present at all.
    expect(&aasa, DOMAIN, APP, "https://example.com/help/123", NoMatch);

    expect(&aasa, DOMAIN, APP, "https://example.com/elsewhere", NoMatch);

    // A different app is not covered by this details entry.
    expect(
        &aasa,
        DOMAIN,
        "ZZZZZ99999.com.other.app",
        "https://example.com/buy/42",
        NoMatch,
    );
}

#[test]
fn selected_rule_is_reported_by_index() {
    let aasa = fixture("apple/applinks-overview.json");
    let result = aasa
        .match_url(
            DOMAIN,
            APP,
            "https://example.com/help/123?articleNumber=4815",
        )
        .unwrap();
    assert_eq!(result.trace.selected_detail, Some(0));
    assert_eq!(result.trace.selected_rule, Some(3));
    let rule = result.selected_rule().expect("a rule decided this");
    assert!(!rule.exclude);
    assert!(rule.comment.as_deref().unwrap().contains("articleNumber"));
}

#[test]
fn detail_level_defaults_make_matching_case_insensitive() {
    let aasa = fixture("apple/details-case-insensitive.json");

    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/buy/thing#my_great_product_123",
        Match,
    );
    // "ignoring case", per Apple's own comment on this example.
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/BUY/thing#MY_GREAT_PRODUCT_123",
        Match,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/buy/thing#some_other_product",
        NoMatch,
    );

    let result = aasa
        .match_url(
            DOMAIN,
            APP,
            "https://example.com/BUY/x#MY_GREAT_PRODUCT_123",
        )
        .unwrap();
    let rule = result.selected_rule().unwrap();
    assert!(
        !rule.effective.case_sensitive,
        "detail defaults must win over the built-in default"
    );
    assert!(
        rule.effective.percent_encoded,
        "percentEncoded was not overridden"
    );
}

#[test]
fn every_specified_component_must_match() {
    // Apple: "https://www.example.com/abc?def matches, but https://www.example.com/abc and
    // https://www.example.com?def don't." The two negatives are unambiguous, so they are asserted
    // here. The positive is not: Apple writes the path pattern as `abc` while every other example
    // writes `/buy/*`, and a URL path always starts with `/`. See docs/parity.md.
    let aasa = fixture("apple/components-all-must-match.json");

    expect(&aasa, DOMAIN, APP, "https://example.com/abc", NoMatch);
    expect(&aasa, DOMAIN, APP, "https://example.com?def", NoMatch);

    let report = aasa.validate();
    assert!(
        report.contains(DiagnosticCode::PathPatternMissingLeadingSlash),
        "the ambiguity is surfaced as a lint rather than guessed at:\n{report}"
    );
}

#[test]
fn substitution_variables_from_the_reference_example() {
    let aasa = fixture("apple/substitution-variables.json");

    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/en_US/pizza/",
        Match,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/ar_CA/samosa/",
        Match,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/zh_GB/sushi/",
        Match,
    );

    // Not one of the four foods.
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/en_US/tacos/",
        NoMatch,
    );
    // Neither a real ISO language nor a real ISO region.
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/qq_ZZ/pizza/",
        NoMatch,
    );
    // Right pieces, wrong shape.
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/en-US/pizza/",
        NoMatch,
    );
}

#[test]
fn region_table_follows_foundation_not_apples_prose() {
    // Apple's reference describes `$(region)` as "All ISO regions in isoRegionCodes, such as CA,
    // UK, and US" — but `UK` is not an ISO 3166-1 alpha-2 code and Foundation's isoRegionCodes
    // does not contain it. The generated table follows the list Apple points at, not the prose
    // example, so `UK` does not match. Recorded here so the choice is deliberate and visible.
    let aasa = fixture("apple/substitution-variables.json");

    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/en_GB/pizza/",
        Match,
    );
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/en_UK/pizza/",
        NoMatch,
    );
    assert!(blazingly_aasa::ISO_TABLE_SOURCE.contains("Foundation"));
}

#[test]
fn legacy_paths_with_not_exclusions() {
    let aasa = fixture("apple/legacy-paths.json");

    expect(&aasa, DOMAIN, APP, "https://example.com/test/a", Match);
    expect(&aasa, DOMAIN, APP, "https://example.com/path/1/a", Exclude);
    expect(&aasa, DOMAIN, APP, "https://example.com/elsewhere", NoMatch);

    let result = aasa
        .match_url(DOMAIN, APP, "https://example.com/path/1/a")
        .unwrap();
    assert!(result.selected_rule().unwrap().legacy);
}

#[test]
fn legacy_details_dictionary_still_matches_and_is_flagged() {
    let aasa = fixture("apple/legacy-details-dictionary.json");

    expect(&aasa, DOMAIN, APP, "https://example.com/wwdc/news/", Match);
    expect(
        &aasa,
        DOMAIN,
        APP,
        "https://example.com/videos/wwdc/2015/live",
        Match,
    );
    expect(&aasa, DOMAIN, APP, "https://example.com/wwdc/news", NoMatch);

    assert!(aasa
        .validate()
        .contains(DiagnosticCode::LegacyDetailsDictionary));
}

#[test]
fn service_membership_across_all_four_sections() {
    use blazingly_aasa::Service::{ActivityContinuation, AppClips, AppLinks, WebCredentials};
    let aasa = fixture("apple/all-services.json");

    assert_eq!(
        aasa.services_for_app(APP),
        vec![AppLinks, WebCredentials, ActivityContinuation]
    );
    assert_eq!(
        aasa.services_for_app("ABCDE12345.com.example.app.Clip"),
        vec![AppClips]
    );
    assert!(aasa.services_for_app("ZZZZZ99999.com.other.app").is_empty());

    assert!(aasa.has_webcredential_app(APP));
    assert!(!aasa.has_appclip(APP));
}
