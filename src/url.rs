//! A small RFC 3986 splitter that preserves each component exactly as written.
//!
//! Associated Domains matching compares patterns against the URL the system was handed, so this
//! crate deliberately does **not** normalize, re-encode, or IDNA-map anything. The only
//! normalization applied is lowercasing the ASCII scheme and host, which are case-insensitive by
//! RFC 3986 definition.

use crate::error::UrlError;
use std::borrow::Cow;

/// The pieces of a URL that Associated Domains matching cares about.
///
/// Every component borrows from the input string. Only the scheme and host can allocate, and only
/// when they actually contain uppercase letters — matching a URL should not cost six heap
/// allocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlParts<'a> {
    scheme: Cow<'a, str>,
    host: Cow<'a, str>,
    port: Option<&'a str>,
    path: &'a str,
    query: &'a str,
    fragment: &'a str,
}

/// Lowercases only when something is actually uppercase, which is rare in practice.
fn ascii_lower(input: &str) -> Cow<'_, str> {
    if input.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(input.to_ascii_lowercase())
    } else {
        Cow::Borrowed(input)
    }
}

impl<'a> UrlParts<'a> {
    /// Splits an absolute URL into its components.
    ///
    /// # Errors
    ///
    /// Returns [`UrlError`] when the input has no scheme, no `//` authority, or an empty host.
    pub fn parse(input: &'a str) -> Result<Self, UrlError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(UrlError::new("URL is empty"));
        }

        let scheme_end = trimmed.find(':').ok_or_else(|| {
            UrlError::new("URL has no scheme; expected something like https://host/path")
        })?;
        let scheme = &trimmed[..scheme_end];
        if scheme.is_empty() || !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(UrlError::new(format!(
                "`{scheme}` is not a valid URL scheme"
            )));
        }
        if !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            return Err(UrlError::new(format!(
                "`{scheme}` is not a valid URL scheme"
            )));
        }

        let rest = &trimmed[scheme_end + 1..];
        let rest = rest
            .strip_prefix("//")
            .ok_or_else(|| UrlError::new("URL has no `//` authority component"))?;

        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let mut remainder = &rest[authority_end..];

        let host_port = authority
            .rsplit_once('@')
            .map_or(authority, |(_userinfo, host_port)| host_port);
        let (host, port) = split_host_port(host_port)?;
        if host.is_empty() {
            return Err(UrlError::new("URL has an empty host"));
        }

        let mut fragment = "";
        if let Some(index) = remainder.find('#') {
            fragment = &remainder[index + 1..];
            remainder = &remainder[..index];
        }

        let mut query = "";
        if let Some(index) = remainder.find('?') {
            query = &remainder[index + 1..];
            remainder = &remainder[..index];
        }

        let path = if remainder.is_empty() { "/" } else { remainder };

        Ok(Self {
            scheme: ascii_lower(scheme),
            host: ascii_lower(host),
            port,
            path,
            query,
            fragment,
        })
    }

    /// The lowercased scheme, without the trailing colon.
    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// The lowercased host, without userinfo or port.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The explicit port, if the URL carried one.
    #[must_use]
    pub fn port(&self) -> Option<&'a str> {
        self.port
    }

    /// The path exactly as written, always starting with `/`.
    #[must_use]
    pub fn path(&self) -> &'a str {
        self.path
    }

    /// The query exactly as written, without the leading `?`. Empty when absent.
    #[must_use]
    pub fn query(&self) -> &'a str {
        self.query
    }

    /// The fragment exactly as written, without the leading `#`. Empty when absent.
    #[must_use]
    pub fn fragment(&self) -> &'a str {
        self.fragment
    }

    /// The query split into `(name, value)` pairs, in source order.
    ///
    /// An item without `=` yields an empty value, matching how `a=1&flag&b=2` is usually read.
    #[must_use]
    pub fn query_items(&self) -> Vec<(&'a str, &'a str)> {
        if self.query.is_empty() {
            return Vec::new();
        }
        self.query
            .split('&')
            .filter(|item| !item.is_empty())
            .map(|item| item.split_once('=').unwrap_or((item, "")))
            .collect()
    }
}

fn split_host_port(host_port: &str) -> Result<(&str, Option<&str>), UrlError> {
    if let Some(end) = host_port.strip_prefix('[').and_then(|rest| rest.find(']')) {
        let host = &host_port[..=end + 1];
        let tail = &host_port[end + 2..];
        return match tail.strip_prefix(':') {
            Some(port) => Ok((host, Some(port))),
            None if tail.is_empty() => Ok((host, None)),
            None => Err(UrlError::new("malformed IPv6 authority")),
        };
    }
    match host_port.rsplit_once(':') {
        Some((host, port)) => Ok((host, Some(port))),
        None => Ok((host_port, None)),
    }
}

/// The forms of a URL path a `/` pattern may be compared against.
///
/// Two things are insignificant at the ends of a path, both confirmed against `swcutil`:
///
/// * a trailing run of slashes — `/buy` and `/buy//` are the same path;
/// * the leading slash — a pattern of `abc` matches `/abc`.
///
/// Note what is *not* here: this does not add a trailing slash. An earlier version did, and it
/// made `/id/????` match `/id/481`, because `481/` is four characters. `swcutil` says that does
/// not match, and the conformance corpus caught it.
#[must_use]
pub fn path_forms(path: &str) -> Vec<String> {
    let trimmed = path.trim_end_matches('/');
    let base = if trimmed.is_empty() { "/" } else { trimmed };
    match base.strip_prefix('/') {
        Some(rest) if !rest.is_empty() => vec![base.to_owned(), rest.to_owned()],
        _ => vec![base.to_owned()],
    }
}

/// The forms a `/` pattern itself takes.
///
/// A leading run of slashes collapses to one, a trailing run is dropped, and a pattern ending in
/// `/*` additionally matches the path without that segment: `swcutil` matches `/buy/*` against
/// `/buy`, which is what an author writing `/buy/*` almost always means.
#[must_use]
pub(crate) fn path_pattern_forms(pattern: &str) -> Vec<String> {
    let leading = pattern.starts_with('/');
    let core = pattern.trim_start_matches('/');
    let mut base = if leading {
        format!("/{core}")
    } else {
        core.to_owned()
    };
    // Keep a lone "/" rather than trimming it away to nothing.
    let trimmed = base.trim_end_matches('/');
    base = if trimmed.is_empty() && base.starts_with('/') {
        "/".to_owned()
    } else {
        trimmed.to_owned()
    };

    let mut forms = vec![base.clone()];
    if let Some(prefix) = base.strip_suffix("/*") {
        if !prefix.is_empty() {
            forms.push(prefix.to_owned());
        }
    }
    forms
}

/// Percent-decodes `input`, leaving invalid escapes untouched.
///
/// Decoded bytes that do not form valid UTF-8 are replaced with `U+FFFD` rather than failing,
/// because a pattern still needs to be compared against something.
#[must_use]
pub fn percent_decode(input: &str) -> String {
    if !input.contains('%') {
        return input.to_owned();
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            ) {
                #[allow(clippy::cast_possible_truncation)]
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
