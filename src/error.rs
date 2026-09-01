//! Error types for parsing and URL handling.
//!
//! A document that parses but does not match a URL is **not** an error: that is
//! [`MatchDecision::NoMatch`](crate::MatchDecision::NoMatch). Likewise an excluded URL is
//! [`MatchDecision::Exclude`](crate::MatchDecision::Exclude). Errors are reserved for input that
//! cannot be interpreted at all.

use std::fmt;

/// Why an `apple-app-site-association` payload could not be turned into a document.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// The bytes are not valid JSON.
    Json,
    /// The JSON is valid but the root value is not an object.
    RootNotObject,
    /// The payload is larger than the configured [`ParseOptions`](crate::ParseOptions) limit.
    TooLarge {
        /// Configured limit, in bytes.
        limit: usize,
        /// Actual payload size, in bytes.
        actual: usize,
    },
}

/// A failure to parse an `apple-app-site-association` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: ParseErrorKind,
    message: String,
}

impl ParseError {
    pub(crate) fn new(kind: ParseErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// The category of failure.
    #[must_use]
    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }

    /// A human-readable description, including a source location for JSON syntax errors.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

/// A URL that could not be split into the components required for matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlError {
    message: String,
}

impl UrlError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// A human-readable description of the problem.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UrlError {}

/// The crate-wide error type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The payload could not be parsed.
    Parse(ParseError),
    /// The URL under test could not be split into components.
    Url(UrlError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid apple-app-site-association: {error}"),
            Self::Url(error) => write!(f, "invalid URL: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ParseError> for Error {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<UrlError> for Error {
    fn from(error: UrlError) -> Self {
        Self::Url(error)
    }
}

/// Convenience alias used across the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
