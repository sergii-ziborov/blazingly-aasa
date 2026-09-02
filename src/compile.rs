//! Normalisation: turning the wire model into something matchable and comparable.
//!
//! Compilation resolves the three-level defaults hierarchy, merges `appID` and `appIDs`, expands
//! `$(...)` references, and compiles every pattern — while preserving rule order and source
//! indices, because both are semantically load-bearing.

use crate::diagnostics::{Diagnostic, DiagnosticCode, ValidationReport};
use crate::error::ParseError;
use crate::model::{
    AasaDocument, AppLinkDetail, AppLinks, ComponentRule, EffectiveDefaults, QueryPredicate,
    QueryRule,
};
use crate::parse::ParseOptions;
use crate::pattern::{Pattern, PatternError};
use crate::substitution::SubstitutionTable;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const SERVICES: [Service; 4] = [
    Service::AppLinks,
    Service::WebCredentials,
    Service::AppClips,
    Service::ActivityContinuation,
];

/// An Associated Domains service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    /// Universal links.
    AppLinks,
    /// Shared web credentials.
    WebCredentials,
    /// App Clips.
    AppClips,
    /// Handoff.
    ActivityContinuation,
}

impl Service {
    /// The key this service uses in the association file.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::AppLinks => "applinks",
            Self::WebCredentials => "webcredentials",
            Self::AppClips => "appclips",
            Self::ActivityContinuation => "activitycontinuation",
        }
    }
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

/// A `?` constraint reduced to its comparable form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveQuery {
    /// One pattern matched against the whole query string.
    Whole(String),
    /// Named predicates, all of which must hold. `None` marks a predicate that can never match.
    Items(BTreeMap<String, Option<String>>),
}

/// A rule reduced to exactly what decides matching.
///
/// Two rules with the same [`EffectiveRule`] behave identically, no matter how the document
/// distributed `caseSensitive` and `percentEncoded` across the defaults hierarchy. This is what
/// makes [`semantic_diff`](crate::CompiledAasa::semantic_diff) able to see past a refactor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
// Four booleans is exactly what an Apple rule carries; collapsing them into enums would obscure
// the mapping to the file format without making anything safer.
#[allow(clippy::struct_excessive_bools)]
pub struct EffectiveRule {
    /// Whether the rule blocks the URL.
    pub exclude: bool,
    /// Whether the rule came from legacy `paths`.
    pub legacy: bool,
    /// The `/` pattern, or `None` when unconstrained.
    pub path: Option<String>,
    /// The `?` constraint, or `None` when unconstrained.
    pub query: Option<EffectiveQuery>,
    /// The `#` pattern, or `None` when unconstrained.
    pub fragment: Option<String>,
    /// Effective `caseSensitive`.
    pub case_sensitive: bool,
    /// Effective `percentEncoded`.
    pub percent_encoded: bool,
}

impl std::fmt::Display for EffectiveRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(path) = &self.path {
            parts.push(format!("/ = {path}"));
        }
        match &self.query {
            Some(EffectiveQuery::Whole(pattern)) => parts.push(format!("? = {pattern}")),
            Some(EffectiveQuery::Items(items)) => {
                let rendered = items
                    .iter()
                    .map(|(key, value)| match value {
                        Some(pattern) => format!("{key}={pattern}"),
                        None => format!("{key}=<unsupported>"),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                parts.push(format!("? = {{{rendered}}}"));
            }
            None => {}
        }
        if let Some(fragment) = &self.fragment {
            parts.push(format!("# = {fragment}"));
        }
        if parts.is_empty() {
            parts.push("<matches every URL>".to_owned());
        }
        if self.exclude {
            parts.push("exclude".to_owned());
        }
        parts.push(format!("caseSensitive={}", self.case_sensitive));
        parts.push(format!("percentEncoded={}", self.percent_encoded));
        if self.legacy {
            parts.push("legacy".to_owned());
        }
        f.write_str(&parts.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompiledQuery {
    Whole(Pattern),
    Items(Vec<(String, Pattern)>),
    /// A dictionary holding at least one non-string predicate.
    ///
    /// `swcutil` ignores the whole dictionary in that case rather than the offending entry, so a
    /// single `"flag": true` silently drops every constraint beside it. This matches that, and
    /// `AASA150` reports it as an error because of how much it quietly opens up.
    IgnoredDictionary(Vec<String>),
}

/// A compiled `/` pattern together with what its shape allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledPath {
    pub(crate) pattern: Pattern,
    /// The same pattern minus a trailing `/*`, compiled only when the pattern has one.
    pub(crate) parent: Option<Pattern>,
    /// Whether the path must also be tried without its leading slash. False for the usual
    /// `/`-rooted pattern, which can never match a path that lacks one.
    pub(crate) try_bare_path: bool,
}

impl CompiledPath {
    /// Whether any form of this pattern matches any form of the path.
    pub(crate) fn matches(&self, trimmed: &str, bare: Option<&str>, case_sensitive: bool) -> bool {
        for pattern in std::iter::once(&self.pattern).chain(self.parent.as_ref()) {
            if pattern.matches_with(trimmed, case_sensitive) {
                return true;
            }
            if self.try_bare_path {
                if let Some(bare) = bare {
                    if pattern.matches_with(bare, case_sensitive) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn source(&self) -> &str {
        self.pattern.source()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledRule {
    pub(crate) detail_index: usize,
    pub(crate) rule_index: usize,
    pub(crate) legacy: bool,
    pub(crate) exclude: bool,
    pub(crate) effective: EffectiveDefaults,
    pub(crate) path: Option<CompiledPath>,
    pub(crate) query: Option<CompiledQuery>,
    pub(crate) fragment: Option<Pattern>,
    pub(crate) comment: Option<String>,
}

impl CompiledRule {
    /// Whether the rule constrains nothing and so accepts every URL.
    pub(crate) fn is_unconstrained(&self) -> bool {
        let path_any = self
            .path
            .as_ref()
            .map_or(true, |path| path.pattern.is_any());
        let fragment_any = self.fragment.as_ref().map_or(true, Pattern::is_any);
        let query_any = match &self.query {
            None | Some(CompiledQuery::IgnoredDictionary(_)) => true,
            Some(CompiledQuery::Whole(pattern)) => pattern.is_any(),
            Some(CompiledQuery::Items(items)) => items.is_empty(),
        };
        path_any && fragment_any && query_any
    }

    pub(crate) fn effective_rule(&self) -> EffectiveRule {
        let query = self.query.as_ref().map(|query| match query {
            CompiledQuery::Whole(pattern) => EffectiveQuery::Whole(pattern.source().to_owned()),
            CompiledQuery::Items(items) => EffectiveQuery::Items(
                items
                    .iter()
                    .map(|(key, pattern)| (key.clone(), Some(pattern.source().to_owned())))
                    .collect(),
            ),
            CompiledQuery::IgnoredDictionary(keys) => {
                EffectiveQuery::Items(keys.iter().map(|key| (key.clone(), None)).collect())
            }
        });
        let path = self.path.as_ref().map(|path| path.source().to_owned());
        let fragment = self.fragment.as_ref().map(|p| p.source().to_owned());
        // A rule that constrains nothing behaves identically regardless of its flags, so normalise
        // them away rather than reporting a spurious difference.
        let unconstrained = self.is_unconstrained();
        EffectiveRule {
            exclude: self.exclude,
            legacy: self.legacy,
            path,
            query,
            fragment,
            case_sensitive: !unconstrained && self.effective.case_sensitive,
            percent_encoded: !unconstrained && self.effective.percent_encoded,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledDetail {
    pub(crate) index: usize,
    pub(crate) app_ids: Vec<String>,
    pub(crate) rules: Vec<CompiledRule>,
}

impl CompiledDetail {
    pub(crate) fn applies_to(&self, app_id: &str) -> bool {
        self.app_ids.iter().any(|candidate| candidate == app_id)
    }
}

/// A document normalised for matching, explaining, and comparing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledAasa {
    pub(crate) document: AasaDocument,
    pub(crate) details: Vec<CompiledDetail>,
    pub(crate) has_applinks: bool,
    pub(crate) webcredentials: Vec<String>,
    pub(crate) appclips: Vec<String>,
    pub(crate) activitycontinuation: Vec<String>,
    pub(crate) substitution_variables: BTreeMap<String, Vec<String>>,
    pub(crate) compile_diagnostics: Vec<Diagnostic>,
    /// Whether any rule turns `percentEncoded` off. When nothing does, matching never pays for
    /// percent-decoding the URL.
    pub(crate) needs_decoded: bool,
    /// Whether any rule uses a `?` dictionary. When none does, the query is never split into items.
    pub(crate) needs_query_items: bool,
}

impl CompiledAasa {
    /// Parses and compiles in one step.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] for invalid JSON, a non-object root, or an oversized payload.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        Ok(AasaDocument::parse(bytes)?.compile())
    }

    /// Parses and compiles with explicit limits.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] for invalid JSON, a non-object root, or an oversized payload.
    pub fn parse_with(bytes: &[u8], options: &ParseOptions) -> Result<Self, ParseError> {
        Ok(AasaDocument::parse_with(bytes, options)?.compile())
    }

    /// The wire model this was compiled from.
    #[must_use]
    pub fn document(&self) -> &AasaDocument {
        &self.document
    }

    /// Whether the document declared an `applinks` section at all.
    #[must_use]
    pub fn has_applinks(&self) -> bool {
        self.has_applinks
    }

    /// Every application identifier that appears in `applinks.details`, deduplicated and sorted.
    #[must_use]
    pub fn applink_apps(&self) -> Vec<&str> {
        let set: BTreeSet<&str> = self
            .details
            .iter()
            .flat_map(|detail| detail.app_ids.iter().map(String::as_str))
            .collect();
        set.into_iter().collect()
    }

    /// Whether `app_id` can open universal links for this domain, ignoring any specific URL.
    #[must_use]
    pub fn has_applink_app(&self, app_id: &str) -> bool {
        self.details.iter().any(|detail| detail.applies_to(app_id))
    }

    /// Whether `app_id` is listed under `webcredentials`.
    #[must_use]
    pub fn has_webcredential_app(&self, app_id: &str) -> bool {
        self.webcredentials.iter().any(|entry| entry == app_id)
    }

    /// Whether `app_id` is listed under `appclips`.
    #[must_use]
    pub fn has_appclip(&self, app_id: &str) -> bool {
        self.appclips.iter().any(|entry| entry == app_id)
    }

    /// Whether `app_id` is listed under `activitycontinuation`.
    #[must_use]
    pub fn has_activitycontinuation_app(&self, app_id: &str) -> bool {
        self.activitycontinuation
            .iter()
            .any(|entry| entry == app_id)
    }

    /// Every service this domain grants the app built from a team prefix and bundle identifier.
    ///
    /// Convenience for callers that hold the two halves separately — Xcode shows them apart, and
    /// so do most validators.
    #[must_use]
    pub fn services_for_bundle(&self, team_id: &str, bundle_id: &str) -> Vec<Service> {
        self.services_for_app(&format!("{team_id}.{bundle_id}"))
    }

    /// Every application identifier in the document whose bundle identifier is `bundle_id`,
    /// whatever its team prefix.
    ///
    /// Useful when an app moved between teams and you want to know which prefix the file still
    /// names.
    #[must_use]
    pub fn app_ids_for_bundle(&self, bundle_id: &str) -> Vec<&str> {
        let mut found: Vec<&str> = Vec::new();
        for service in SERVICES {
            for app_id in self.apps_for_service(service) {
                if crate::split_app_id(app_id).is_some_and(|(_, bundle)| bundle == bundle_id)
                    && !found.contains(&app_id)
                {
                    found.push(app_id);
                }
            }
        }
        found.sort_unstable();
        found
    }

    /// Every service this domain grants `app_id`.
    #[must_use]
    pub fn services_for_app(&self, app_id: &str) -> Vec<Service> {
        let mut services = Vec::new();
        if self.has_applink_app(app_id) {
            services.push(Service::AppLinks);
        }
        if self.has_webcredential_app(app_id) {
            services.push(Service::WebCredentials);
        }
        if self.has_appclip(app_id) {
            services.push(Service::AppClips);
        }
        if self.has_activitycontinuation_app(app_id) {
            services.push(Service::ActivityContinuation);
        }
        services
    }

    /// The apps listed for one service.
    #[must_use]
    pub fn apps_for_service(&self, service: Service) -> Vec<&str> {
        match service {
            Service::AppLinks => self.applink_apps(),
            Service::WebCredentials => self.webcredentials.iter().map(String::as_str).collect(),
            Service::AppClips => self.appclips.iter().map(String::as_str).collect(),
            Service::ActivityContinuation => self
                .activitycontinuation
                .iter()
                .map(String::as_str)
                .collect(),
        }
    }

    /// The ordered rules that apply to `app_id`, reduced to their deciding form.
    #[must_use]
    pub fn effective_rules_for(&self, app_id: &str) -> Vec<EffectiveRule> {
        self.details
            .iter()
            .filter(|detail| detail.applies_to(app_id))
            .flat_map(|detail| detail.rules.iter().map(CompiledRule::effective_rule))
            .collect()
    }

    /// The custom `substitutionVariables` table.
    #[must_use]
    pub fn substitution_variables(&self) -> &BTreeMap<String, Vec<String>> {
        &self.substitution_variables
    }

    /// Validates the document, combining structural, compilation, and semantic findings.
    #[must_use]
    pub fn validate(&self) -> ValidationReport {
        let mut diagnostics = self.document.structural.clone();
        diagnostics.extend(self.compile_diagnostics.iter().cloned());
        diagnostics.extend(crate::validate::semantic(self));
        ValidationReport::from_diagnostics(diagnostics)
    }
}

pub(crate) fn compile(document: &AasaDocument) -> CompiledAasa {
    let mut diagnostics = Vec::new();
    let mut details = Vec::new();
    let mut substitution_variables = BTreeMap::new();
    let has_applinks = document.applinks.is_some();

    if let Some(applinks) = &document.applinks {
        substitution_variables.clone_from(&applinks.substitution_variables);
        let table = build_table(applinks, &mut diagnostics);
        for (index, detail) in applinks.details.iter().enumerate() {
            details.push(compile_detail(
                applinks,
                detail,
                index,
                &table,
                &mut diagnostics,
            ));
        }
    }

    let needs_decoded = details
        .iter()
        .flat_map(|detail| detail.rules.iter())
        .any(|rule| !rule.effective.percent_encoded);
    let needs_query_items = details
        .iter()
        .flat_map(|detail| detail.rules.iter())
        .any(|rule| matches!(rule.query, Some(CompiledQuery::Items(_))));

    CompiledAasa {
        document: document.clone(),
        details,
        has_applinks,
        webcredentials: service_apps(document.webcredentials.as_ref()),
        appclips: service_apps(document.appclips.as_ref()),
        activitycontinuation: service_apps(document.activitycontinuation.as_ref()),
        substitution_variables,
        compile_diagnostics: diagnostics,
        needs_decoded,
        needs_query_items,
    }
}

fn service_apps(service: Option<&crate::model::AppService>) -> Vec<String> {
    service
        .map(|service| service.apps.clone())
        .unwrap_or_default()
}

fn build_table(applinks: &AppLinks, diagnostics: &mut Vec<Diagnostic>) -> SubstitutionTable {
    for (name, values) in &applinks.substitution_variables {
        let path = format!("applinks.substitutionVariables.{name}");
        if name.contains(['$', '(', ')']) {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::MalformedSubstitutionName,
                    &path,
                    format!("`{name}` contains $, ( or ), which Apple does not allow in a variable name"),
                )
                .with_help("rename the variable using only characters outside $ ( )"),
            );
        }
        if SubstitutionTable::is_predefined(name) {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::SubstitutionShadowsPredefined,
                    &path,
                    format!("`{name}` shadows the predefined $({name}) variable"),
                )
                .with_help("this crate honours your definition; rename it to avoid ambiguity"),
            );
        }
        if values.is_empty() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::EmptySubstitutionList,
                    &path,
                    format!("`{name}` has no values, so any pattern using it can never match"),
                )
                .with_help("remove the variable or give it at least one value"),
            );
        }
        for (index, value) in values.iter().enumerate() {
            if value.contains("$(") {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::RecursiveSubstitutionValue,
                        format!("{path}[{index}]"),
                        format!(
                            "`{value}` references another substitution variable, which Apple does \
                             not allow"
                        ),
                    )
                    .with_help("inline the referenced values instead"),
                );
            }
            if value.is_empty() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::EmptyPatternAlternative,
                    format!("{path}[{index}]"),
                    format!("`{name}` contains an empty alternative"),
                ));
            }
        }
    }
    SubstitutionTable::from_custom(applinks.substitution_variables.clone())
}

fn compile_detail(
    applinks: &AppLinks,
    detail: &AppLinkDetail,
    index: usize,
    table: &SubstitutionTable,
    diagnostics: &mut Vec<Diagnostic>,
) -> CompiledDetail {
    let base = EffectiveDefaults::default()
        .overridden_by(applinks.defaults.as_ref())
        .overridden_by(detail.defaults.as_ref());

    let mut app_ids: Vec<String> = Vec::new();
    for app_id in detail.declared_app_ids() {
        if !app_ids.iter().any(|existing| existing == app_id) {
            app_ids.push(app_id.to_owned());
        }
    }

    let mut rules = Vec::new();
    let detail_path = || format!("applinks.details[{index}]");

    if let Some(components) = &detail.components {
        for (rule_index, component) in components.iter().enumerate() {
            rules.push(compile_component(
                component,
                index,
                rule_index,
                base,
                table,
                &|| format!("{}.components[{rule_index}]", detail_path()),
                diagnostics,
            ));
        }
    }

    if let Some(paths) = &detail.paths {
        let offset = rules.len();
        for (position, path) in paths.iter().enumerate() {
            let rule_index = offset + position;
            rules.push(compile_legacy_path(
                path,
                index,
                rule_index,
                base,
                table,
                &|| format!("{}.paths[{position}]", detail_path()),
                diagnostics,
            ));
        }
    }

    CompiledDetail {
        index,
        app_ids,
        rules,
    }
}

/// Diagnostic paths are passed as closures: a healthy rule never formats one.
type PathFn<'a> = &'a dyn Fn() -> String;

fn compile_component(
    component: &ComponentRule,
    detail_index: usize,
    rule_index: usize,
    base: EffectiveDefaults,
    table: &SubstitutionTable,
    path: PathFn<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> CompiledRule {
    let mut effective = base;
    if let Some(case_sensitive) = component.case_sensitive {
        effective.case_sensitive = case_sensitive;
    }
    if let Some(percent_encoded) = component.percent_encoded {
        effective.percent_encoded = percent_encoded;
    }

    // A leading run of slashes in a path pattern is not significant; `swcutil` matches `//abc`
    // against `/abc`.
    let compiled_path = component.path.as_ref().map(|pattern| {
        compile_path(
            pattern,
            effective.case_sensitive,
            table,
            &|| format!("{}./", path()),
            diagnostics,
        )
    });

    let compiled_fragment = component.fragment.as_ref().map(|pattern| {
        compile_pattern(
            pattern,
            effective.case_sensitive,
            table,
            &|| format!("{}.#", path()),
            diagnostics,
        )
    });

    let compiled_query = component.query.as_ref().map(|query| match query {
        QueryRule::Whole(pattern) => CompiledQuery::Whole(compile_pattern(
            pattern,
            effective.case_sensitive,
            table,
            &|| format!("{}.?", path()),
            diagnostics,
        )),
        QueryRule::Items(items) => {
            if items
                .values()
                .any(|predicate| matches!(predicate, QueryPredicate::Unsupported { .. }))
            {
                CompiledQuery::IgnoredDictionary(items.keys().cloned().collect())
            } else {
                CompiledQuery::Items(
                    items
                        .iter()
                        .filter_map(|(key, predicate)| match predicate {
                            QueryPredicate::Pattern(pattern) => Some((
                                key.clone(),
                                compile_pattern(
                                    pattern,
                                    effective.case_sensitive,
                                    table,
                                    &|| format!("{}.?.{key}", path()),
                                    diagnostics,
                                ),
                            )),
                            QueryPredicate::Unsupported { .. } => None,
                        })
                        .collect(),
                )
            }
        }
    });

    CompiledRule {
        detail_index,
        rule_index,
        legacy: false,
        exclude: component.exclude.unwrap_or(false),
        effective,
        path: compiled_path,
        query: compiled_query,
        fragment: compiled_fragment,
        comment: component.comment.clone(),
    }
}

fn compile_legacy_path(
    source: &str,
    detail_index: usize,
    rule_index: usize,
    base: EffectiveDefaults,
    table: &SubstitutionTable,
    path: PathFn<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> CompiledRule {
    let (exclude, pattern_source) = match source.strip_prefix("NOT ") {
        Some(rest) => (true, rest.trim_start()),
        None => (false, source),
    };
    let compiled = compile_path(
        pattern_source,
        base.case_sensitive,
        table,
        path,
        diagnostics,
    );
    CompiledRule {
        detail_index,
        rule_index,
        legacy: true,
        exclude,
        effective: base,
        path: Some(compiled),
        query: None,
        fragment: None,
        comment: None,
    }
}

/// Compiles a `/` pattern, adding the parent form only when the pattern actually ends in `/*`.
fn compile_path(
    source: &str,
    case_sensitive: bool,
    table: &SubstitutionTable,
    path: PathFn<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> CompiledPath {
    let (canonical, shape) = crate::url::normalize_path_pattern(source);
    let pattern = compile_pattern(&canonical, case_sensitive, table, path, diagnostics);
    let parent = shape.matches_parent.then(|| {
        let parent = canonical
            .strip_suffix("/*")
            .expect("matches_parent implies a trailing /*");
        compile_pattern(parent, case_sensitive, table, path, diagnostics)
    });
    CompiledPath {
        pattern,
        parent,
        try_bare_path: !shape.canonical_leading_slash,
    }
}

fn compile_pattern(
    source: &str,
    case_sensitive: bool,
    table: &SubstitutionTable,
    path: PathFn<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Pattern {
    let mut errors = Vec::new();
    let pattern = Pattern::compile(source, case_sensitive, table, &mut errors);
    for error in errors {
        diagnostics.push(match error {
            PatternError::UnterminatedReference => Diagnostic::new(
                DiagnosticCode::UnterminatedSubstitutionReference,
                path(),
                format!("`{source}` contains a `$(` that is never closed"),
            )
            .with_help("close the reference with `)`"),
            PatternError::UnknownVariable(name) => Diagnostic::new(
                DiagnosticCode::UnknownSubstitutionVariable,
                path(),
                format!("`$({name})` is neither a predefined variable nor declared in substitutionVariables"),
            )
            .with_help("declare it under applinks.substitutionVariables, or fix the spelling"),
            PatternError::NestedSubstitution { variable, value } => Diagnostic::new(
                DiagnosticCode::RecursiveSubstitutionValue,
                path(),
                format!("substitution value `{value}` references `$({variable})`"),
            ),
            PatternError::EmptyVariable(name) => Diagnostic::new(
                DiagnosticCode::EmptySubstitutionList,
                path(),
                format!("`$({name})` has no values, so this pattern can never match"),
            ),
        });
    }
    pattern
}
