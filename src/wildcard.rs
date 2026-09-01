//! A standalone Apple wildcard pattern, outside any association file.
//!
//! Useful when you want to check one pattern against one string — in a test, a REPL, or an editor
//! plugin — without constructing a whole document.

use crate::pattern::{Pattern, PatternError};
use crate::substitution::SubstitutionTable;
use std::collections::BTreeMap;
use std::fmt;

/// A pattern that could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternSyntaxError {
    message: String,
}

impl PatternSyntaxError {
    /// What is wrong with the pattern.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PatternSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PatternSyntaxError {}

/// A compiled Apple URL-component pattern.
///
/// `*` matches zero or more characters, `?` matches exactly one, and therefore `?*` matches one or
/// more. `$(name)` expands to any one of a substitution variable's alternatives.
///
/// ```
/// use blazingly_aasa::WildcardPattern;
///
/// let pattern = WildcardPattern::compile("/help/*", true)?;
/// assert!(pattern.matches("/help/website"));
/// assert!(!pattern.matches("/support/website"));
///
/// let predefined = WildcardPattern::compile("/id/$(digit)$(digit)", true)?;
/// assert!(predefined.matches("/id/42"));
/// assert!(!predefined.matches("/id/4x"));
/// # Ok::<(), blazingly_aasa::PatternSyntaxError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WildcardPattern {
    inner: Pattern,
}

impl WildcardPattern {
    /// Compiles a pattern with only Apple's predefined substitution variables available.
    ///
    /// # Errors
    ///
    /// Returns [`PatternSyntaxError`] for an unterminated `$(` or an unknown variable name.
    pub fn compile(source: &str, case_sensitive: bool) -> Result<Self, PatternSyntaxError> {
        Self::compile_with(source, case_sensitive, &BTreeMap::new())
    }

    /// Compiles a pattern with custom substitution variables in scope.
    ///
    /// # Errors
    ///
    /// Returns [`PatternSyntaxError`] for an unterminated `$(`, an unknown variable name, an empty
    /// variable, or a value that references another variable.
    pub fn compile_with(
        source: &str,
        case_sensitive: bool,
        variables: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self, PatternSyntaxError> {
        let table = SubstitutionTable::from_custom(variables.clone());
        let mut errors = Vec::new();
        let inner = Pattern::compile(source, case_sensitive, &table, &mut errors);
        if let Some(error) = errors.first() {
            return Err(PatternSyntaxError {
                message: match error {
                    PatternError::UnterminatedReference => {
                        format!("`{source}` contains a `$(` that is never closed")
                    }
                    PatternError::UnknownVariable(name) => format!(
                        "`$({name})` is neither predefined nor supplied as a custom variable"
                    ),
                    PatternError::NestedSubstitution { variable, value } => format!(
                        "substitution value `{value}` references `$({variable})`, which Apple does \
                         not allow"
                    ),
                    PatternError::EmptyVariable(name) => {
                        format!("`$({name})` has no values, so the pattern can never match")
                    }
                },
            });
        }
        Ok(Self { inner })
    }

    /// Whether the whole of `input` matches.
    #[must_use]
    pub fn matches(&self, input: &str) -> bool {
        self.inner.matches(input)
    }

    /// Whether the whole of `input` matches, overriding case sensitivity for this call.
    #[must_use]
    pub fn matches_with_case(&self, input: &str, case_sensitive: bool) -> bool {
        self.inner.matches_with(input, case_sensitive)
    }

    /// The pattern text as written.
    #[must_use]
    pub fn source(&self) -> &str {
        self.inner.source()
    }
}
