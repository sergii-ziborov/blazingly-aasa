//! The wire model: what an `apple-app-site-association` file literally says.
//!
//! This layer is deliberately permissive. It keeps `appID` and `appIDs` separate so the validator
//! can report a document that sets both, and it keeps legacy `paths` alongside modern `components`
//! so a mixed document can be diagnosed rather than silently reinterpreted. Normalisation into a
//! single matchable form happens in [`CompiledAasa`](crate::CompiledAasa).

use std::collections::BTreeMap;

/// A parsed `apple-app-site-association` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AasaDocument {
    /// The `applinks` (universal links) section.
    pub applinks: Option<AppLinks>,
    /// The `webcredentials` (shared web credentials) section.
    pub webcredentials: Option<AppService>,
    /// The `appclips` section.
    pub appclips: Option<AppService>,
    /// The `activitycontinuation` (Handoff) section.
    pub activitycontinuation: Option<AppService>,
    /// Top-level keys this crate does not recognize, in source order.
    pub unknown_keys: Vec<String>,
    pub(crate) structural: Vec<crate::diagnostics::Diagnostic>,
    pub(crate) byte_len: usize,
}

/// A service that is configured with a flat list of app identifiers.
///
/// Used by `webcredentials`, `appclips`, and `activitycontinuation`, none of which perform URL
/// component matching.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppService {
    /// The application identifiers listed under `apps`.
    pub apps: Vec<String>,
}

/// The `applinks` section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppLinks {
    /// The legacy `apps` key, which Apple requires to be empty when present.
    pub apps: Option<Vec<String>>,
    /// Domain-level pattern-matching defaults.
    pub defaults: Option<MatchDefaults>,
    /// The app entries, in source order. Order is significant.
    pub details: Vec<AppLinkDetail>,
    /// Custom `substitutionVariables`.
    pub substitution_variables: BTreeMap<String, Vec<String>>,
    /// Whether `details` used the oldest dictionary-keyed-by-appID form rather than an array.
    pub details_were_dictionary: bool,
}

/// One entry of `applinks.details`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppLinkDetail {
    /// The singular `appID` key.
    pub app_id: Option<String>,
    /// The plural `appIDs` key.
    pub app_ids: Option<Vec<String>>,
    /// Modern ordered component rules.
    pub components: Option<Vec<ComponentRule>>,
    /// Legacy `paths` patterns, including `NOT `-prefixed exclusions.
    pub paths: Option<Vec<String>>,
    /// App-level pattern-matching defaults.
    pub defaults: Option<MatchDefaults>,
}

impl AppLinkDetail {
    /// Every application identifier this entry declares, `appID` first.
    #[must_use]
    pub fn declared_app_ids(&self) -> Vec<&str> {
        let mut out = Vec::new();
        if let Some(app_id) = &self.app_id {
            out.push(app_id.as_str());
        }
        if let Some(app_ids) = &self.app_ids {
            out.extend(app_ids.iter().map(String::as_str));
        }
        out
    }
}

/// One entry of a `components` array.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComponentRule {
    /// The `/` key: a pattern for the URL path.
    pub path: Option<String>,
    /// The `?` key: a pattern or dictionary for the URL query.
    pub query: Option<QueryRule>,
    /// The `#` key: a pattern for the URL fragment.
    pub fragment: Option<String>,
    /// `exclude`: stop matching and refuse to open the URL.
    pub exclude: Option<bool>,
    /// `comment`: ignored by the system, preserved here for traces.
    pub comment: Option<String>,
    /// `caseSensitive` override.
    pub case_sensitive: Option<bool>,
    /// `percentEncoded` override.
    pub percent_encoded: Option<bool>,
}

impl ComponentRule {
    /// Whether the rule constrains no URL component at all, and so matches everything.
    #[must_use]
    pub fn is_unconstrained(&self) -> bool {
        self.path.is_none() && self.query.is_none() && self.fragment.is_none()
    }
}

/// The `?` key, which Apple allows to be either a pattern or a dictionary of predicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryRule {
    /// A single pattern matched against the whole query string.
    Whole(String),
    /// Named predicates that must all be satisfied.
    Items(BTreeMap<String, QueryPredicate>),
}

/// One entry of a `?` dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryPredicate {
    /// A pattern matched against the query item's value.
    Pattern(String),
    /// A value whose meaning Apple does not document; it is reported rather than guessed at.
    Unsupported {
        /// The JSON type that appeared in place of a string.
        json_type: &'static str,
    },
}

/// Pattern-matching defaults, which may appear at the domain and app level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchDefaults {
    /// `caseSensitive` default for everything below this level.
    pub case_sensitive: Option<bool>,
    /// `percentEncoded` default for everything below this level.
    pub percent_encoded: Option<bool>,
    /// Other keys present in the object, which Apple documents as legal but does not specify.
    pub other_keys: Vec<String>,
}

impl MatchDefaults {
    /// Whether the object carried no setting this crate acts on.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.case_sensitive.is_none() && self.percent_encoded.is_none()
    }
}

/// Apple's documented default: patterns are case-sensitive.
pub const DEFAULT_CASE_SENSITIVE: bool = true;
/// Apple's documented default: patterns are written percent-encoded.
pub const DEFAULT_PERCENT_ENCODED: bool = true;

/// The effective pattern-matching settings for one rule, after resolving the defaults hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct EffectiveDefaults {
    /// Effective `caseSensitive`.
    pub case_sensitive: bool,
    /// Effective `percentEncoded`.
    pub percent_encoded: bool,
}

impl Default for EffectiveDefaults {
    fn default() -> Self {
        Self {
            case_sensitive: DEFAULT_CASE_SENSITIVE,
            percent_encoded: DEFAULT_PERCENT_ENCODED,
        }
    }
}

impl EffectiveDefaults {
    /// Applies a less specific layer of defaults, with existing values winning.
    #[must_use]
    pub fn overridden_by(self, defaults: Option<&MatchDefaults>) -> Self {
        let Some(defaults) = defaults else {
            return self;
        };
        Self {
            case_sensitive: defaults.case_sensitive.unwrap_or(self.case_sensitive),
            percent_encoded: defaults.percent_encoded.unwrap_or(self.percent_encoded),
        }
    }
}
