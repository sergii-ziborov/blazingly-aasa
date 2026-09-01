//! Apple's wildcard pattern language, compiled to a backtracking-free matcher.
//!
//! The language has three wildcards (`*` zero or more, `?` exactly one, and therefore `?*` for
//! one or more) plus `$(name)` substitution references. Rather than translating to a regular
//! expression — where a pattern such as `*a*a*a*a*b` can blow up — patterns are compiled to a
//! token sequence and evaluated as an NFA over a bitset of reachable character positions. That
//! makes matching `O(tokens x positions)` in the worst case with no backtracking at all.
//!
//! Most real patterns (`/buy/*`, `/help/website/*`, `no_universal_links`) never reach the general
//! engine: they compile to an allocation-free literal, prefix, suffix, or contains test.

use crate::substitution::{CharClass, Resolved, SubstitutionTable};

/// Simple, 1:1 lowercase folding.
///
/// ASCII folds exactly. Non-ASCII uses the first scalar of `char::to_lowercase`, which keeps the
/// fold length-preserving so character positions stay meaningful. Full case folding (where one
/// scalar expands to several) is deliberately out of scope; see `docs/parity.md`.
#[inline]
fn fold(value: char) -> char {
    if value.is_ascii() {
        value.to_ascii_lowercase()
    } else {
        value.to_lowercase().next().unwrap_or(value)
    }
}

#[inline]
fn char_eq(left: char, right: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        left == right
    } else {
        fold(left) == fold(right)
    }
}

/// Compares two strings under the same folding rules patterns use.
///
/// Shared with query-item name comparison so that `caseSensitive: false` means one thing across a
/// rule rather than ASCII-only folding for names and full folding for values.
pub(crate) fn str_eq(left: &str, right: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        return left == right;
    }
    let mut left = left.chars();
    let mut right = right.chars();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) if char_eq(a, b, false) => {}
            _ => return false,
        }
    }
}

fn starts_with(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        return haystack.starts_with(needle);
    }
    let mut haystack = haystack.chars();
    for expected in needle.chars() {
        match haystack.next() {
            Some(actual) if char_eq(actual, expected, false) => {}
            _ => return false,
        }
    }
    true
}

fn ends_with(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        return haystack.ends_with(needle);
    }
    let mut haystack = haystack.chars().rev();
    for expected in needle.chars().rev() {
        match haystack.next() {
            Some(actual) if char_eq(actual, expected, false) => {}
            _ => return false,
        }
    }
    true
}

fn contains(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        return haystack.contains(needle);
    }
    if needle.is_empty() {
        return true;
    }
    let mut cursor = haystack;
    loop {
        if starts_with(cursor, needle, false) {
            return true;
        }
        let mut chars = cursor.chars();
        if chars.next().is_none() {
            return false;
        }
        cursor = chars.as_str();
    }
}

/// Random access to the input as characters.
///
/// URL components are overwhelmingly ASCII, where a byte *is* a character; that case borrows the
/// string directly and never allocates. Non-ASCII input falls back to a `Vec<char>` so that `?`
/// keeps meaning "one character" rather than "one byte". The trait is generic rather than an enum
/// so the hot loop has no per-character branch.
trait Text {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> char;
}

impl Text for &[u8] {
    #[inline]
    fn len(&self) -> usize {
        <[u8]>::len(self)
    }

    #[inline]
    fn get(&self, index: usize) -> char {
        char::from(self[index])
    }
}

impl Text for &[char] {
    #[inline]
    fn len(&self) -> usize {
        <[char]>::len(self)
    }

    #[inline]
    fn get(&self, index: usize) -> char {
        self[index]
    }
}

const INLINE_WORDS: usize = 8;

/// A set of reachable character positions in `0..=len`.
#[derive(Clone)]
struct Bits {
    inline: [u64; INLINE_WORDS],
    heap: Vec<u64>,
    words: usize,
    len: usize,
}

impl Bits {
    fn new(len: usize) -> Self {
        let words = len / 64 + 1;
        Self {
            inline: [0; INLINE_WORDS],
            heap: if words > INLINE_WORDS {
                vec![0; words]
            } else {
                Vec::new()
            },
            words,
            len,
        }
    }

    #[inline]
    fn words(&self) -> &[u64] {
        if self.words > INLINE_WORDS {
            &self.heap
        } else {
            &self.inline[..self.words]
        }
    }

    #[inline]
    fn words_mut(&mut self) -> &mut [u64] {
        if self.words > INLINE_WORDS {
            &mut self.heap
        } else {
            &mut self.inline[..self.words]
        }
    }

    #[inline]
    fn set(&mut self, index: usize) {
        debug_assert!(index <= self.len);
        self.words_mut()[index / 64] |= 1u64 << (index % 64);
    }

    #[inline]
    fn get(&self, index: usize) -> bool {
        index <= self.len && self.words()[index / 64] & (1u64 << (index % 64)) != 0
    }

    fn is_empty(&self) -> bool {
        self.words().iter().all(|word| *word == 0)
    }

    fn lowest(&self) -> Option<usize> {
        self.words()
            .iter()
            .enumerate()
            .find(|(_, word)| **word != 0)
            .map(|(index, word)| index * 64 + word.trailing_zeros() as usize)
    }

    /// Sets every position in `start..=self.len`.
    fn set_tail(&mut self, start: usize) {
        let len = self.len;
        for index in start..=len {
            self.set(index);
        }
    }

    /// Shifts every position up by one, dropping anything past `len`.
    fn shift_one(&self) -> Self {
        let mut out = Self::new(self.len);
        let mut carry = 0u64;
        {
            let source = self.words();
            let target = out.words_mut();
            for index in 0..source.len() {
                target[index] = (source[index] << 1) | carry;
                carry = source[index] >> 63;
            }
        }
        out.mask();
        out
    }

    /// Clears bits above `len`, which shifting can introduce.
    fn mask(&mut self) {
        let len = self.len;
        let last = len / 64;
        let bit = len % 64;
        let words = self.words_mut();
        if bit == 63 {
            // Nothing above `len` inside the final word.
        } else {
            words[last] &= (1u64 << (bit + 1)) - 1;
        }
        for word in words.iter_mut().skip(last + 1) {
            *word = 0;
        }
    }

    fn union_with(&mut self, other: &Self) {
        let source: Vec<u64> = other.words().to_vec();
        for (target, value) in self.words_mut().iter_mut().zip(source) {
            *target |= value;
        }
    }

    /// Iterates set positions, skipping empty words instead of scanning every index.
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words()
            .iter()
            .enumerate()
            .flat_map(|(word_index, word)| {
                let mut remaining = *word;
                std::iter::from_fn(move || {
                    if remaining == 0 {
                        return None;
                    }
                    let bit = remaining.trailing_zeros() as usize;
                    remaining &= remaining - 1;
                    Some(word_index * 64 + bit)
                })
            })
    }
}

/// One element of a compiled pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Literal(Box<[char]>),
    AnyOne,
    AnyMany,
    Class(CharClass),
    /// A predefined variable backed by a sorted static table, such as `$(region)`.
    Table {
        exact: &'static [&'static str],
        lower: &'static [&'static str],
        lengths: &'static [usize],
    },
    /// A custom `$(name)` reference: any one of these alternatives, each a pattern of its own.
    Alternatives(Box<[Box<[Token]>]>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// `*`
    Any,
    /// No wildcards at all.
    Exact(Box<str>),
    /// `literal*`
    Prefix(Box<str>),
    /// `*literal`
    Suffix(Box<str>),
    /// `*literal*`
    Contains(Box<str>),
    /// Only literals, `?`, `*`, and single-character classes: matched greedily, without
    /// allocating.
    Glob(Box<[Token]>),
    /// Contains a multi-character alternative set, which needs the NFA.
    General(Box<[Token]>),
}

/// What a pattern is made of, used to explain why it matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `*` on its own.
    Any,
    /// No wildcards and no substitutions.
    Literal,
    /// Contains `*` or `?`.
    Wildcard,
    /// Contains at least one `$(...)` reference.
    Substitution,
}

/// A compiled Apple URL-component pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pattern {
    source: Box<str>,
    case_sensitive: bool,
    shape: Shape,
    kind: Kind,
}

/// A problem found while compiling a pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PatternError {
    UnterminatedReference,
    UnknownVariable(String),
    NestedSubstitution { variable: String, value: String },
    EmptyVariable(String),
}

impl Pattern {
    /// Compiles `source`, resolving `$(name)` references through `table`.
    ///
    /// Errors are accumulated rather than returned so a linter can report every problem in one
    /// pass; a pattern with errors still compiles to something that simply never matches.
    pub(crate) fn compile(
        source: &str,
        case_sensitive: bool,
        table: &SubstitutionTable,
        errors: &mut Vec<PatternError>,
    ) -> Self {
        let before = errors.len();
        let tokens = tokenize(source, table, errors, true);
        let shape = shape_of(&tokens);
        let kind = if errors.len() > before {
            // A pattern we could not fully understand must never claim a match.
            Kind::General(Box::new([]))
        } else {
            classify(tokens)
        };
        Self {
            source: source.into(),
            case_sensitive,
            shape,
            kind,
        }
    }

    /// What the pattern is made of.
    pub(crate) fn shape(&self) -> Shape {
        self.shape
    }

    /// The pattern text exactly as it appeared in the document.
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// Whether this pattern matches every possible input.
    pub(crate) fn is_any(&self) -> bool {
        matches!(self.kind, Kind::Any)
    }

    /// Whether the whole of `input` matches, under the pattern's own case sensitivity.
    pub(crate) fn matches(&self, input: &str) -> bool {
        self.matches_with(input, self.case_sensitive)
    }

    /// Whether the whole of `input` matches, overriding case sensitivity.
    ///
    /// Case sensitivity is a match-time parameter rather than something baked into the compiled
    /// form, which lets a failed comparison be retried case-insensitively to report
    /// [`ComponentReason::CaseMismatch`](crate::ComponentReason::CaseMismatch).
    pub(crate) fn matches_with(&self, input: &str, case_sensitive: bool) -> bool {
        match &self.kind {
            Kind::Any => true,
            Kind::Exact(literal) => str_eq(input, literal, case_sensitive),
            Kind::Prefix(literal) => starts_with(input, literal, case_sensitive),
            Kind::Suffix(literal) => ends_with(input, literal, case_sensitive),
            Kind::Contains(literal) => contains(input, literal, case_sensitive),
            Kind::Glob(tokens) => {
                if input.is_ascii() {
                    glob_match(tokens, &input.as_bytes(), case_sensitive)
                } else {
                    let chars: Vec<char> = input.chars().collect();
                    glob_match(tokens, &chars.as_slice(), case_sensitive)
                }
            }
            Kind::General(tokens) => {
                if tokens.is_empty() {
                    return false;
                }
                if input.is_ascii() {
                    general_match(tokens, &input.as_bytes(), case_sensitive)
                } else {
                    let chars: Vec<char> = input.chars().collect();
                    general_match(tokens, &chars.as_slice(), case_sensitive)
                }
            }
        }
    }
}

/// How many characters `token` consumes at `position`, if it matches there.
#[inline]
fn consume<T: Text>(
    token: &Token,
    text: &T,
    position: usize,
    case_sensitive: bool,
) -> Option<usize> {
    match token {
        Token::Literal(literal) => {
            let end = position + literal.len();
            (end <= text.len()
                && literal.iter().enumerate().all(|(offset, expected)| {
                    char_eq(text.get(position + offset), *expected, case_sensitive)
                }))
            .then_some(literal.len())
        }
        Token::AnyOne => (position < text.len()).then_some(1),
        Token::Class(class) => (position < text.len()
            && class.matches(text.get(position), case_sensitive))
        .then_some(1),
        Token::AnyMany | Token::Alternatives(_) | Token::Table { .. } => None,
    }
}

/// The classical greedy glob algorithm, generalised from characters to tokens.
///
/// Only the most recent `*` is ever reconsidered, which is what makes this correct for patterns
/// built from `*`, `?`, literals, and single-character classes. It uses no heap and is bounded by
/// `O(positions x tokens)` — the same bound as the NFA, with a far smaller constant. Patterns that
/// contain a multi-character substitution set need real alternation and go to the NFA instead.
fn glob_match<T: Text>(tokens: &[Token], text: &T, case_sensitive: bool) -> bool {
    let mut position = 0usize;
    let mut index = 0usize;
    let mut star: Option<usize> = None;
    let mut star_position = 0usize;

    loop {
        if index < tokens.len() {
            if matches!(tokens[index], Token::AnyMany) {
                star = Some(index);
                star_position = position;
                index += 1;
                continue;
            }
            if let Some(width) = consume(&tokens[index], text, position, case_sensitive) {
                position += width;
                index += 1;
                continue;
            }
        } else if position == text.len() {
            return true;
        }

        // Either a token failed or the pattern ran out with text left over. Let the most recent
        // `*` swallow one more character and resume from there.
        let Some(resume) = star else { return false };
        star_position += 1;
        if star_position > text.len() {
            return false;
        }
        position = star_position;
        index = resume + 1;
    }
}

fn general_match<T: Text>(tokens: &[Token], text: &T, case_sensitive: bool) -> bool {
    let mut state = Bits::new(text.len());
    state.set(0);
    let state = run(tokens, &state, text, case_sensitive);
    state.get(text.len())
}

fn run<T: Text>(tokens: &[Token], start: &Bits, text: &T, case_sensitive: bool) -> Bits {
    let mut state = start.clone();
    for token in tokens {
        if state.is_empty() {
            return state;
        }
        state = step(token, &state, text, case_sensitive);
    }
    state
}

fn step<T: Text>(token: &Token, state: &Bits, text: &T, case_sensitive: bool) -> Bits {
    match token {
        Token::AnyOne => state.shift_one(),
        Token::AnyMany => {
            let mut next = Bits::new(text.len());
            if let Some(lowest) = state.lowest() {
                next.set_tail(lowest);
            }
            next
        }
        Token::Literal(_) | Token::Class(_) => {
            let mut next = Bits::new(text.len());
            for position in state.iter() {
                if let Some(width) = consume(token, text, position, case_sensitive) {
                    next.set(position + width);
                }
            }
            next
        }
        Token::Table {
            exact,
            lower,
            lengths,
        } => {
            let entries = if case_sensitive { exact } else { lower };
            let mut next = Bits::new(text.len());
            for position in state.iter() {
                for length in *lengths {
                    let end = position + length;
                    if end <= text.len()
                        && entries
                            .binary_search_by(|entry| {
                                compare_entry(entry, text, position, end, case_sensitive)
                            })
                            .is_ok()
                    {
                        next.set(end);
                    }
                }
            }
            next
        }
        Token::Alternatives(alternatives) => {
            let mut next = Bits::new(text.len());
            for alternative in alternatives.iter() {
                let reached = run(alternative, state, text, case_sensitive);
                next.union_with(&reached);
            }
            next
        }
    }
}

/// Orders a table entry against a slice of input characters.
///
/// When matching case-insensitively the table is already lowercase, so only the input is folded.
fn compare_entry<T: Text>(
    entry: &str,
    text: &T,
    start: usize,
    end: usize,
    case_sensitive: bool,
) -> std::cmp::Ordering {
    let mut entry = entry.chars();
    let mut input = (start..end).map(|index| text.get(index));
    loop {
        match (entry.next(), input.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(left), Some(right)) => {
                let right = if case_sensitive { right } else { fold(right) };
                match left.cmp(&right) {
                    std::cmp::Ordering::Equal => {}
                    other => return other,
                }
            }
        }
    }
}

fn tokenize(
    source: &str,
    table: &SubstitutionTable,
    errors: &mut Vec<PatternError>,
    allow_substitutions: bool,
) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut literal: Vec<char> = Vec::new();
    let mut chars = source.chars().peekable();

    let flush = |literal: &mut Vec<char>, tokens: &mut Vec<Token>| {
        if !literal.is_empty() {
            tokens.push(Token::Literal(std::mem::take(literal).into_boxed_slice()));
        }
    };

    while let Some(current) = chars.next() {
        match current {
            '*' => {
                flush(&mut literal, &mut tokens);
                if !matches!(tokens.last(), Some(Token::AnyMany)) {
                    tokens.push(Token::AnyMany);
                }
            }
            '?' => {
                flush(&mut literal, &mut tokens);
                tokens.push(Token::AnyOne);
            }
            '$' if chars.peek() == Some(&'(') => {
                chars.next();
                let mut name = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == ')' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if !closed {
                    errors.push(PatternError::UnterminatedReference);
                    return tokens;
                }
                flush(&mut literal, &mut tokens);
                if !allow_substitutions {
                    errors.push(PatternError::NestedSubstitution {
                        variable: name.clone(),
                        value: source.to_owned(),
                    });
                    continue;
                }
                match table.resolve(&name) {
                    Some(Resolved::Class(class)) => tokens.push(Token::Class(class)),
                    Some(Resolved::Table {
                        exact,
                        lower,
                        lengths,
                    }) => tokens.push(Token::Table {
                        exact,
                        lower,
                        lengths,
                    }),
                    Some(Resolved::Custom(values)) => {
                        if values.is_empty() {
                            errors.push(PatternError::EmptyVariable(name.clone()));
                            continue;
                        }
                        let alternatives: Vec<Box<[Token]>> = values
                            .iter()
                            .map(|value| tokenize(value, table, errors, false).into_boxed_slice())
                            .collect();
                        tokens.push(Token::Alternatives(alternatives.into_boxed_slice()));
                    }
                    None => errors.push(PatternError::UnknownVariable(name)),
                }
            }
            other => literal.push(other),
        }
    }
    flush(&mut literal, &mut tokens);
    tokens
}

fn shape_of(tokens: &[Token]) -> Shape {
    if tokens.iter().any(|token| {
        matches!(
            token,
            Token::Class(_) | Token::Table { .. } | Token::Alternatives(_)
        )
    }) {
        return Shape::Substitution;
    }
    if matches!(tokens, [Token::AnyMany]) {
        return Shape::Any;
    }
    if tokens
        .iter()
        .any(|token| matches!(token, Token::AnyOne | Token::AnyMany))
    {
        return Shape::Wildcard;
    }
    Shape::Literal
}

fn classify(tokens: Vec<Token>) -> Kind {
    let literal_text = |literal: &[char]| -> Box<str> { literal.iter().collect::<String>().into() };
    match tokens.as_slice() {
        [] => Kind::Exact(String::new().into()),
        [Token::AnyMany] => Kind::Any,
        [Token::Literal(literal)] => Kind::Exact(literal_text(literal)),
        [Token::Literal(literal), Token::AnyMany] => Kind::Prefix(literal_text(literal)),
        [Token::AnyMany, Token::Literal(literal)] => Kind::Suffix(literal_text(literal)),
        [Token::AnyMany, Token::Literal(literal), Token::AnyMany] => {
            Kind::Contains(literal_text(literal))
        }
        _ => {
            if tokens
                .iter()
                .any(|token| matches!(token, Token::Alternatives(_) | Token::Table { .. }))
            {
                Kind::General(tokens.into_boxed_slice())
            } else {
                Kind::Glob(tokens.into_boxed_slice())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(source: &str, case_sensitive: bool) -> Pattern {
        let table = SubstitutionTable::default();
        let mut errors = Vec::new();
        let pattern = Pattern::compile(source, case_sensitive, &table, &mut errors);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        pattern
    }

    #[test]
    fn star_matches_zero_or_more() {
        let pattern = compile("/buy/*", true);
        assert!(pattern.matches("/buy/"));
        assert!(pattern.matches("/buy/42"));
        assert!(!pattern.matches("/buy"));
        assert!(!pattern.matches("/sell/42"));
    }

    #[test]
    fn question_matches_exactly_one() {
        let pattern = compile("????", true);
        assert!(pattern.matches("4815"));
        assert!(!pattern.matches("481"));
        assert!(!pattern.matches("48159"));
    }

    #[test]
    fn question_star_matches_one_or_more() {
        let pattern = compile("/a/?*", true);
        assert!(!pattern.matches("/a/"));
        assert!(pattern.matches("/a/b"));
        assert!(pattern.matches("/a/bcd"));
    }

    #[test]
    fn case_insensitive_folding() {
        let sensitive = compile("/Help/*", true);
        assert!(!sensitive.matches("/help/1"));
        let insensitive = compile("/Help/*", false);
        assert!(insensitive.matches("/help/1"));
        assert!(insensitive.matches("/HELP/1"));
    }

    #[test]
    fn adversarial_pattern_does_not_blow_up() {
        let pattern = compile("*a*a*a*a*a*a*a*a*a*a*a*a*b", true);
        let haystack = "a".repeat(4096);
        assert!(!pattern.matches(&haystack));
    }

    #[test]
    fn unicode_positions_are_characters_not_bytes() {
        let pattern = compile("/?/", true);
        assert!(pattern.matches("/é/"));
        assert!(!pattern.matches("/éé/"));
    }

    #[test]
    fn fast_paths_agree_with_general_engine() {
        for (source, input) in [
            ("*", "anything"),
            ("abc", "abc"),
            ("abc*", "abcdef"),
            ("*abc", "xxabc"),
            ("*abc*", "xxabcyy"),
        ] {
            let fast = compile(source, true);
            // Force the general engine by appending a redundant `?`-free construct.
            let general = Pattern {
                source: source.into(),
                case_sensitive: true,
                shape: Shape::Wildcard,
                kind: Kind::General(
                    tokenize(source, &SubstitutionTable::default(), &mut Vec::new(), true)
                        .into_boxed_slice(),
                ),
            };
            assert_eq!(
                fast.matches(input),
                general.matches(input),
                "mismatch for {source} against {input}"
            );
        }
    }
}
