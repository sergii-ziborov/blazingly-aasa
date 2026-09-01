//! Machine-readable validation diagnostics.
//!
//! Every diagnostic carries a stable [`DiagnosticCode`] so CI consumers can allow-list or
//! fail on specific findings without string matching. Codes are part of the public contract:
//! they are added in minor releases and never repurposed.

use serde::Serialize;
use std::fmt;

/// How seriously to take a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational: legal, but worth surfacing.
    Info,
    /// The document is usable but something is likely unintended.
    Warning,
    /// The document is malformed or self-contradictory.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

macro_rules! diagnostic_codes {
    ($($variant:ident => ($code:literal, $severity:ident, $title:literal),)*) => {
        /// A stable, machine-readable identifier for a validation finding.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[non_exhaustive]
        #[serde(into = "&'static str")]
        pub enum DiagnosticCode {
            $(
                #[doc = $title]
                $variant,
            )*
        }

        impl DiagnosticCode {
            /// The stable textual code, for example `AASA110`.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)*
                }
            }

            /// The default severity for this code.
            #[must_use]
            pub const fn default_severity(self) -> Severity {
                match self {
                    $(Self::$variant => Severity::$severity,)*
                }
            }

            /// A short human-readable title.
            #[must_use]
            pub const fn title(self) -> &'static str {
                match self {
                    $(Self::$variant => $title,)*
                }
            }

            /// Every code known to this release, in ascending code order.
            #[must_use]
            pub const fn all() -> &'static [DiagnosticCode] {
                &[$(Self::$variant,)*]
            }
        }

        impl From<DiagnosticCode> for &'static str {
            fn from(code: DiagnosticCode) -> Self {
                code.as_str()
            }
        }
    };
}

diagnostic_codes! {
    InvalidJson => ("AASA001", Error, "payload is not valid JSON"),
    RootNotObject => ("AASA002", Error, "root value is not a JSON object"),
    FieldTypeMismatch => ("AASA004", Error, "field has an unexpected JSON type"),
    NoRecognizedService => ("AASA100", Warning, "no recognized Associated Domains service section"),
    UnknownTopLevelKey => ("AASA101", Info, "unrecognized top-level key"),
    DetailMissingAppId => ("AASA110", Error, "details entry has neither appID nor appIDs"),
    DetailHasBothAppIdForms => ("AASA111", Warning, "details entry declares both appID and appIDs"),
    MixedComponentsAndPaths => ("AASA120", Warning, "details entry mixes modern components with legacy paths"),
    LegacyDetailsDictionary => ("AASA121", Warning, "details uses the legacy dictionary form"),
    LegacyAppsKeyNonEmpty => ("AASA122", Warning, "legacy applinks.apps array is not empty"),
    EmptyAppIdentifier => ("AASA130", Error, "empty application identifier"),
    SuspiciousAppIdentifier => ("AASA131", Warning, "application identifier is not in <TeamID>.<BundleID> form"),
    MalformedSubstitutionName => ("AASA140", Error, "substitution variable name contains $, ( or )"),
    RecursiveSubstitutionValue => ("AASA141", Error, "substitution value references another substitution variable"),
    UnknownSubstitutionVariable => ("AASA142", Error, "pattern references an undefined substitution variable"),
    EmptySubstitutionList => ("AASA143", Warning, "substitution variable has no values and can never match"),
    SubstitutionShadowsPredefined => ("AASA144", Warning, "substitution variable shadows a predefined Apple variable"),
    UnsupportedQueryPredicate => ("AASA150", Error, "query predicate value is not a string"),
    UnterminatedSubstitutionReference => ("AASA151", Error, "pattern contains an unterminated $( reference"),
    DuplicateAppIdentifier => ("AASA160", Warning, "application identifier is listed more than once"),
    DocumentTooLarge => ("AASA170", Error, "payload exceeds the configured size limit"),
    EmptyComponentRule => ("AASA180", Warning, "component rule constrains nothing and matches every URL"),
    UnreachableRule => ("AASA190", Warning, "rule is unreachable because an earlier rule always matches"),
    PathPatternMissingLeadingSlash => ("AASA191", Warning, "path pattern cannot match because URL paths start with /"),
    DefaultsContainsPatternKeys => ("AASA192", Info, "defaults object carries pattern keys with undocumented behavior"),
    NoDetails => ("AASA193", Warning, "applinks declares no details, so no app can open this domain"),
    EmptyPatternAlternative => ("AASA194", Warning, "substitution value is empty"),
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single validation finding, anchored at a location inside the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable machine-readable code.
    pub code: DiagnosticCode,
    /// How seriously to take this finding.
    pub severity: Severity,
    /// Dotted path to the offending value, for example `applinks.details[0].components[2]./`.
    pub path: String,
    /// What is wrong.
    pub message: String,
    /// How to fix it, when there is a concrete suggestion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    pub(crate) fn new(
        code: DiagnosticCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: code.default_severity(),
            path: path.into(),
            message: message.into(),
            help: None,
        }
    }

    pub(crate) fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] {}: {}",
            self.severity, self.code, self.path, self.message
        )?;
        if let Some(help) = &self.help {
            write!(f, "\n  help: {help}")?;
        }
        Ok(())
    }
}

/// The result of validating a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub(crate) fn from_diagnostics(mut diagnostics: Vec<Diagnostic>) -> Self {
        diagnostics.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.code.cmp(&b.code))
        });
        Self { diagnostics }
    }

    /// Every diagnostic, most severe first.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Only the [`Severity::Error`] diagnostics.
    #[must_use]
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.iter_severity(Severity::Error)
    }

    /// Only the [`Severity::Warning`] diagnostics.
    #[must_use]
    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.iter_severity(Severity::Warning)
    }

    /// Only the [`Severity::Info`] diagnostics.
    #[must_use]
    pub fn infos(&self) -> Vec<&Diagnostic> {
        self.iter_severity(Severity::Info)
    }

    fn iter_severity(&self, severity: Severity) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == severity)
            .collect()
    }

    /// Whether any [`Severity::Error`] diagnostic was reported.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }

    /// Whether the report is completely empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Whether a specific code was reported.
    #[must_use]
    pub fn contains(&self, code: DiagnosticCode) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code)
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.diagnostics.is_empty() {
            return f.write_str("no diagnostics");
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{diagnostic}")?;
        }
        Ok(())
    }
}
