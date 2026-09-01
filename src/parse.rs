//! Turning bytes into the wire model.
//!
//! Parsing is lenient by design: only invalid JSON, a non-object root, or an oversized payload
//! fail outright. Everything else — a `details` array holding a number, a `?` predicate that is a
//! boolean, an unrecognized top-level key — is recorded as a structural diagnostic and skipped, so
//! one bad entry never hides the rest of the file. Apple adds keys over time; rejecting a document
//! because of an unfamiliar one would be worse than ignoring it.

use crate::diagnostics::{Diagnostic, DiagnosticCode};
use crate::error::{ParseError, ParseErrorKind};
use crate::model::{
    AasaDocument, AppLinkDetail, AppLinks, AppService, ComponentRule, MatchDefaults,
    QueryPredicate, QueryRule,
};
use blazingly_json::{Map, Value};
use std::collections::BTreeMap;

/// Limits applied while parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOptions {
    max_bytes: usize,
}

impl ParseOptions {
    /// The default payload ceiling, in bytes.
    ///
    /// This is a defensive policy chosen by this crate for handling remote, attacker-controlled
    /// input — not a limit Apple states in the reference pages this crate cites. Raise or lower it
    /// freely with [`ParseOptions::max_bytes`].
    pub const DEFAULT_MAX_BYTES: usize = 128 * 1024;

    /// Options with the default limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum accepted payload size, in bytes.
    #[must_use]
    pub fn max_bytes(mut self, bytes: usize) -> Self {
        self.max_bytes = bytes;
        self
    }

    /// The configured payload ceiling.
    #[must_use]
    pub fn max_bytes_value(&self) -> usize {
        self.max_bytes
    }
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            max_bytes: Self::DEFAULT_MAX_BYTES,
        }
    }
}

pub(crate) fn parse(bytes: &[u8], options: &ParseOptions) -> Result<AasaDocument, ParseError> {
    if bytes.len() > options.max_bytes {
        return Err(ParseError::new(
            ParseErrorKind::TooLarge {
                limit: options.max_bytes,
                actual: bytes.len(),
            },
            format!(
                "payload is {} bytes, above the configured {} byte limit",
                bytes.len(),
                options.max_bytes
            ),
        ));
    }

    let value: Value = blazingly_json::from_slice(bytes)
        .map_err(|error| ParseError::new(ParseErrorKind::Json, error.to_string()))?;

    let Value::Object(root) = value else {
        return Err(ParseError::new(
            ParseErrorKind::RootNotObject,
            format!(
                "root value is {}, but apple-app-site-association must be a JSON object",
                type_name(&value)
            ),
        ));
    };

    let mut walker = Walker::default();
    let mut document = AasaDocument {
        applinks: None,
        webcredentials: None,
        appclips: None,
        activitycontinuation: None,
        unknown_keys: Vec::new(),
        structural: Vec::new(),
        byte_len: bytes.len(),
    };

    for (key, value) in &root {
        match key.as_str() {
            "applinks" => document.applinks = walker.applinks(value, "applinks"),
            "webcredentials" => document.webcredentials = walker.service(value, "webcredentials"),
            "appclips" => document.appclips = walker.service(value, "appclips"),
            "activitycontinuation" => {
                document.activitycontinuation = walker.service(value, "activitycontinuation");
            }
            other => {
                document.unknown_keys.push(other.to_owned());
                walker.push(
                    Diagnostic::new(
                        DiagnosticCode::UnknownTopLevelKey,
                        other,
                        format!("`{other}` is not an Associated Domains service this crate knows"),
                    )
                    .with_help(
                        "it is ignored; Apple ignores unknown keys too, so this is only a heads-up",
                    ),
                );
            }
        }
    }

    document.structural = walker.diagnostics;
    Ok(document)
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[derive(Default)]
struct Walker {
    diagnostics: Vec<Diagnostic>,
}

impl Walker {
    fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Diagnostic paths are built by a closure so the happy path never formats a string. Parsing
    /// a healthy document is the overwhelmingly common case, and it used to spend more time
    /// building paths for diagnostics it never emitted than it did parsing JSON.
    fn mismatch(&mut self, path: impl FnOnce() -> String, expected: &str, value: &Value) {
        self.push(Diagnostic::new(
            DiagnosticCode::FieldTypeMismatch,
            path(),
            format!("expected {expected} but found {}", type_name(value)),
        ));
    }

    fn object<'a>(
        &mut self,
        value: &'a Value,
        path: impl FnOnce() -> String,
    ) -> Option<&'a Map<String, Value>> {
        match value {
            Value::Object(object) => Some(object),
            other => {
                self.mismatch(path, "an object", other);
                None
            }
        }
    }

    fn string(&mut self, value: &Value, path: impl FnOnce() -> String) -> Option<String> {
        match value {
            Value::String(text) => Some(text.clone()),
            other => {
                self.mismatch(path, "a string", other);
                None
            }
        }
    }

    fn bool(&mut self, value: &Value, path: impl FnOnce() -> String) -> Option<bool> {
        match value {
            Value::Bool(flag) => Some(*flag),
            other => {
                self.mismatch(path, "a boolean", other);
                None
            }
        }
    }

    fn string_array(&mut self, value: &Value, path: &str) -> Option<Vec<String>> {
        let Value::Array(items) = value else {
            self.mismatch(|| path.to_owned(), "an array of strings", value);
            return None;
        };
        let mut out = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            if let Some(text) = self.string(item, || format!("{path}[{index}]")) {
                out.push(text);
            }
        }
        Some(out)
    }

    fn service(&mut self, value: &Value, path: &str) -> Option<AppService> {
        if value.is_null() {
            return None;
        }
        let object = self.object(value, || path.to_owned())?;
        let apps = match object.get("apps") {
            Some(apps) => self
                .string_array(apps, &format!("{path}.apps"))
                .unwrap_or_default(),
            None => Vec::new(),
        };
        Some(AppService { apps })
    }

    fn applinks(&mut self, value: &Value, path: &str) -> Option<AppLinks> {
        if value.is_null() {
            return None;
        }
        let object = self.object(value, || path.to_owned())?;
        let mut applinks = AppLinks::default();

        if let Some(apps) = object.get("apps") {
            applinks.apps = self.string_array(apps, &format!("{path}.apps"));
        }
        if let Some(defaults) = object.get("defaults") {
            applinks.defaults = self.defaults(defaults, &format!("{path}.defaults"));
        }
        if let Some(variables) = object.get("substitutionVariables") {
            applinks.substitution_variables =
                self.substitutions(variables, &format!("{path}.substitutionVariables"));
        }
        if let Some(details) = object.get("details") {
            let details_path = format!("{path}.details");
            match details {
                Value::Array(items) => {
                    applinks.details = items
                        .iter()
                        .enumerate()
                        .filter_map(|(index, item)| {
                            self.detail(item, &format!("{details_path}[{index}]"), None)
                        })
                        .collect();
                }
                Value::Object(entries) => {
                    applinks.details_were_dictionary = true;
                    self.push(
                        Diagnostic::new(
                            DiagnosticCode::LegacyDetailsDictionary,
                            &details_path,
                            "`details` is a dictionary keyed by application identifier, the oldest \
                             association-file form",
                        )
                        .with_help(
                            "migrate to an array of detail objects; a dictionary has no defined \
                             rule order, so this crate evaluates the keys in sorted order",
                        ),
                    );
                    applinks.details = entries
                        .iter()
                        .filter_map(|(app_id, item)| {
                            self.detail(
                                item,
                                &format!("{details_path}.{app_id}"),
                                Some(app_id.clone()),
                            )
                        })
                        .collect();
                }
                Value::Null => {}
                other => {
                    self.mismatch(|| details_path.clone(), "an array of detail objects", other);
                }
            }
        }
        Some(applinks)
    }

    fn detail(
        &mut self,
        value: &Value,
        path: &str,
        implied_app_id: Option<String>,
    ) -> Option<AppLinkDetail> {
        let object = self.object(value, || path.to_owned())?;
        let mut detail = AppLinkDetail {
            app_id: implied_app_id,
            ..AppLinkDetail::default()
        };

        if let Some(app_id) = object.get("appID") {
            detail.app_id = self.string(app_id, || format!("{path}.appID"));
        }
        if let Some(app_ids) = object.get("appIDs") {
            detail.app_ids = self.string_array(app_ids, &format!("{path}.appIDs"));
        }
        if let Some(defaults) = object.get("defaults") {
            detail.defaults = self.defaults(defaults, &format!("{path}.defaults"));
        }
        if let Some(paths) = object.get("paths") {
            detail.paths = self.string_array(paths, &format!("{path}.paths"));
        }
        if let Some(components) = object.get("components") {
            let components_path = format!("{path}.components");
            match components {
                Value::Array(items) => {
                    detail.components = Some(
                        items
                            .iter()
                            .enumerate()
                            .filter_map(|(index, item)| {
                                self.component(item, &format!("{components_path}[{index}]"))
                            })
                            .collect(),
                    );
                }
                other => self.mismatch(
                    || components_path.clone(),
                    "an array of component objects",
                    other,
                ),
            }
        }
        Some(detail)
    }

    fn component(&mut self, value: &Value, path: &str) -> Option<ComponentRule> {
        let object = self.object(value, || path.to_owned())?;
        let mut rule = ComponentRule::default();
        for (key, value) in object {
            let field = || format!("{path}.{key}");
            match key.as_str() {
                "/" => rule.path = self.string(value, field),
                "#" => rule.fragment = self.string(value, field),
                // `query` needs the path for its own nested diagnostics, so it pays the format
                // only when the component actually carries a `?`.
                "?" => rule.query = self.query(value, &field()),
                "exclude" => rule.exclude = self.bool(value, field),
                "caseSensitive" => rule.case_sensitive = self.bool(value, field),
                "percentEncoded" => rule.percent_encoded = self.bool(value, field),
                "comment" => rule.comment = self.string(value, field),
                _ => {}
            }
        }
        Some(rule)
    }

    fn query(&mut self, value: &Value, path: &str) -> Option<QueryRule> {
        match value {
            Value::String(pattern) => Some(QueryRule::Whole(pattern.clone())),
            Value::Object(items) => {
                let mut predicates = BTreeMap::new();
                for (key, value) in items {
                    let predicate = match value {
                        Value::String(pattern) => QueryPredicate::Pattern(pattern.clone()),
                        other => {
                            self.push(
                                Diagnostic::new(
                                    DiagnosticCode::UnsupportedQueryPredicate,
                                    format!("{path}.{key}"),
                                    format!(
                                        "query predicate is {}, but Apple documents only string \
                                         patterns here",
                                        type_name(other)
                                    ),
                                )
                                .with_help(
                                    "this predicate can never match; use a string pattern such as \
                                     \"*\" to accept any value",
                                ),
                            );
                            QueryPredicate::Unsupported {
                                json_type: match other {
                                    Value::Null => "null",
                                    Value::Bool(_) => "boolean",
                                    Value::Number(_) => "number",
                                    Value::Array(_) => "array",
                                    Value::Object(_) => "object",
                                    Value::String(_) => "string",
                                },
                            }
                        }
                    };
                    predicates.insert(key.clone(), predicate);
                }
                Some(QueryRule::Items(predicates))
            }
            other => {
                self.mismatch(
                    || path.to_owned(),
                    "a string pattern or an object of predicates",
                    other,
                );
                None
            }
        }
    }

    fn defaults(&mut self, value: &Value, path: &str) -> Option<MatchDefaults> {
        let object = self.object(value, || path.to_owned())?;
        let mut defaults = MatchDefaults::default();
        for (key, value) in object {
            let field = || format!("{path}.{key}");
            match key.as_str() {
                "caseSensitive" => defaults.case_sensitive = self.bool(value, field),
                "percentEncoded" => defaults.percent_encoded = self.bool(value, field),
                other => defaults.other_keys.push(other.to_owned()),
            }
        }
        if !defaults.other_keys.is_empty() {
            let keys = defaults.other_keys.join(", ");
            self.push(
                Diagnostic::new(
                    DiagnosticCode::DefaultsContainsPatternKeys,
                    path,
                    format!("`defaults` also carries {keys}"),
                )
                .with_help(
                    "Apple documents `defaults` as a subclass of `components`, but does not \
                     specify what a pattern key means there; this crate applies only \
                     caseSensitive and percentEncoded",
                ),
            );
        }
        Some(defaults)
    }

    fn substitutions(&mut self, value: &Value, path: &str) -> BTreeMap<String, Vec<String>> {
        let Some(object) = self.object(value, || path.to_owned()) else {
            return BTreeMap::new();
        };
        let mut out = BTreeMap::new();
        for (name, value) in object {
            if let Some(values) = self.string_array(value, &format!("{path}.{name}")) {
                out.insert(name.clone(), values);
            }
        }
        out
    }
}
