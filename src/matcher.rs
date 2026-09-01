//! Evaluating a URL against a compiled document.
//!
//! The result says what the *document* considers eligible. It deliberately does not claim what a
//! device will do: that also depends on whether the app is installed, what its Associated Domains
//! entitlement says, what Apple's CDN currently serves, and how the user got to the link.
//!
//! There are two entry points. [`CompiledAasa::decide`] answers the question and allocates almost
//! nothing. [`CompiledAasa::match_url`] answers it and builds a full [`MatchTrace`], which costs a
//! string per compared component. Use the first in a hot loop and the second when a human needs to
//! understand the answer.

use crate::compile::{CompiledAasa, CompiledQuery, CompiledRule};
use crate::error::UrlError;
use crate::explain::{
    ComponentReason, ComponentTrace, DetailTrace, MatchDecision, MatchResult, MatchTrace,
    RuleTrace, StopReason, UrlComponent,
};
use crate::model::EffectiveDefaults;
use crate::pattern::{Pattern, Shape};
use crate::url::{percent_decode, UrlParts};

/// Query items, either borrowed straight out of the URL or owned after percent-decoding.
#[derive(Clone, Copy)]
enum Items<'a> {
    Encoded(&'a [(&'a str, &'a str)]),
    Decoded(&'a [(String, String)]),
}

impl<'a> Items<'a> {
    fn len(self) -> usize {
        match self {
            Self::Encoded(items) => items.len(),
            Self::Decoded(items) => items.len(),
        }
    }

    fn get(self, index: usize) -> (&'a str, &'a str) {
        match self {
            Self::Encoded(items) => items[index],
            Self::Decoded(items) => (items[index].0.as_str(), items[index].1.as_str()),
        }
    }
}

/// The percent-decoded forms, built only when some rule actually asks for them.
struct Decoded {
    path: String,
    query: String,
    fragment: String,
    items: Vec<(String, String)>,
}

/// The URL components a rule can be compared against.
struct Inputs<'a> {
    path: &'a str,
    query: &'a str,
    fragment: &'a str,
    items: Vec<(&'a str, &'a str)>,
    decoded: Option<Decoded>,
}

impl<'a> Inputs<'a> {
    fn new(parts: &'a UrlParts, needs_decoded: bool) -> Self {
        let items = parts.query_items();
        let decoded = needs_decoded.then(|| Decoded {
            path: percent_decode(parts.path()),
            query: percent_decode(parts.query()),
            fragment: percent_decode(parts.fragment()),
            items: items
                .iter()
                .map(|(name, value)| (percent_decode(name), percent_decode(value)))
                .collect(),
        });
        Self {
            path: parts.path(),
            query: parts.query(),
            fragment: parts.fragment(),
            items,
            decoded,
        }
    }

    fn path_for(&self, percent_encoded: bool) -> &str {
        match (percent_encoded, &self.decoded) {
            (false, Some(decoded)) => &decoded.path,
            _ => self.path,
        }
    }

    fn query_for(&self, percent_encoded: bool) -> &str {
        match (percent_encoded, &self.decoded) {
            (false, Some(decoded)) => &decoded.query,
            _ => self.query,
        }
    }

    fn fragment_for(&self, percent_encoded: bool) -> &str {
        match (percent_encoded, &self.decoded) {
            (false, Some(decoded)) => &decoded.fragment,
            _ => self.fragment,
        }
    }

    fn items_for(&self, percent_encoded: bool) -> Items<'_> {
        match (percent_encoded, &self.decoded) {
            (false, Some(decoded)) => Items::Decoded(&decoded.items),
            _ => Items::Encoded(&self.items),
        }
    }
}

/// Everything the two evaluation paths need to agree on before rules are consulted.
enum Preflight {
    Proceed,
    Stop(StopReason),
}

fn preflight(aasa: &CompiledAasa, domain: &str, parts: &UrlParts) -> Preflight {
    if !domain.is_empty() && !domain.eq_ignore_ascii_case(parts.host()) {
        return Preflight::Stop(StopReason::HostMismatch {
            expected: domain.to_owned(),
            actual: parts.host().to_owned(),
        });
    }
    if !aasa.has_applinks {
        return Preflight::Stop(StopReason::NoAppLinksSection);
    }
    Preflight::Proceed
}

fn context_notes(parts: &UrlParts) -> Vec<String> {
    let mut notes = Vec::new();
    if parts.scheme() != "https" {
        notes.push(format!(
            "the URL scheme is `{}`; Apple serves and matches universal links over https only",
            parts.scheme()
        ));
    }
    if let Some(port) = parts.port() {
        notes.push(format!(
            "the URL carries an explicit port ({port}); whether a port is allowed is decided by \
             the app's Associated Domains entitlement, not by this file"
        ));
    }
    notes
}

impl CompiledAasa {
    /// Decides whether this document lets `app_id` open `url` on `domain`, without building a
    /// trace.
    ///
    /// This is the hot-loop entry point: it walks the same rules as [`CompiledAasa::match_url`]
    /// and reaches the same conclusion, but allocates only what URL splitting requires.
    ///
    /// Pass an empty `domain` to skip the host check.
    ///
    /// # Errors
    ///
    /// Returns [`UrlError`] when `url` cannot be split into scheme, host, and path.
    pub fn decide(&self, domain: &str, app_id: &str, url: &str) -> Result<MatchDecision, UrlError> {
        let parts = UrlParts::parse(url)?;
        Ok(self.decide_parts(domain, app_id, &parts))
    }

    /// The same decision, for a URL that has already been split.
    #[must_use]
    pub fn decide_parts(&self, domain: &str, app_id: &str, parts: &UrlParts) -> MatchDecision {
        if let Preflight::Stop(_) = preflight(self, domain, parts) {
            return MatchDecision::NoMatch;
        }
        let inputs = Inputs::new(parts, self.needs_decoded);
        for detail in &self.details {
            if !detail.applies_to(app_id) {
                continue;
            }
            for rule in &detail.rules {
                if rule_matches(rule, &inputs) {
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

    /// Decides, and records why.
    ///
    /// Pass an empty `domain` to skip the host check, for example when testing a file in
    /// isolation.
    ///
    /// # Errors
    ///
    /// Returns [`UrlError`] when `url` cannot be split into scheme, host, and path.
    pub fn match_url(
        &self,
        domain: &str,
        app_id: &str,
        url: &str,
    ) -> Result<MatchResult, UrlError> {
        let parts = UrlParts::parse(url)?;
        Ok(self.match_parts(domain, app_id, &parts, url))
    }

    /// The same, for a URL that has already been split. `url_text` is echoed into the result.
    #[must_use]
    pub fn match_parts(
        &self,
        domain: &str,
        app_id: &str,
        parts: &UrlParts,
        url_text: &str,
    ) -> MatchResult {
        let mut result = MatchResult {
            decision: MatchDecision::NoMatch,
            domain: domain.to_owned(),
            app_id: app_id.to_owned(),
            url: url_text.to_owned(),
            trace: MatchTrace {
                details: Vec::new(),
                selected_detail: None,
                selected_rule: None,
                stop_reason: StopReason::NoRuleMatched,
                closest_failure: None,
            },
            notes: context_notes(parts),
        };

        if let Preflight::Stop(reason) = preflight(self, domain, parts) {
            result.trace.stop_reason = reason;
            return result;
        }

        let inputs = Inputs::new(parts, self.needs_decoded);
        let mut any_applicable = false;
        let mut closest: Option<RuleTrace> = None;

        'outer: for detail in &self.details {
            let applies = detail.applies_to(app_id);
            any_applicable |= applies;
            let mut detail_trace = DetailTrace {
                index: detail.index,
                app_ids: detail.app_ids.clone(),
                applies,
                rules: Vec::new(),
            };

            if applies {
                for rule in &detail.rules {
                    let trace = evaluate(rule, &inputs);
                    let matched = trace.matched;
                    if !matched {
                        // Keep the rule that got furthest. Ties keep the earlier rule, which is
                        // the one a reader will look at first.
                        let better = closest.as_ref().map_or(true, |current| {
                            trace.matched_component_count() > current.matched_component_count()
                        });
                        if better {
                            closest = Some(trace.clone());
                        }
                    }
                    detail_trace.rules.push(trace);
                    if matched {
                        result.decision = if rule.exclude {
                            MatchDecision::Exclude
                        } else {
                            MatchDecision::Match
                        };
                        result.trace.stop_reason = if rule.exclude {
                            StopReason::Excluded
                        } else {
                            StopReason::Matched
                        };
                        result.trace.selected_detail = Some(rule.detail_index);
                        result.trace.selected_rule = Some(rule.rule_index);
                        result.trace.details.push(detail_trace);
                        break 'outer;
                    }
                }
            }
            result.trace.details.push(detail_trace);
        }

        if result.decision == MatchDecision::NoMatch {
            result.trace.stop_reason = if any_applicable {
                StopReason::NoRuleMatched
            } else {
                StopReason::NoApplicableDetail
            };
            result.trace.closest_failure = closest;
        }
        result
    }
}

/// The trace-free evaluation. Short-circuits on the first failing component.
fn rule_matches(rule: &CompiledRule, inputs: &Inputs<'_>) -> bool {
    let effective = rule.effective;
    let case_sensitive = effective.case_sensitive;

    if let Some(pattern) = &rule.path {
        if !pattern.matches_with(inputs.path_for(effective.percent_encoded), case_sensitive) {
            return false;
        }
    }

    match &rule.query {
        None => {}
        Some(CompiledQuery::Whole(pattern)) => {
            if !pattern.matches_with(inputs.query_for(effective.percent_encoded), case_sensitive) {
                return false;
            }
        }
        Some(CompiledQuery::Items(predicates)) => {
            let items = inputs.items_for(effective.percent_encoded);
            for (name, pattern) in predicates {
                let Some(pattern) = pattern else { return false };
                if !query_item_matches(name, pattern, items, case_sensitive) {
                    return false;
                }
            }
        }
    }

    if let Some(pattern) = &rule.fragment {
        if !pattern.matches_with(
            inputs.fragment_for(effective.percent_encoded),
            case_sensitive,
        ) {
            return false;
        }
    }
    true
}

fn query_item_matches(
    name: &str,
    pattern: &Pattern,
    items: Items<'_>,
    case_sensitive: bool,
) -> bool {
    for index in 0..items.len() {
        let (candidate, value) = items.get(index);
        let same_name = if case_sensitive {
            candidate == name
        } else {
            candidate.eq_ignore_ascii_case(name)
        };
        if same_name && pattern.matches_with(value, case_sensitive) {
            return true;
        }
    }
    false
}

fn evaluate(rule: &CompiledRule, inputs: &Inputs<'_>) -> RuleTrace {
    let effective = rule.effective;
    let mut components = Vec::new();

    components.push(compare(
        UrlComponent::Path,
        rule.path.as_ref(),
        inputs.path_for(effective.percent_encoded),
        effective,
    ));

    match &rule.query {
        None => components.push(compare(
            UrlComponent::Query,
            None,
            inputs.query_for(effective.percent_encoded),
            effective,
        )),
        Some(CompiledQuery::Whole(pattern)) => components.push(compare(
            UrlComponent::Query,
            Some(pattern),
            inputs.query_for(effective.percent_encoded),
            effective,
        )),
        Some(CompiledQuery::Items(predicates)) => {
            let items = inputs.items_for(effective.percent_encoded);
            for (name, pattern) in predicates {
                components.push(compare_query_item(name, pattern.as_ref(), items, effective));
            }
        }
    }

    components.push(compare(
        UrlComponent::Fragment,
        rule.fragment.as_ref(),
        inputs.fragment_for(effective.percent_encoded),
        effective,
    ));

    let matched = components.iter().all(|component| component.matched);
    RuleTrace {
        detail_index: rule.detail_index,
        rule_index: rule.rule_index,
        legacy: rule.legacy,
        exclude: rule.exclude,
        comment: rule.comment.clone(),
        effective,
        components,
        matched,
    }
}

fn compare(
    component: UrlComponent,
    pattern: Option<&Pattern>,
    input: &str,
    effective: EffectiveDefaults,
) -> ComponentTrace {
    let Some(pattern) = pattern else {
        return ComponentTrace {
            component,
            pattern: None,
            input: input.to_owned(),
            matched: true,
            reason: ComponentReason::Unconstrained,
        };
    };
    let (matched, reason) = decide_component(pattern, input, effective.case_sensitive);
    ComponentTrace {
        component,
        pattern: Some(pattern.source().to_owned()),
        input: input.to_owned(),
        matched,
        reason,
    }
}

fn compare_query_item(
    name: &str,
    pattern: Option<&Pattern>,
    items: Items<'_>,
    effective: EffectiveDefaults,
) -> ComponentTrace {
    let component = UrlComponent::QueryItem(name.to_owned());
    let mut present: Vec<&str> = Vec::new();
    for index in 0..items.len() {
        let (candidate, value) = items.get(index);
        let same_name = if effective.case_sensitive {
            candidate == name
        } else {
            candidate.eq_ignore_ascii_case(name)
        };
        if same_name {
            present.push(value);
        }
    }

    if present.is_empty() {
        return ComponentTrace {
            component,
            pattern: pattern.map(|pattern| pattern.source().to_owned()),
            input: String::new(),
            matched: false,
            reason: ComponentReason::MissingQueryItem,
        };
    }

    let Some(pattern) = pattern else {
        return ComponentTrace {
            component,
            pattern: None,
            input: present[0].to_owned(),
            matched: false,
            reason: ComponentReason::UnsupportedPredicate,
        };
    };

    // A query may repeat a name; Apple does not document which wins, so any match counts.
    let mut best: Option<(bool, ComponentReason, String)> = None;
    for value in present {
        let (matched, reason) = decide_component(pattern, value, effective.case_sensitive);
        if matched {
            best = Some((true, reason, value.to_owned()));
            break;
        }
        if best.is_none() || reason == ComponentReason::CaseMismatch {
            best = Some((false, reason, value.to_owned()));
        }
    }
    let (matched, reason, input) =
        best.unwrap_or((false, ComponentReason::PatternMismatch, String::new()));
    ComponentTrace {
        component,
        pattern: Some(pattern.source().to_owned()),
        input,
        matched,
        reason,
    }
}

fn decide_component(
    pattern: &Pattern,
    input: &str,
    case_sensitive: bool,
) -> (bool, ComponentReason) {
    if pattern.matches_with(input, case_sensitive) {
        let reason = match pattern.shape() {
            Shape::Any | Shape::Wildcard => ComponentReason::Wildcard,
            Shape::Literal => ComponentReason::Exact,
            Shape::Substitution => ComponentReason::Substitution,
        };
        return (true, reason);
    }
    if case_sensitive && pattern.matches_with(input, false) {
        return (false, ComponentReason::CaseMismatch);
    }
    (false, ComponentReason::PatternMismatch)
}
