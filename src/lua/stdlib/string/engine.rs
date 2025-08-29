//! A Lua-compliant pattern matching engine adapted for UTF-8 strings.

// This code is adapted from PUC-Lua 5.4.8. The upstream copyright is:
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyright: 1994-2025 Lua.org, PUC-Rio

use core::slice::SliceIndex;
use piccolo::{Context, Error as VmError, TypeError, Value};
use std::ops::Range;
use unicode_general_category::{GeneralCategory, get_general_category};

/// A pattern string parsing error.
// Clippy: The error fields are self-explanatory.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Eq, thiserror::Error, PartialEq)]
pub enum Error {
    /// The pattern caused too much recursion.
    #[error("pattern too complex at {pos}")]
    TooComplex { pos: usize },
    /// The pattern contains more captures than can be stored.
    #[error("too many captures at {pos}")]
    TooManyCaptures { pos: usize },
    /// The pattern contains an invalid capture group.
    #[error("invalid pattern capture at {pos}")]
    InvalidPatternCapture { pos: usize },
    /// The pattern contains an incomplete frontier class.
    #[error("missing '[' after '%f' in pattern at {pos}")]
    IncompleteFrontier { pos: usize },
    /// The pattern contains a malformed balance class.
    #[error("malformed pattern (missing arguments to '%b') at {pos}")]
    MissingBalanceArgs { pos: usize },
    /// The pattern tries capturing an index which does not exist.
    #[error("invalid capture index %{index} at {pos}")]
    InvalidCaptureIndex { pos: usize, index: usize },
    /// The pattern ends in the middle of a character class.
    #[error("malformed pattern (ends with '%') at {pos}")]
    EndsWithPercent { pos: usize },
    /// The pattern ends in the middle of a character set.
    #[error("malformed pattern (missing ']') at {pos}")]
    EndsWithoutBracket { pos: usize },
    /// The pattern ends in the middle of a capture group.
    #[error("unfinished capture at {pos}")]
    UnfinishedCapture { pos: usize },
    /// A substitution replacement string contains an invalid '%' symbol.
    #[error("invalid use of '%' in replacement string")]
    InvalidReplacement,
}

/// The standard [`Result`](core::result::Result) type used by the pattern
/// matching engine.
pub(super) type Result<T, E = Error> = core::result::Result<T, E>;

/// The type of a captured string.
pub(super) type Capture<'a> = piccolo::Value<'a>;

/// The maximum number of allowed capture groups in a pattern.
const LUA_MAXCAPTURES: usize = 32;

/// A capture group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CaptureRange {
    /// A substring capture group.
    Range(Range<usize>),
    /// A current string position capture group.
    Position(usize),
}

impl CaptureRange {
    /// Converts the captured group into a Lua value, consuming this object.
    #[must_use]
    pub fn into_value<'gc, B: BackingType + ?Sized>(
        self,
        ctx: Context<'gc>,
        text: &B,
    ) -> Value<'gc> {
        match self {
            CaptureRange::Range(range) => ctx.intern(&text.as_bytes()[range]).into(),
            CaptureRange::Position(at) => i64::try_from(text.char_count(at).saturating_add(1))
                .unwrap()
                .into(),
        }
    }
}

impl Default for CaptureRange {
    fn default() -> Self {
        Self::Range(<_>::default())
    }
}

/// The ranged indexes of a matched pattern. These are always 0-indexed.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct MatchRanges {
    /// The full range of the matched pattern.
    pub full_match: Range<usize>,
    /// The ranges of each captured group. If a group did not capture anything,
    /// the range will be empty.
    pub captures: Vec<CaptureRange>,
}

/// Tries to find the first match of the pattern in the input string,
/// starting the search at `start_index` (0-based).
/// Returns the range of the full match and the ranges of captures if successful.
pub(super) fn find_first_match<B: BackingType + ?Sized>(
    input: &B,
    pattern: &B,
    start_index: usize,
    anchor: bool,
) -> Result<Option<MatchRanges>> {
    let is_anchored = anchor && pattern.starts_with(B::Primitive::from_ascii(b'^'));
    let pattern = if is_anchored { &pattern[1..] } else { pattern };

    for (start, _) in input.start_scan(start_index) {
        let mut state = State {
            input,
            pattern,
            level: 0,
            depth: MAX_RECURSION_DEPTH,
            captures: <_>::default(),
        };

        if let Some(end) = next_match(&mut state, start, 0)? {
            let full_match = start..end;
            return Ok(Some(MatchRanges {
                full_match,
                captures: state
                    .captures
                    .into_iter()
                    .take(state.level)
                    .map(|capture| CaptureRange::try_from((pattern, capture)))
                    .collect::<Result<_, _>>()?,
            }));
        }

        if is_anchored {
            break;
        }
    }

    Ok(None)
}

/// The main pattern matching function.
#[allow(clippy::too_many_lines)]
fn next_match<B: BackingType + ?Sized>(
    state: &mut State<'_, B>,
    mut s: usize,
    mut p: usize,
) -> Result<Option<usize>> {
    if state.depth == 0 {
        return Err(Error::TooComplex {
            pos: state.char_pos(p),
        });
    }

    state.depth -= 1;

    // A loop is used to avoid unnecessary recursion. Because the matching
    // engine tracks recursion explicitly in order to abort pathological cases,
    // it is not enough to rely on the compiler to set up tail calls anyway.
    let s = loop {
        let mut i = state.pattern[p..].chars();
        let Some(c) = i.next() else {
            break Some(s);
        };

        // Special items: captures, anchors, balances, and frontiers
        match c.as_ascii() {
            b'(' => {
                // It is possible we are at the end of an invalid pattern here.
                let (p, is_position) = if i.next() == Some(B::Primitive::from_ascii(b')')) {
                    (p + 2, true)
                } else {
                    (p + 1, false)
                };
                break state.start_capture(s, p, is_position)?;
            }
            b')' => break state.end_capture(s, p + 1)?,
            b'$' => {
                if p + 1 != state.pattern.len() {
                    // Literal '$' in pattern, not an anchor. Process it as a
                    // normal character by allowing code flow to continue.
                } else if s == state.input.len() {
                    // Anchor in pattern at the end of input.
                    break Some(s);
                } else {
                    // Anchor in pattern, but not at the end of input.
                    break None;
                }
            }
            b'%' => match i.next().map(PrimitiveType::as_ascii) {
                Some(b'b') => {
                    if let Some((next, len)) = state.match_balance(s, p + 2)? {
                        // Balance sub-match succeeded. Advance input and step
                        // to the next token.
                        s = next;
                        p += 2 + len;
                        continue;
                    }

                    // Balanced did not match.
                    break None;
                }
                Some(b'f') => {
                    // Advance pattern to parse the frontier set.
                    p += 2;
                    if i.next() != Some(B::Primitive::from_ascii(b'[')) {
                        return Err(Error::IncompleteFrontier {
                            pos: state.char_pos(p),
                        });
                    }
                    let p_after = state.class_end(p)?;

                    // Lua manual: “The beginning and end of the subject are
                    // handled as if they were the character '\0'.”
                    let first = state.input[..s]
                        .chars()
                        .next_back()
                        .unwrap_or(B::Primitive::from_ascii(b'\0'));
                    let last = state.input[s..]
                        .chars()
                        .next()
                        .unwrap_or(B::Primitive::from_ascii(b'\0'));

                    if !state.is_in_set(first, p, p_after - 1)
                        && state.is_in_set(last, p, p_after - 1)
                    {
                        // Matched; advance the pattern and continue.
                        p = p_after;
                        continue;
                    }

                    // Frontier did not match.
                    break None;
                }
                Some(level @ b'0'..=b'9') => {
                    if let Some(next) =
                        state.match_capture(s, p, B::Primitive::from_ascii(level))?
                    {
                        // Matched; advance the pattern and the input and
                        // continue.
                        s = next;
                        p += 2;
                        continue;
                    }

                    // Captured string did not match.
                    break None;
                }
                _ => {
                    // This is actually a single character class, so handle
                    // it below.
                }
            },
            _ => {
                // This is actually a normal character, so handle it below.
            }
        }

        // Normal characters and character classes
        let p_after = state.class_end(p)?;
        // It is possible the character class is at the end of the pattern.
        let quantifier = state.pattern[p_after..]
            .chars()
            .next()
            .map(PrimitiveType::as_ascii);
        if let Some(c_len) = state.is_single_match(s, p, p_after) {
            match quantifier {
                Some(b'?') => {
                    if let item @ Some(_) = next_match(state, s + c_len, p_after + 1)? {
                        // Matched one item successfully
                        break item;
                    }

                    // Matched zero items successfully
                    p = p_after + 1;
                }
                Some(b'+' | b'*') => {
                    // For '+', one item was already matched by `single_match`
                    if quantifier == Some(b'+') {
                        s += c_len;
                    }

                    // Match zero or more, greedily
                    break state.max_expand(s, p, p_after)?;
                }
                Some(b'-') => break state.min_expand(s, p, p_after)?,
                _ => {
                    // It was not a quantifier after all, but some other
                    // character literal that matched
                    s += c_len;
                    p = p_after;
                }
            }
        } else if matches!(quantifier, Some(b'*' | b'?' | b'-')) {
            // Nothing matched `is_single_match`. Is it OK?
            p = p_after + 1;
        } else {
            // No, it is not OK. This is a failure condition.
            break None;
        }
    };

    state.depth += 1;
    Ok(s)
}

/// Pattern matching engine state.
///
/// This struct does not represent the complete state of the matching engine;
/// the pattern and input cursors are maintained on the stack in the `p` and `s`
/// variables, respectively.
struct State<'a, B: BackingType + ?Sized> {
    /// The input string to match.
    input: &'a B,
    /// The pattern to match.
    pattern: &'a B,
    /// Recursion depth of `full_match`.
    depth: usize,
    /// Number of capture groups.
    level: usize,
    /// Intermediate capture group states.
    captures: [CaptureState; LUA_MAXCAPTURES],
}

impl<B: BackingType + ?Sized> State<'_, B> {
    /// Returns the 1-indexed character count at the given byte position.
    #[inline]
    fn char_pos(&self, offset: usize) -> usize {
        char_pos(self.pattern, offset)
    }

    /// Matches a pattern balance item. If successful, returns the next position
    /// of the input and the size of the balance characters.
    fn match_balance(&self, s: usize, p: usize) -> Result<Option<(usize, usize)>> {
        if p >= self.pattern.len() - 1 {
            return Err(Error::MissingBalanceArgs {
                pos: self.char_pos(p),
            });
        }

        let mut i = self.pattern[p..].chars();

        let open = i.next().unwrap();
        // It is possible that we are at the end of the input.
        if !self.input[s..].starts_with(open) {
            return Ok(None);
        }

        let close = i.next().unwrap();
        let mut count = 1;

        let start = s + open.len_utf8();
        for (len, c) in self.input[start..].char_indices() {
            if c == close {
                count -= 1;
                if count == 0 {
                    return Ok(Some((
                        start + len + c.len_utf8(),
                        open.len_utf8() + close.len_utf8(),
                    )));
                }
            } else if c == open {
                count += 1;
            }
        }

        Ok(None)
    }

    /// Matches the capture group at the given level to the input string.
    /// Returns the next position of the input string if successful.
    fn match_capture(&self, s: usize, p: usize, level: B::Primitive) -> Result<Option<usize>> {
        let range = self.check_capture(p, level)?;
        let end = s + range.len();
        Ok(
            (self.input.as_bytes().get(range.clone()) == self.input.as_bytes().get(s..end))
                .then_some(end),
        )
    }

    /// Takes as many pattern items as possible and then backs off until either
    /// the rest of the pattern matches or there are no more items to give back.
    /// If successful, returns the next position of the input.
    fn max_expand(&mut self, s: usize, p: usize, p_end: usize) -> Result<Option<usize>> {
        let mut i = 0;
        while let Some(c_len) = self.is_single_match(s + i, p, p_end) {
            i += c_len;
        }
        let p_after = p_end
            + self.pattern[p_end..]
                .chars()
                .next()
                .map_or(0, PrimitiveType::len_utf8);
        while i != usize::MAX {
            if let result @ Some(_) = next_match(self, s + i, p_after)? {
                return Ok(result);
            }
            i = i.wrapping_sub(1);
            while i != usize::MAX && !self.input.is_char_boundary(s + i) {
                i = i.wrapping_sub(1);
            }
        }
        Ok(None)
    }

    /// Takes the fewest number of items possible until the rest of the pattern
    /// starts to fail to match. If successful, returns the next position of the
    /// input.
    fn min_expand(&mut self, mut s: usize, p: usize, p_end: usize) -> Result<Option<usize>> {
        let p_after = p_end
            + self.pattern[p_end..]
                .chars()
                .next()
                .map_or(0, PrimitiveType::len_utf8);
        loop {
            if let result @ Some(_) = next_match(self, s, p_after)? {
                break Ok(result);
            } else if let Some(c_len) = self.is_single_match(s, p, p_end) {
                s += c_len;
            } else {
                break Ok(None);
            }
        }
    }

    /// Starts a new capture group. Completes matching the input and returns its
    /// final position if successful.
    fn start_capture(&mut self, s: usize, p: usize, is_position: bool) -> Result<Option<usize>> {
        if self.level >= LUA_MAXCAPTURES {
            return Err(Error::TooManyCaptures {
                pos: self.char_pos(p),
            });
        }

        let slot = &mut self.captures[self.level];

        *slot = if is_position {
            CaptureState::Finished(CaptureRange::Position(s))
        } else {
            CaptureState::Pending { start: s }
        };

        self.level += 1;

        Ok(next_match(self, s, p)?.or_else(|| {
            self.level -= 1;
            None
        }))
    }

    /// Finalises a new capture group. Completes matching the input and returns
    /// its final position if successful.
    fn end_capture(&mut self, s: usize, p: usize) -> Result<Option<usize>> {
        let level = self.capture_to_close(p)?;
        self.captures[level].finish(self.pattern, s, p)?;

        Ok(next_match(self, s, p)?.or_else(|| {
            self.captures[level].revert();
            None
        }))
    }

    /// Returns the index of the highest pending capture group still needing
    /// finalising.
    fn capture_to_close(&self, p: usize) -> Result<usize> {
        for level in (0..self.level).rev() {
            if matches!(self.captures[level], CaptureState::Pending { .. }) {
                return Ok(level);
            }
        }
        Err(Error::InvalidPatternCapture {
            pos: self.char_pos(p),
        })
    }

    /// Ensures the given capture index belongs to a finished capture group and
    /// returns its range if so.
    fn check_capture(&self, p: usize, level: B::Primitive) -> Result<&Range<usize>> {
        // Clippy: The range of the number is 0-9.
        #[allow(clippy::cast_possible_truncation)]
        let (level, oops) = (level.to_digit().unwrap() as u8).overflowing_sub(1);
        let index = usize::from(level);
        if !oops
            && index < self.level
            && let CaptureState::Finished(CaptureRange::Range(range)) = &self.captures[index]
        {
            Ok(range)
        } else {
            Err(Error::InvalidCaptureIndex {
                index: usize::from(level.wrapping_add(1)),
                pos: self.char_pos(p),
            })
        }
    }

    /// Finds the end of a character set. Returns the next position of the
    /// pattern, or an error if the pattern ends before the set is closed.
    /// `p` points to `[` for a set, `%` for a class, or a plain character.
    fn class_end(&self, mut p: usize) -> Result<usize> {
        let mut i = self.pattern[p..].chars().peekable();
        let c = i.next().unwrap();
        p += c.len_utf8();
        Ok(match c.as_ascii() {
            b'%' => {
                let Some(class) = i.next() else {
                    return Err(Error::EndsWithPercent {
                        pos: self.char_pos(p),
                    });
                };
                p + class.len_utf8()
            }
            b'[' => {
                if i.next_if_eq(&B::Primitive::from_ascii(b'^')).is_some() {
                    p += 1;
                }

                // This loop seems to be written in a convoluted way so it does
                // not break when the first character in the set is ']'
                loop {
                    let Some(c) = i.next() else {
                        return Err(Error::EndsWithoutBracket {
                            pos: self.char_pos(p),
                        });
                    };

                    p += c.len_utf8();

                    if c == B::Primitive::from_ascii(b'%')
                        && let Some(c) = i.next()
                    {
                        p += c.len_utf8();
                    }

                    if i.peek() == Some(&B::Primitive::from_ascii(b']')) {
                        break;
                    }
                }

                p + 1
            }
            _ => p,
        })
    }

    /// Checks whether the next input character matches the pattern item at the
    /// given range. Returns the length of the matched character.
    fn is_single_match(&self, s: usize, p_start: usize, p_end: usize) -> Option<usize> {
        let c = self.input[s..].chars().next()?;
        let mut i = self.pattern[p_start..].chars();
        let lit = i.next().unwrap();
        let is_match = match lit.as_ascii() {
            b'.' => true,
            b'%' => match_class(c, i.next().unwrap()),
            b'[' => self.is_in_set(c, p_start, p_end - 1),
            _ => lit == c,
        };
        is_match.then_some(c.len_utf8())
    }

    /// Checks whether the given input character matches the character set
    /// at the given range. `p` points to `[` and `p_end` points to `]`.
    fn is_in_set(&self, c: B::Primitive, p: usize, p_end: usize) -> bool {
        let mut matched = true;
        let mut i = self.pattern[p + 1..p_end].chars().peekable();

        if i.next_if_eq(&B::Primitive::from_ascii(b'^')).is_some() {
            matched = false;
        }

        while let Some(s) = i.next() {
            if s == B::Primitive::from_ascii(b'%') {
                // %w
                if let Some(cl) = i.next()
                    && match_class(c, cl)
                {
                    return matched;
                }
            } else if let Some(r) = i.next_if_eq(&B::Primitive::from_ascii(b'-')) {
                if let Some(e) = i.next() {
                    // [a-z]
                    if s <= c && c <= e {
                        return matched;
                    }
                } else if s == c || r == c {
                    // Literal character with terminating -
                    return matched;
                }
            } else if s == c {
                // Literal character
                return matched;
            }
        }

        !matched
    }
}

/// Intermediate state representation of a capture group.
// Clippy: Internal fields are self-documenting.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Clone)]
enum CaptureState {
    /// The capture group is waiting to be closed.
    Pending { start: usize },
    /// The capture group is fully created.
    Finished(CaptureRange),
}

impl CaptureState {
    /// Finalise a ranged capture group.
    fn finish<B: BackingType + ?Sized>(&mut self, pattern: &B, end: usize, p: usize) -> Result<()> {
        match self {
            CaptureState::Pending { start } => {
                *self = CaptureState::Finished(CaptureRange::Range(*start..end));
                Ok(())
            }
            CaptureState::Finished(..) => Err(Error::InvalidPatternCapture {
                pos: char_pos(pattern, p),
            }),
        }
    }

    /// Roll back a ranged capture group to a pending state.
    fn revert(&mut self) {
        if let CaptureState::Finished(CaptureRange::Range(range)) = self {
            *self = CaptureState::Pending { start: range.start }
        }
    }
}

impl Default for CaptureState {
    fn default() -> Self {
        Self::Finished(<_>::default())
    }
}

impl<B> TryFrom<(&B, CaptureState)> for CaptureRange
where
    B: BackingType + ?Sized,
{
    type Error = Error;

    fn try_from((pattern, value): (&B, CaptureState)) -> Result<Self, Self::Error> {
        match value {
            CaptureState::Pending { start } => Err(Error::UnfinishedCapture {
                pos: char_pos(pattern, start),
            }),
            CaptureState::Finished(capture_range) => Ok(capture_range),
        }
    }
}

/// The maximum amount of recursion allowed through [`next_match`] during
/// pattern matching.
const MAX_RECURSION_DEPTH: usize = 500;

/// Returns true if the character `c` is in the range of classes corresponding
/// to `class`.
///
/// This Unicode features here follow rule specified by the MediaWiki
/// implementation of this feature, not anything specified in PUC-Lua.
// TODO: Probably these should check ASCII first and then fall back to the
// Unicode tables since most of the time they are going to be matching ASCII
// anyway and that would improve cacheline efficiency probably?
fn match_class<C: PrimitiveType>(c: C, class: C) -> bool {
    let matches = match class.as_ascii().to_ascii_lowercase() {
        b'a' => c.is_alphabetic(),
        b'c' => c.is_control(),
        b'd' => c.is_decimal(),
        b'g' => c.is_graphic(),
        b'l' => c.is_lowercase(),
        b'p' => c.is_punctuation(),
        b's' => c.is_whitespace(),
        b'u' => c.is_uppercase(),
        b'w' => c.is_alphanumeric(),
        b'x' => c.is_hexdigit(),
        b'z' => c.is_null(),
        _ => return c == class,
    };
    if class.as_ascii().is_ascii_lowercase() {
        matches
    } else {
        !matches
    }
}

/// Returns the 1-indexed character count at the given byte position.
#[inline]
fn char_pos<B: BackingType + ?Sized>(pattern: &B, offset: usize) -> usize {
    pattern.char_count(offset) + 1
}

/// A character primitive type.
pub trait PrimitiveType: Copy + Eq + Ord {
    /// The primitive type as an ASCII character.
    fn as_ascii(self) -> u8;
    /// Creates a new [`PrimitiveType`] from an ASCII byte.
    ///
    /// # Panics
    ///
    /// Panics if the byte is not in the range of valid ASCII characters.
    fn from_ascii(b: u8) -> Self;
    /// Whether the primitive is an alphabetic character.
    fn is_alphabetic(self) -> bool;
    /// Whether the primitive is alphanumeric character.
    fn is_alphanumeric(self) -> bool;
    /// Whether the primitive is a control character.
    fn is_control(self) -> bool;
    /// Whether the primitive is a decimal digit.
    fn is_decimal(self) -> bool;
    /// Whether the primitive is a graphic character.
    fn is_graphic(self) -> bool;
    /// Whether the primitive is a hexadecimal digit.
    fn is_hexdigit(self) -> bool;
    /// Whether the primitive is a lowercase alphabetical character.
    fn is_lowercase(self) -> bool;
    /// Whether the primitive is a null character.
    fn is_null(self) -> bool;
    /// Whether the primitive is a punctuation character.
    fn is_punctuation(self) -> bool;
    /// Whether the primitive is a whitespace character.
    fn is_whitespace(self) -> bool;
    /// Whether the primitive is an uppercase alphabetical character.
    fn is_uppercase(self) -> bool;
    /// The length of the value encoded as UTF-8.
    fn len_utf8(self) -> usize;
    /// Converts the value from an ASCII digit to a number.
    fn to_digit(self) -> Option<u32>;
}

impl PrimitiveType for u8 {
    #[inline]
    fn as_ascii(self) -> u8 {
        self
    }
    #[inline]
    fn from_ascii(b: u8) -> Self {
        b
    }
    #[inline]
    fn is_alphabetic(self) -> bool {
        self.is_ascii_alphabetic()
    }
    #[inline]
    fn is_alphanumeric(self) -> bool {
        self.is_ascii_alphanumeric()
    }
    #[inline]
    fn is_control(self) -> bool {
        self.is_ascii_control()
    }
    #[inline]
    fn is_decimal(self) -> bool {
        self.is_ascii_digit()
    }
    #[inline]
    fn is_graphic(self) -> bool {
        self.is_ascii_graphic()
    }
    #[inline]
    fn is_hexdigit(self) -> bool {
        self.is_ascii_hexdigit()
    }
    #[inline]
    fn is_lowercase(self) -> bool {
        self.is_ascii_lowercase()
    }
    #[inline]
    fn is_null(self) -> bool {
        self == b'\0'
    }
    #[inline]
    fn is_punctuation(self) -> bool {
        self.is_ascii_punctuation()
    }
    #[inline]
    fn is_whitespace(self) -> bool {
        self.is_ascii_whitespace()
    }
    #[inline]
    fn is_uppercase(self) -> bool {
        self.is_ascii_uppercase()
    }
    #[inline]
    fn len_utf8(self) -> usize {
        1
    }
    #[inline]
    fn to_digit(self) -> Option<u32> {
        Some((self - b'0').into())
    }
}

impl PrimitiveType for char {
    #[inline]
    fn as_ascii(self) -> u8 {
        let mut b = [0; 4];
        char::encode_utf8(self, &mut b);
        b[0]
    }
    #[inline]
    fn from_ascii(b: u8) -> Self {
        char::from_u32(b.into()).unwrap()
    }
    #[inline]
    fn is_alphabetic(self) -> bool {
        // Clippy: Verbosity is not a useful thing here.
        #[allow(clippy::enum_glob_use)]
        use GeneralCategory::*;
        matches!(
            get_general_category(self),
            LowercaseLetter | ModifierLetter | OtherLetter | TitlecaseLetter | UppercaseLetter
        )
    }
    #[inline]
    fn is_alphanumeric(self) -> bool {
        self.is_alphabetic() || self.is_decimal()
    }
    #[inline]
    fn is_control(self) -> bool {
        get_general_category(self) == GeneralCategory::Control
    }
    #[inline]
    fn is_decimal(self) -> bool {
        get_general_category(self) == GeneralCategory::DecimalNumber
    }
    #[inline]
    fn is_graphic(self) -> bool {
        self.is_ascii_graphic()
    }
    #[inline]
    fn is_hexdigit(self) -> bool {
        matches!(self, '0'..='9' | 'A'..='F' | 'a'..='f' | '０'..='９' | 'Ａ'..='Ｆ' | 'ａ'..='ｆ')
    }
    #[inline]
    fn is_lowercase(self) -> bool {
        get_general_category(self) == GeneralCategory::LowercaseLetter
    }
    #[inline]
    fn is_null(self) -> bool {
        self == '\0'
    }
    #[inline]
    fn is_punctuation(self) -> bool {
        // Clippy: Verbosity is not a useful thing here.
        #[allow(clippy::enum_glob_use)]
        use GeneralCategory::*;
        matches!(
            get_general_category(self),
            ClosePunctuation
                | ConnectorPunctuation
                | DashPunctuation
                | FinalPunctuation
                | InitialPunctuation
                | OpenPunctuation
                | OtherPunctuation
        )
    }
    #[inline]
    fn is_whitespace(self) -> bool {
        char::is_whitespace(self)
    }
    #[inline]
    fn is_uppercase(self) -> bool {
        get_general_category(self) == GeneralCategory::UppercaseLetter
    }
    #[inline]
    fn len_utf8(self) -> usize {
        char::len_utf8(self)
    }
    #[inline]
    fn to_digit(self) -> Option<u32> {
        char::to_digit(self, 10)
    }
}

/// A type which can be searched by the pattern matching engine.
pub trait BackingType:
    core::ops::Index<Range<usize>, Output = Self>
    + core::ops::Index<core::ops::RangeFrom<usize>, Output = Self>
    + core::ops::Index<core::ops::RangeTo<usize>, Output = Self>
    + ToOwned
{
    /// The underlying primitive type.
    type Primitive: PrimitiveType;
    /// Converts from a piccolo value to the backing type.
    fn from_value<'gc>(ctx: Context<'gc>, value: Value<'gc>) -> Result<&'gc Self, VmError<'gc>>;
    /// Gets the underlying byte view.
    fn as_bytes(&self) -> &[u8];
    /// Returns an iterator over the primitives of the backing type.
    fn chars(&self) -> impl DoubleEndedIterator<Item = Self::Primitive>;
    /// Returns the character count at the given byte index.
    fn char_count(&self, index: usize) -> usize;
    /// Returns an iterator over the byte indexes and primitives of the backing
    /// type.
    fn char_indices(&self) -> impl DoubleEndedIterator<Item = (usize, Self::Primitive)>;
    /// Returns the index of the first occurrence of the given pattern.
    fn find(&self, pattern: &Self) -> Option<usize>;
    /// Returns a slice of the backing type.
    fn get<I>(&self, index: I) -> Option<&Self>
    where
        I: SliceIndex<Self, Output = Self>;
    /// Whether the given index lies on a primitive boundary.
    fn is_char_boundary(&self, index: usize) -> bool;
    /// Whether the backing type is empty.
    fn is_empty(&self) -> bool;
    /// The byte length of the backing type.
    fn len(&self) -> usize;
    /// Whether the backing type starts with the given primitive.
    fn starts_with(&self, pattern: Self::Primitive) -> bool;
    /// Returns an iterator over the byte indexes and primitives of the backing
    /// type starting at the given *character* index.
    fn start_scan(&self, index: usize) -> impl Iterator<Item = (usize, Self::Primitive)>;
    /// Converts from the backing type to a piccolo value.
    fn to_value<'gc>(&self, ctx: Context<'gc>) -> Value<'gc>;
}

impl BackingType for str {
    type Primitive = char;

    #[inline]
    fn from_value<'gc>(ctx: Context<'gc>, value: Value<'gc>) -> Result<&'gc Self, VmError<'gc>> {
        if let Some(s) = value.into_string(ctx) {
            Ok(s.to_str()?)
        } else {
            Err(TypeError {
                expected: "string",
                found: value.type_name(),
            }
            .into())
        }
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
    #[inline]
    fn chars(&self) -> impl DoubleEndedIterator<Item = Self::Primitive> {
        self.chars()
    }
    #[inline]
    fn char_count(&self, end: usize) -> usize {
        self[..end].chars().count()
    }
    #[inline]
    fn char_indices(&self) -> impl DoubleEndedIterator<Item = (usize, Self::Primitive)> {
        self.char_indices()
    }
    #[inline]
    fn find(&self, pattern: &Self) -> Option<usize> {
        memchr::memmem::find(self.as_bytes(), pattern.as_bytes())
    }
    #[inline]
    fn get<I>(&self, index: I) -> Option<&Self>
    where
        I: SliceIndex<Self, Output = Self>,
    {
        self.get(index)
    }
    #[inline]
    fn is_char_boundary(&self, index: usize) -> bool {
        self.is_char_boundary(index)
    }
    #[inline]
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
    #[inline]
    fn len(&self) -> usize {
        self.len()
    }
    #[inline]
    fn start_scan(&self, index: usize) -> impl Iterator<Item = (usize, Self::Primitive)> {
        self.char_indices()
            .skip_while(move |(pos, _)| *pos < index)
            .chain(core::iter::once((self.len(), '\0')))
    }
    #[inline]
    fn starts_with(&self, pattern: Self::Primitive) -> bool {
        self.starts_with(pattern)
    }
    #[inline]
    fn to_value<'gc>(&self, ctx: Context<'gc>) -> Value<'gc> {
        Value::String(ctx.intern(self.as_bytes()))
    }
}

impl BackingType for [u8] {
    type Primitive = u8;

    #[inline]
    fn from_value<'gc>(ctx: Context<'gc>, value: Value<'gc>) -> Result<&'gc Self, VmError<'gc>> {
        if let Some(s) = value.into_string(ctx) {
            Ok(s.as_bytes())
        } else {
            Err(TypeError {
                expected: "string",
                found: value.type_name(),
            }
            .into())
        }
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self
    }
    #[inline]
    fn chars(&self) -> impl DoubleEndedIterator<Item = Self::Primitive> {
        self.iter().copied()
    }
    #[inline]
    fn char_count(&self, end: usize) -> usize {
        end
    }
    #[inline]
    fn char_indices(&self) -> impl DoubleEndedIterator<Item = (usize, Self::Primitive)> {
        self.iter().copied().enumerate()
    }
    #[inline]
    fn find(&self, pattern: &Self) -> Option<usize> {
        memchr::memmem::find(self, pattern)
    }
    #[inline]
    fn get<I>(&self, index: I) -> Option<&Self>
    where
        I: SliceIndex<Self, Output = Self>,
    {
        self.get(index)
    }
    #[inline]
    fn is_char_boundary(&self, _: usize) -> bool {
        true
    }
    #[inline]
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
    #[inline]
    fn len(&self) -> usize {
        self.len()
    }
    #[inline]
    fn start_scan(&self, index: usize) -> impl Iterator<Item = (usize, Self::Primitive)> {
        self.char_indices()
            .skip(index)
            .chain(core::iter::once((self.len(), b'\0')))
    }
    #[inline]
    fn starts_with(&self, pattern: Self::Primitive) -> bool {
        self.starts_with(&[pattern])
    }
    #[inline]
    fn to_value<'gc>(&self, ctx: Context<'gc>) -> Value<'gc> {
        Value::String(ctx.intern(self.as_bytes()))
    }
}
