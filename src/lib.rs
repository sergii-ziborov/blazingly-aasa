//! Apple Associated Domains semantics for Rust and WebAssembly.
//!
//! `blazingly-aasa` parses, validates, matches, explains, and compares `apple-app-site-association`
//! files. It is a semantic engine, not a fetcher: it never touches the network, never opens an
//! `.ipa`, and never claims to know what a device will do. Give it bytes and explicit context, and
//! it tells you exactly what the document says.
//!
//! # Three separate questions
//!
//! 1. **Is this parseable?** [`AasaDocument::parse`] fails only on invalid JSON, a non-object root,
//!    or an oversized payload.
//! 2. **Is this sane?** [`CompiledAasa::validate`] returns a [`ValidationReport`] of stable,
//!    machine-readable [`DiagnosticCode`]s rather than a single yes/no.
//! 3. **Does this URL match?** [`CompiledAasa::match_url`] returns [`MatchDecision::Match`],
//!    [`MatchDecision::Exclude`], or [`MatchDecision::NoMatch`] — with a trace explaining why.
//!
//! A URL that does not match is not an error, and neither is one that is excluded. Both are
//! answers.
//!
//! # Matching a URL
//!
//! ```
//! use blazingly_aasa::{CompiledAasa, MatchDecision};
//!
//! let bytes = br#"{
//!   "applinks": {
//!     "details": [{
//!       "appIDs": ["ABCDE12345.com.example.app"],
//!       "components": [
//!         { "/": "/help/website/*", "exclude": true },
//!         { "/": "/help/*", "?": { "articleNumber": "????" } }
//!       ]
//!     }]
//!   }
//! }"#;
//!
//! let aasa = CompiledAasa::parse(bytes)?;
//! let app = "ABCDE12345.com.example.app";
//!
//! let hit = aasa.match_url("example.com", app, "https://example.com/help/1?articleNumber=4815")?;
//! assert_eq!(hit.decision, MatchDecision::Match);
//!
//! let blocked = aasa.match_url("example.com", app, "https://example.com/help/website/faq")?;
//! assert_eq!(blocked.decision, MatchDecision::Exclude);
//!
//! // Three characters, not four: the query predicate rejects it.
//! let miss = aasa.match_url("example.com", app, "https://example.com/help/1?articleNumber=481")?;
//! assert_eq!(miss.decision, MatchDecision::NoMatch);
//! # Ok::<(), blazingly_aasa::Error>(())
//! ```
//!
//! # Explaining a decision
//!
//! Every result formats itself into something you can paste into a bug report:
//!
//! ```
//! # use blazingly_aasa::CompiledAasa;
//! # let bytes = br#"{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/buy/*"}]}]}}"#;
//! # let aasa = CompiledAasa::parse(bytes)?;
//! let result = aasa.match_url("example.com", "A.b", "https://example.com/sell/42")?;
//! println!("{result}");
//! # Ok::<(), blazingly_aasa::Error>(())
//! ```
//!
//! # Comparing two files
//!
//! [`CompiledAasa::semantic_diff`] compares behaviour rather than text, so moving
//! `caseSensitive` from every component up into `defaults` reports no change, while reordering two
//! rules does:
//!
//! ```
//! use blazingly_aasa::CompiledAasa;
//!
//! let spelled_out = CompiledAasa::parse(br#"{"applinks":{"details":[{
//!     "appIDs": ["A.b"],
//!     "components": [{ "/": "/buy/*", "caseSensitive": false }]
//! }]}}"#)?;
//!
//! let refactored = CompiledAasa::parse(br#"{"applinks":{"details":[{
//!     "appIDs": ["A.b"],
//!     "defaults": { "caseSensitive": false },
//!     "components": [{ "/": "/buy/*" }]
//! }]}}"#)?;
//!
//! assert!(spelled_out.semantic_diff(&refactored).is_equivalent());
//! assert!(!spelled_out.structural_equal(&refactored));
//! # Ok::<(), blazingly_aasa::Error>(())
//! ```
//!
//! # What this crate will not do
//!
//! It does not fetch `.well-known/apple-app-site-association`, talk to Apple's CDN, read
//! entitlements out of a signed binary, or model device state. Those belong in the tools that use
//! this crate. See `docs/parity.md` for the behaviours that are verified against Apple's
//! documentation and the ones that are still open questions.

#![forbid(unsafe_code)]

mod compile;
mod diagnostics;
mod diff;
mod error;
mod explain;
mod iso_tables;
mod matcher;
mod model;
mod normalize;
mod parse;
mod pattern;
mod signed;
mod substitution;
mod url;
mod validate;
mod wildcard;

pub use compile::{CompiledAasa, EffectiveQuery, EffectiveRule, Service};
pub use diagnostics::{Diagnostic, DiagnosticCode, Severity, ValidationReport};
pub use diff::{AasaDiff, SemanticChange};
pub use error::{Error, ParseError, ParseErrorKind, Result, UrlError};
pub use explain::{
    ComponentReason, ComponentTrace, DetailTrace, MatchDecision, MatchResult, MatchTrace,
    RuleTrace, StopReason, UrlComponent,
};
pub use model::{
    AasaDocument, AppLinkDetail, AppLinks, AppService, ComponentRule, EffectiveDefaults,
    MatchDefaults, QueryPredicate, QueryRule, DEFAULT_CASE_SENSITIVE, DEFAULT_PERCENT_ENCODED,
};
pub use parse::ParseOptions;
pub use url::{percent_decode, UrlParts};
pub use wildcard::{PatternSyntaxError, WildcardPattern};

/// The Foundation release the `$(region)` and `$(lang)` tables were generated from.
///
/// Apple defines those variables as `Locale.isoRegionCodes` and `Locale.isoLanguageCodes`, which
/// change between OS releases. Knowing which snapshot you are matching against matters.
pub const ISO_TABLE_SOURCE: &str = substitution::ISO_TABLE_SOURCE;

impl AasaDocument {
    /// Parses `apple-app-site-association` bytes with the default limits.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] for invalid JSON, a non-object root, or an oversized payload.
    /// Structural problems inside the document are reported by [`CompiledAasa::validate`] instead.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseError> {
        parse::parse(bytes, &ParseOptions::default())
    }

    /// Parses with explicit limits.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] for invalid JSON, a non-object root, or an oversized payload.
    pub fn parse_with(
        bytes: &[u8],
        options: &ParseOptions,
    ) -> std::result::Result<Self, ParseError> {
        parse::parse(bytes, options)
    }

    /// Parses from a string.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] for invalid JSON, a non-object root, or an oversized payload.
    pub fn parse_str(input: &str) -> std::result::Result<Self, ParseError> {
        Self::parse(input.as_bytes())
    }

    /// Normalizes the document for matching, explaining, and comparing.
    #[must_use]
    pub fn compile(&self) -> CompiledAasa {
        compile::compile(self)
    }

    /// Validates the document. Equivalent to `self.compile().validate()`.
    #[must_use]
    pub fn validate(&self) -> ValidationReport {
        self.compile().validate()
    }

    /// The size of the payload this document was parsed from, in bytes.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }
}

/// Splits `ABCDE12345.com.example.app` into its application identifier prefix and bundle
/// identifier.
///
/// Apple documents the form as `<Application Identifier Prefix>.<Bundle Identifier>`, and the
/// prefix never contains a dot, so the split is at the first one. Returns `None` when there is no
/// dot, or when either half would be empty.
///
/// ```
/// assert_eq!(
///     blazingly_aasa::split_app_id("ABCDE12345.com.example.app"),
///     Some(("ABCDE12345", "com.example.app")),
/// );
/// assert_eq!(blazingly_aasa::split_app_id("nodots"), None);
/// ```
#[must_use]
pub fn split_app_id(app_id: &str) -> Option<(&str, &str)> {
    let (prefix, bundle) = app_id.split_once('.')?;
    (!prefix.is_empty() && !bundle.is_empty()).then_some((prefix, bundle))
}

/// Parses and validates in one call.
///
/// # Errors
///
/// Returns [`ParseError`] for invalid JSON, a non-object root, or an oversized payload.
pub fn validate(bytes: &[u8]) -> std::result::Result<ValidationReport, ParseError> {
    Ok(CompiledAasa::parse(bytes)?.validate())
}

/// Parses and matches in one call.
///
/// Prefer [`CompiledAasa::match_url`] when testing more than one URL against the same document:
/// this helper reparses and recompiles every time.
///
/// # Errors
///
/// Returns [`Error::Parse`] for an unusable document and [`Error::Url`] for an unusable URL.
pub fn match_url(bytes: &[u8], domain: &str, app_id: &str, url: &str) -> Result<MatchResult> {
    let compiled = CompiledAasa::parse(bytes)?;
    Ok(compiled.match_url(domain, app_id, url)?)
}

/// Parses both documents and compares them semantically.
///
/// # Errors
///
/// Returns [`ParseError`] when either payload is unusable.
pub fn diff(left: &[u8], right: &[u8]) -> std::result::Result<AasaDiff, ParseError> {
    let left = CompiledAasa::parse(left)?;
    let right = CompiledAasa::parse(right)?;
    Ok(left.semantic_diff(&right))
}
