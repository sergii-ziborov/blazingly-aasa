//! Deciding whether a URL opens an app, and showing the work.
//!
//! ```text
//! cargo run --example matching
//! ```

use blazingly_aasa::{CompiledAasa, MatchDecision};

const DOCUMENT: &[u8] = br#"{
  "applinks": {
    "details": [{
      "appIDs": ["ABCDE12345.com.example.app"],
      "components": [
        { "/": "/help/website/*", "exclude": true, "comment": "the web help stays on the web" },
        { "/": "/help/*", "?": { "articleNumber": "????" } }
      ]
    }]
  }
}"#;

const APP: &str = "ABCDE12345.com.example.app";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let aasa = CompiledAasa::parse(DOCUMENT)?;

    // `decide` is the fast path: a decision, nothing else.
    for url in [
        "https://example.com/help/1?articleNumber=4815",
        "https://example.com/help/1?articleNumber=481",
        "https://example.com/help/website/faq",
        "https://example.com/store",
    ] {
        println!(
            "{:<8} {url}",
            aasa.decide("example.com", APP, url)?.to_string()
        );
    }

    // `match_url` costs more and answers "why". A near miss is the interesting case.
    println!();
    let miss = aasa.match_url(
        "example.com",
        APP,
        "https://example.com/help/1?articleNumber=481",
    )?;
    assert_eq!(miss.decision, MatchDecision::NoMatch);
    println!("{miss}");

    // The other direction: which apps does this URL reach?
    println!();
    for (app_id, decision) in aasa.apps_for_url(
        "example.com",
        "https://example.com/help/1?articleNumber=4815",
    )? {
        println!("{app_id}: {decision}");
    }
    Ok(())
}
