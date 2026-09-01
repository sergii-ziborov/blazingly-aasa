//! Match traces: why a URL matched, was excluded, or was ignored.
//!
//! Every [`MatchResult`](crate::MatchResult) carries a trace. A boolean tells you nothing when a
//! universal link mysteriously stops working; the trace names the detail entry, the rule index,
//! the effective settings that rule ran under, and the exact component that failed.

use crate::model::EffectiveDefaults;
use serde::Serialize;
use std::fmt;

/// The outcome of matching a URL against a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchDecision {
    /// A rule matched and did not exclude the URL.
    Match,
    /// The first matching rule set `exclude: true`.
    Exclude,
    /// No rule matched.
    NoMatch,
}

impl fmt::Display for MatchDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Match => "MATCH",
            Self::Exclude => "BLOCK",
            Self::NoMatch => "NO_MATCH",
        })
    }
}

/// Which part of the URL a component trace refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "component", content = "name")]
pub enum UrlComponent {
    /// The `/` key.
    Path,
    /// The `?` key, matched as one string.
    Query,
    /// One named predicate inside a `?` dictionary.
    QueryItem(String),
    /// The `#` key.
    Fragment,
}

impl fmt::Display for UrlComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path => f.write_str("path"),
            Self::Query => f.write_str("query"),
            Self::QueryItem(name) => write!(f, "query[{name}]"),
            Self::Fragment => f.write_str("fragment"),
        }
    }
}

/// Why one component matched or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ComponentReason {
    /// The rule does not constrain this component, so it accepts anything.
    Unconstrained,
    /// A literal pattern compared equal.
    Exact,
    /// A wildcard pattern matched.
    Wildcard,
    /// A pattern containing `$(...)` matched.
    Substitution,
    /// The pattern did not match.
    PatternMismatch,
    /// The pattern would have matched if `caseSensitive` were `false`.
    CaseMismatch,
    /// A `?` dictionary named a query item the URL does not carry.
    MissingQueryItem,
    /// A `?` predicate was not a string, so it can never match.
    UnsupportedPredicate,
}

impl ComponentReason {
    /// Whether this reason represents a successful comparison.
    #[must_use]
    pub fn is_match(self) -> bool {
        matches!(
            self,
            Self::Unconstrained | Self::Exact | Self::Wildcard | Self::Substitution
        )
    }
}

impl fmt::Display for ComponentReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unconstrained => "not constrained by this rule",
            Self::Exact => "literal match",
            Self::Wildcard => "wildcard match",
            Self::Substitution => "substitution match",
            Self::PatternMismatch => "pattern did not match",
            Self::CaseMismatch => "differs only by letter case",
            Self::MissingQueryItem => "query item is missing",
            Self::UnsupportedPredicate => "predicate is not a string pattern",
        })
    }
}

/// One component comparison inside a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentTrace {
    /// Which URL component was compared.
    #[serde(flatten)]
    pub component: UrlComponent,
    /// The pattern as written in the document, when the rule specified one.
    pub pattern: Option<String>,
    /// The value taken from the URL, after any percent-decoding the rule asked for.
    pub input: String,
    /// Whether this component accepted the URL.
    pub matched: bool,
    /// Why.
    pub reason: ComponentReason,
}

/// One rule evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleTrace {
    /// Index of the owning entry in `applinks.details`.
    pub detail_index: usize,
    /// Index of this rule inside the entry, counting `components` or legacy `paths`.
    pub rule_index: usize,
    /// Whether this rule came from legacy `paths` rather than `components`.
    pub legacy: bool,
    /// Whether the rule carries `exclude: true`.
    pub exclude: bool,
    /// The rule's `comment`, when it has one.
    pub comment: Option<String>,
    /// The settings this rule ran under, after resolving the defaults hierarchy.
    pub effective: EffectiveDefaults,
    /// Every component this rule compared.
    pub components: Vec<ComponentTrace>,
    /// Whether every specified component matched.
    pub matched: bool,
}

impl RuleTrace {
    /// How many specified components matched, used to pick the closest near-miss.
    #[must_use]
    pub fn matched_component_count(&self) -> usize {
        self.components
            .iter()
            .filter(|component| {
                component.matched && component.reason != ComponentReason::Unconstrained
            })
            .count()
    }
}

/// One `applinks.details` entry considered during matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetailTrace {
    /// Index in `applinks.details`.
    pub index: usize,
    /// The application identifiers this entry declares.
    pub app_ids: Vec<String>,
    /// Whether the entry lists the application identifier under test.
    pub applies: bool,
    /// Rules evaluated, empty when the entry does not apply.
    pub rules: Vec<RuleTrace>,
}

/// Why matching stopped where it did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "stop")]
#[non_exhaustive]
pub enum StopReason {
    /// A rule matched.
    Matched,
    /// A rule matched and excluded the URL.
    Excluded,
    /// The document has no `applinks` section.
    NoAppLinksSection,
    /// No `applinks.details` entry lists this application identifier.
    NoApplicableDetail,
    /// Entries applied, but none of their rules matched.
    NoRuleMatched,
    /// The URL's host is not the domain this document was served for.
    HostMismatch {
        /// The domain the document was served for.
        expected: String,
        /// The host in the URL.
        actual: String,
    },
}

/// The full record of a match attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchTrace {
    /// Every `applinks.details` entry, in source order.
    pub details: Vec<DetailTrace>,
    /// The detail index of the deciding rule, when there was one.
    pub selected_detail: Option<usize>,
    /// The rule index of the deciding rule, when there was one.
    pub selected_rule: Option<usize>,
    /// Why matching stopped.
    pub stop_reason: StopReason,
    /// The rule that came closest, when nothing matched.
    pub closest_failure: Option<RuleTrace>,
}

/// The result of matching one URL for one application identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchResult {
    /// Match, exclude, or no match.
    pub decision: MatchDecision,
    /// The domain the document was served for.
    pub domain: String,
    /// The application identifier under test.
    pub app_id: String,
    /// The URL under test.
    pub url: String,
    /// Why.
    pub trace: MatchTrace,
    /// Context-level warnings that do not affect the decision, such as a non-HTTPS scheme.
    pub notes: Vec<String>,
}

impl MatchResult {
    /// Whether the document considers this URL openable by this app.
    #[must_use]
    pub fn is_match(&self) -> bool {
        self.decision == MatchDecision::Match
    }

    /// The deciding rule, when the decision came from one.
    #[must_use]
    pub fn selected_rule(&self) -> Option<&RuleTrace> {
        let detail = self.trace.selected_detail?;
        let rule = self.trace.selected_rule?;
        self.trace
            .details
            .iter()
            .find(|entry| entry.index == detail)?
            .rules
            .iter()
            .find(|candidate| candidate.rule_index == rule)
    }
}

impl fmt::Display for MatchResult {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.decision)?;
        writeln!(f)?;
        writeln!(f, "application: {}", self.app_id)?;
        writeln!(f, "domain:      {}", self.domain)?;
        writeln!(f, "url:         {}", self.url)?;

        if let Some(rule) = self.selected_rule() {
            writeln!(f)?;
            writeln!(f, "detail:      #{}", rule.detail_index)?;
            writeln!(
                f,
                "rule:        #{}{}",
                rule.rule_index,
                if rule.legacy { " (legacy paths)" } else { "" }
            )?;
            if let Some(comment) = &rule.comment {
                writeln!(f, "comment:     {comment}")?;
            }
            writeln!(f)?;
            write_components(f, rule)?;
            writeln!(f)?;
            writeln!(
                f,
                "effective settings:\n  caseSensitive  = {}\n  percentEncoded = {}",
                rule.effective.case_sensitive, rule.effective.percent_encoded
            )?;
        }

        writeln!(f)?;
        writeln!(f, "reason:")?;
        match &self.trace.stop_reason {
            StopReason::Matched => {
                writeln!(f, "  every component this rule specifies matched")?;
            }
            StopReason::Excluded => {
                writeln!(
                    f,
                    "  the first matching rule sets exclude: true, so matching stopped"
                )?;
            }
            StopReason::NoAppLinksSection => {
                writeln!(f, "  the document has no applinks section")?;
            }
            StopReason::NoApplicableDetail => {
                writeln!(f, "  no applinks.details entry lists {}", self.app_id)?;
            }
            StopReason::NoRuleMatched => {
                writeln!(
                    f,
                    "  the entries that apply to {} have no rule matching this URL",
                    self.app_id
                )?;
            }
            StopReason::HostMismatch { expected, actual } => {
                writeln!(
                    f,
                    "  the URL host is {actual}, but this document was served for {expected}"
                )?;
            }
        }

        if let Some(closest) = &self.trace.closest_failure {
            writeln!(f)?;
            writeln!(
                f,
                "closest failure:\n  detail #{}, rule #{}",
                closest.detail_index, closest.rule_index
            )?;
            write_components(f, closest)?;
        }

        for note in &self.notes {
            writeln!(f, "\nnote: {note}")?;
        }
        Ok(())
    }
}

fn write_components(f: &mut fmt::Formatter<'_>, rule: &RuleTrace) -> fmt::Result {
    for component in &rule.components {
        if component.reason == ComponentReason::Unconstrained {
            continue;
        }
        let mark = if component.matched { "ok  " } else { "FAIL" };
        writeln!(f, "  [{mark}] {}", component.component)?;
        writeln!(f, "         url:     {}", component.input)?;
        if let Some(pattern) = &component.pattern {
            writeln!(f, "         pattern: {pattern}")?;
        }
        writeln!(f, "         {}", component.reason)?;
    }
    Ok(())
}
