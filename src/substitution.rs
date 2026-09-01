//! Substitution variables: the custom `substitutionVariables` table plus Apple's predefined set.
//!
//! Apple describes every substitution variable the same way — a named list of alternative strings
//! that may themselves contain `?` and `*` but may not reference other variables. The predefined
//! variables are simply lists Apple ships: `$(digit)` is the ten decimal digits, `$(region)` is
//! `Locale.isoRegionCodes`, and so on. This module models all of them uniformly, with the ASCII
//! classes specialised to a single-character test and the ISO lists to a binary search.

use crate::iso_tables;
use std::collections::BTreeMap;

/// A predefined ASCII character class, matching exactly one character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CharClass {
    Alpha,
    Upper,
    Lower,
    Alnum,
    Digit,
    XDigit,
}

impl CharClass {
    pub(crate) fn matches(self, value: char, case_sensitive: bool) -> bool {
        match self {
            Self::Alpha => value.is_ascii_alphabetic(),
            // The class is a list of strings, so folding applies to it exactly as it applies to
            // any other alternative: with `caseSensitive: false`, `$(upper)` also accepts `a`.
            Self::Upper => {
                if case_sensitive {
                    value.is_ascii_uppercase()
                } else {
                    value.is_ascii_alphabetic()
                }
            }
            Self::Lower => {
                if case_sensitive {
                    value.is_ascii_lowercase()
                } else {
                    value.is_ascii_alphabetic()
                }
            }
            Self::Alnum => value.is_ascii_alphanumeric(),
            Self::Digit => value.is_ascii_digit(),
            Self::XDigit => value.is_ascii_hexdigit(),
        }
    }
}

/// What a `$(name)` reference resolved to.
pub(crate) enum Resolved<'a> {
    /// A predefined single-character ASCII class.
    Class(CharClass),
    /// A predefined list backed by a sorted static table.
    Table {
        exact: &'static [&'static str],
        lower: &'static [&'static str],
        lengths: &'static [usize],
    },
    /// A user-defined list from `substitutionVariables`.
    Custom(&'a [String]),
}

/// Every predefined variable name, in documentation order.
pub(crate) const PREDEFINED: &[&str] = &[
    "alpha", "upper", "lower", "alnum", "digit", "xdigit", "region", "lang",
];

/// The Foundation release the `$(region)` and `$(lang)` tables were captured from.
pub(crate) const ISO_TABLE_SOURCE: &str = iso_tables::SOURCE;

/// Resolves custom and predefined `$(name)` references.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SubstitutionTable {
    custom: BTreeMap<String, Vec<String>>,
}

impl SubstitutionTable {
    pub(crate) fn from_custom(custom: BTreeMap<String, Vec<String>>) -> Self {
        Self { custom }
    }

    /// Whether `name` is one of Apple's predefined variables.
    pub(crate) fn is_predefined(name: &str) -> bool {
        PREDEFINED.contains(&name)
    }

    /// Resolves `name`, preferring an explicit `substitutionVariables` entry.
    ///
    /// A custom entry shadowing a predefined name is honoured — what the document says wins — but
    /// the validator reports it, because it is almost always a mistake.
    pub(crate) fn resolve(&self, name: &str) -> Option<Resolved<'_>> {
        if let Some(values) = self.custom.get(name) {
            return Some(Resolved::Custom(values));
        }
        Some(match name {
            "alpha" => Resolved::Class(CharClass::Alpha),
            "upper" => Resolved::Class(CharClass::Upper),
            "lower" => Resolved::Class(CharClass::Lower),
            "alnum" => Resolved::Class(CharClass::Alnum),
            "digit" => Resolved::Class(CharClass::Digit),
            "xdigit" => Resolved::Class(CharClass::XDigit),
            "region" => Resolved::Table {
                exact: iso_tables::REGIONS,
                lower: iso_tables::REGIONS_LOWER,
                lengths: iso_tables::REGIONS_LENGTHS,
            },
            "lang" => Resolved::Table {
                exact: iso_tables::LANGS,
                lower: iso_tables::LANGS_LOWER,
                lengths: iso_tables::LANGS_LENGTHS,
            },
            _ => return None,
        })
    }
}
