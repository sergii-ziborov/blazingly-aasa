//! Baselines and corpora shared by the benchmarks.
//!
//! There is no established Rust crate implementing `apple-app-site-association` semantics to
//! benchmark against, so the comparisons here are against the two implementations a competent
//! engineer would actually reach for:
//!
//! * `RegexAasa` — `serde_json` for parsing plus the `regex` crate for wildcards, translating
//!   `*` to `.*` and `?` to `.`. This is how nearly every AASA checker in the wild is written.
//! * `blazingly_aasa` — this crate.
//!
//! Both sides use `blazingly_aasa::UrlParts` to split the URL, so the numbers reflect the
//! AASA-specific work rather than differences in URL parsing.

#![allow(missing_docs, dead_code)]
#![allow(clippy::format_push_string)] // corpus generation is not on any hot path

use blazingly_aasa::{MatchDecision, UrlParts};
use regex::Regex;

/// A single-file AASA engine built the obvious way: `serde_json` plus `regex`.
pub struct RegexAasa {
    details: Vec<RegexDetail>,
}

struct RegexDetail {
    app_ids: Vec<String>,
    rules: Vec<RegexRule>,
}

struct RegexRule {
    path: Option<Regex>,
    query_whole: Option<Regex>,
    query_items: Vec<(String, Regex)>,
    fragment: Option<Regex>,
    exclude: bool,
}

/// Translates an Apple wildcard pattern into an anchored regular expression.
pub fn to_regex(pattern: &str, case_sensitive: bool) -> Regex {
    let mut out = String::with_capacity(pattern.len() * 2 + 8);
    if !case_sensitive {
        out.push_str("(?i)");
    }
    out.push_str("(?s)^");
    for character in pattern.chars() {
        match character {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            other => {
                if "\\.+()|[]{}^$#&-~".contains(other) {
                    out.push('\\');
                }
                out.push(other);
            }
        }
    }
    out.push('$');
    Regex::new(&out).expect("translated pattern should compile")
}

impl RegexAasa {
    /// Parses and compiles, the `serde_json` + `regex` way.
    pub fn parse(bytes: &[u8]) -> Self {
        let root: serde_json::Value = serde_json::from_slice(bytes).expect("valid JSON");
        let mut details = Vec::new();
        let entries = root
            .get("applinks")
            .and_then(|applinks| applinks.get("details"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();

        for entry in entries {
            let mut app_ids = Vec::new();
            if let Some(app_id) = entry.get("appID").and_then(serde_json::Value::as_str) {
                app_ids.push(app_id.to_owned());
            }
            if let Some(list) = entry.get("appIDs").and_then(serde_json::Value::as_array) {
                app_ids.extend(
                    list.iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned),
                );
            }

            let mut rules = Vec::new();
            for component in entry
                .get("components")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                let case_sensitive = component
                    .get("caseSensitive")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                let mut query_whole = None;
                let mut query_items = Vec::new();
                match component.get("?") {
                    Some(serde_json::Value::String(pattern)) => {
                        query_whole = Some(to_regex(pattern, case_sensitive));
                    }
                    Some(serde_json::Value::Object(items)) => {
                        for (name, value) in items {
                            if let Some(pattern) = value.as_str() {
                                query_items.push((name.clone(), to_regex(pattern, case_sensitive)));
                            }
                        }
                    }
                    _ => {}
                }
                rules.push(RegexRule {
                    path: component
                        .get("/")
                        .and_then(serde_json::Value::as_str)
                        .map(|pattern| to_regex(pattern, case_sensitive)),
                    query_whole,
                    query_items,
                    fragment: component
                        .get("#")
                        .and_then(serde_json::Value::as_str)
                        .map(|pattern| to_regex(pattern, case_sensitive)),
                    exclude: component
                        .get("exclude")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                });
            }
            details.push(RegexDetail { app_ids, rules });
        }
        Self { details }
    }

    /// The same first-match-wins decision this crate makes.
    pub fn decide(&self, app_id: &str, parts: &UrlParts) -> MatchDecision {
        let items = parts.query_items();
        for detail in &self.details {
            if !detail.app_ids.iter().any(|candidate| candidate == app_id) {
                continue;
            }
            for rule in &detail.rules {
                if rule.matches(parts, &items) {
                    return if rule.exclude {
                        MatchDecision::Exclude
                    } else {
                        MatchDecision::Match
                    };
                }
            }
        }
        MatchDecision::NoMatch
    }
}

impl RegexRule {
    fn matches(&self, parts: &UrlParts, items: &[(&str, &str)]) -> bool {
        if let Some(path) = &self.path {
            if !path.is_match(parts.path()) {
                return false;
            }
        }
        if let Some(query) = &self.query_whole {
            if !query.is_match(parts.query()) {
                return false;
            }
        }
        for (name, pattern) in &self.query_items {
            let found = items
                .iter()
                .any(|(candidate, value)| candidate == name && pattern.is_match(value));
            if !found {
                return false;
            }
        }
        if let Some(fragment) = &self.fragment {
            if !fragment.is_match(parts.fragment()) {
                return false;
            }
        }
        true
    }
}

/// Builds a realistic association file with `details` apps and `rules` rules each.
pub fn corpus(details: usize, rules: usize) -> String {
    let mut out = String::from(
        r#"{"applinks":{"substitutionVariables":{"food":["burrito","pizza","sushi","samosa"]},"details":["#,
    );
    for detail in 0..details {
        if detail > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            r#"{{"appIDs":["ABCDE12345.com.example.app{detail}","ABCDE12345.com.example.sibling{detail}"],"components":["#
        ));
        for rule in 0..rules {
            if rule > 0 {
                out.push(',');
            }
            match rule % 5 {
                0 => out.push_str(&format!(
                    r#"{{"/":"/section{rule}/private/*","exclude":true,"comment":"internal only"}}"#
                )),
                1 => out.push_str(&format!(r#"{{"/":"/section{rule}/*"}}"#)),
                2 => out.push_str(&format!(
                    r#"{{"/":"/help{rule}/*","?":{{"articleNumber":"????"}}}}"#
                )),
                3 => out.push_str(&format!(
                    r#"{{"/":"/catalog{rule}/$(food)/*","caseSensitive":false}}"#
                )),
                _ => out.push_str(&format!(r#"{{"/":"/item{rule}/?*","?":"ref=*"}}"#)),
            }
        }
        out.push_str("]}");
    }
    out.push_str(r#"]},"webcredentials":{"apps":["ABCDE12345.com.example.app0"]}}"#);
    out
}

/// A spread of URLs that exercise hits, misses, and exclusions.
pub fn urls() -> Vec<String> {
    vec![
        "https://example.com/section1/product/42".to_owned(),
        "https://example.com/section0/private/secret".to_owned(),
        "https://example.com/help2/topic?articleNumber=4815".to_owned(),
        "https://example.com/help2/topic?articleNumber=481".to_owned(),
        "https://example.com/catalog3/pizza/margherita".to_owned(),
        "https://example.com/item4/x?ref=email".to_owned(),
        "https://example.com/nothing/here".to_owned(),
        "https://example.com/".to_owned(),
    ]
}
