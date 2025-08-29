//! MediaWiki Scribunto Lua Unicode string support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::lua::stdlib::{
    calculate_start_count, find_lua, gmatch_next, gsub::gsub_lua, match_lua, sub_lua,
};
use piccolo::Stack;
use unicode_normalization::UnicodeNormalization as _;

#[cfg(test)]
mod tests;

/// The maximum length, in bytes, allowed for pattern strings.
const PATTERN_LENGTH_LIMIT: usize = 10000;
/// The maximum length, in bytes, allowed for input strings.
const STRING_LENGTH_LIMIT: usize = 2048 * 1024;

/// The Unicode-aware Lua string support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct UstringLibrary;

impl UstringLibrary {
    /// Returns the byte offset of the character which is `char_at` 1-indexed
    /// characters after the 1-indexed byte index `byte_at`. If `byte_at` is
    /// negative, counts from the end of the string.
    fn byte_offset<'gc>(
        &self,
        ctx: Context<'gc>,
        (s, char_at, byte_at): (VmString<'gc>, Option<i64>, Option<i64>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        check_string("byteoffset", ctx, s)?;
        let mut char_at = char_at.unwrap_or(1);
        let mut byte_at = byte_at.unwrap_or(1);
        let s = s.to_str()?;

        let len = i64::try_from(s.len())?;
        if byte_at < 0 {
            byte_at += len + 1;
        }

        if byte_at < 1 || byte_at > len {
            return Ok(Value::Nil);
        }

        byte_at -= 1;
        let initial = byte_at;
        // Rust guarantees `true` at 0, so this will never walk too far
        while !s.is_char_boundary(byte_at.try_into()?) {
            byte_at -= 1;
        }
        if char_at > 0 && initial == byte_at {
            char_at -= 1;
        }

        let chars_before = s[..usize::try_from(byte_at)?].chars().count();

        let char_at = i64::try_from(chars_before)? + char_at;
        if char_at < 0 {
            return Ok(Value::Nil);
        }

        s.char_indices()
            .nth(usize::try_from(char_at)?)
            .map_or(
                Ok(Value::Nil),
                |(pos, _)| Ok(i64::try_from(pos + 1)?.into()),
            )
    }

    /// Creates a utf-8 string from a sequence of Unicode code points.
    fn r#char<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        if stack.len() > STRING_LENGTH_LIMIT {
            return Err("too many arguments to 'char'".into_value(ctx))?;
        }

        let value = stack
            .drain(..)
            .enumerate()
            .map(|(k, v)| {
                // MW accepts flooring floats, but this seems too sus to be a good
                // idea
                let v = v.to_integer().ok_or_else(|| {
                    anyhow::anyhow!("bad argument #{} to 'char' (integer expected)", k + 1)
                })?;
                let v = u32::try_from(v).map_err(RuntimeError::new)?;
                char::try_from(v).map_err(RuntimeError::new)
            })
            .collect::<Result<String, _>>()?;

        if value.len() > STRING_LENGTH_LIMIT {
            return Err("result too long for 'char'".into_value(ctx))?;
        }

        stack.replace(ctx, ctx.intern(value.as_bytes()));
        Ok(CallbackReturn::Return)
    }

    /// Returns a list of code points between 1-indexed `start` and `end` as a
    /// list.
    fn code_point<'gc>(
        &self,
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        let (s, start, end) = stack.consume::<(VmString<'_>, Option<i64>, Option<i64>)>(ctx)?;
        check_string("codepoint", ctx, s)?;

        for c in code_point(s, start, end)? {
            stack.push_back(c);
        }

        Ok(CallbackReturn::Return)
    }

    /// Finds a pattern in a string, optionally starting from an index. Returns
    /// the 1-indexed start and end positions of the match, plus any captured
    /// strings.
    fn find<'gc>(
        &self,
        ctx: Context<'gc>,
        stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        // TODO: Change the abstraction slightly so these checks can happen
        // without redundant string conversions
        if let Some(s) = stack.get(0).into_string(ctx) {
            check_string("find", ctx, s)?;
        }
        if let Some(pattern) = stack.get(1).into_string(ctx) {
            check_pattern("find", ctx, pattern)?;
        }
        find_lua::<str>(ctx, stack)
    }

    /// Returns a list of code points between 1-indexed `start` and `end` as a
    /// sequence.
    fn gcodepoint_init<'gc>(
        &self,
        ctx: Context<'gc>,
        (s, start, end): (VmString<'_>, Option<i64>, Option<i64>),
    ) -> Result<Table<'gc>, VmError<'gc>> {
        check_string("gcodepoint_init", ctx, s)?;

        let seq = Table::new(&ctx);
        // Clippy: The index will never be large enough to wrap.
        #[allow(clippy::cast_possible_wrap)]
        for (index, value) in code_point(s, start, end)?.enumerate() {
            seq.set(ctx, index as i64 + 1, value)?;
        }
        Ok(seq)
    }

    /// Matches a pattern in a string starting from an index.
    /// Returns the position of the match and the matched captures, or the whole
    /// match if no capture groups were specified.
    fn gmatch_callback<'gc>(
        &self,
        ctx: Context<'gc>,
        (s, pattern, _, at): (VmString<'gc>, VmString<'gc>, Value<'gc>, i64),
    ) -> Result<(i64, Table<'gc>), VmError<'gc>> {
        // The Lua side of this function does not conform to Lua standards and
        // instead uses a 0-index
        let at = usize::try_from(at)?;
        let (at, result) = gmatch_next::<str>(ctx, s, pattern, at)?;
        Ok((at.try_into()?, result))
    }

    /// The initialiser for a gmatch pseudo-iterator.
    fn gmatch_init<'gc>(
        &self,
        ctx: Context<'gc>,
        (s, pattern): (VmString<'gc>, VmString<'gc>),
    ) -> Result<(VmString<'gc>, Value<'gc>), VmError<'gc>> {
        check_string("gmatch", ctx, s)?;
        check_pattern("gmatch", ctx, pattern)?;
        Ok((pattern, Value::Nil))
    }

    /// Finds and replaces matching patterns within a string.
    fn gsub<'gc>(
        &self,
        ctx: Context<'gc>,
        stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        // TODO: Change the abstraction slightly so these checks can happen
        // without redundant string conversions
        if let Some(s) = stack.get(0).into_string(ctx) {
            check_string("gsub", ctx, s)?;
        }
        if let Some(pattern) = stack.get(1).into_string(ctx) {
            check_pattern("gsub", ctx, pattern)?;
        }
        gsub_lua::<str>(ctx, stack)
    }

    /// Returns true if the input string is valid utf-8.
    fn is_utf8<'gc>(&self, ctx: Context<'gc>, s: VmString<'_>) -> Result<bool, VmError<'gc>> {
        check_string("isutf8", ctx, s)?;
        Ok(s.to_str().is_ok())
    }

    /// Returns the length of the input string in Unicode characters.
    fn len<'gc>(&self, ctx: Context<'gc>, s: VmString<'_>) -> Result<Value<'gc>, VmError<'gc>> {
        check_string("len", ctx, s)?;
        Ok(s.to_str().map_or(Ok(Value::Nil), |s| {
            i64::try_from(s.chars().count()).map(Value::Integer)
        })?)
    }

    /// Converts a string to lowercase.
    fn lower<'gc>(
        &self,
        ctx: Context<'gc>,
        s: VmString<'_>,
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        check_string("lower", ctx, s)?;
        Ok(ctx.intern(s.to_str()?.to_lowercase().as_bytes()))
    }

    /// Matches a pattern in a string, optionally starting from an index.
    /// Returns the matched captures, or the whole match if no capture groups
    /// were specified.
    fn r#match<'gc>(
        &self,
        ctx: Context<'gc>,
        stack: Stack<'gc, '_>,
    ) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
        // TODO: Change the abstraction slightly so these checks can happen
        // without redundant string conversions
        if let Some(s) = stack.get(0).into_string(ctx) {
            check_string("match", ctx, s)?;
        }
        match_lua::<str>(ctx, stack)
    }

    /// Returns a slice of the given string.
    fn sub<'gc>(
        &self,
        ctx: Context<'gc>,
        args: (VmString<'gc>, Option<i64>, Option<i64>),
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        check_string("sub", ctx, args.0)?;
        sub_lua::<'gc, str>(ctx, args)
    }

    /// Converts the input string to NFC form.
    fn to_nfc<'gc>(&self, ctx: Context<'gc>, s: VmString<'gc>) -> Result<Value<'gc>, VmError<'gc>> {
        check_string("toNFC", ctx, s)?;
        Ok(s.to_str().map_or(Value::Nil, |s| {
            ctx.intern(s.nfc().collect::<String>().as_bytes()).into()
        }))
    }

    /// Converts the input string to NFD form.
    fn to_nfd<'gc>(&self, ctx: Context<'gc>, s: VmString<'gc>) -> Result<Value<'gc>, VmError<'gc>> {
        check_string("toNFD", ctx, s)?;
        Ok(s.to_str().map_or(Value::Nil, |s| {
            ctx.intern(s.nfd().collect::<String>().as_bytes()).into()
        }))
    }

    /// Converts the input string to NFKC form.
    fn to_nfkc<'gc>(
        &self,
        ctx: Context<'gc>,
        s: VmString<'gc>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        check_string("toNFKC", ctx, s)?;
        Ok(s.to_str().map_or(Value::Nil, |s| {
            ctx.intern(s.nfkc().collect::<String>().as_bytes()).into()
        }))
    }

    /// Converts the input string to NFKD form.
    fn to_nfkd<'gc>(
        &self,
        ctx: Context<'gc>,
        s: VmString<'gc>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        check_string("toNFKD", ctx, s)?;
        Ok(s.to_str().map_or(Value::Nil, |s| {
            ctx.intern(s.nfkd().collect::<String>().as_bytes()).into()
        }))
    }

    /// Converts a string to uppercase.
    fn upper<'gc>(
        &self,
        ctx: Context<'gc>,
        s: VmString<'_>,
    ) -> Result<VmString<'gc>, VmError<'gc>> {
        check_string("upper", ctx, s)?;
        Ok(ctx.intern(s.to_str()?.to_uppercase().as_bytes()))
    }
}

impl MwInterface for UstringLibrary {
    const NAME: &str = "mw.ustring";
    const CODE: &[u8] = include_bytes!("./modules/mw.ustring.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            byteoffset = byte_offset,
            ~ r#char = r#char,
            ~ codepoint = code_point,
            ~ find = find,
            gcodepoint_init = gcodepoint_init,
            gmatch_callback = gmatch_callback,
            gmatch_init = gmatch_init,
            ~ gsub = gsub,
            isutf8 = is_utf8,
            len = len,
            lower = lower,
            ~ r#match = r#match,
            sub = sub,
            upper = upper,
            toNFC = to_nfc,
            toNFD = to_nfd,
            toNFKC = to_nfkc,
            toNFKD = to_nfkd,
        }
    }

    // Clippy: The value is constant and known to be in range.
    #[allow(clippy::cast_possible_wrap)]
    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(table! {
            using ctx;

            patternLengthLimit = PATTERN_LENGTH_LIMIT as i64,
            stringLengthLimit = STRING_LENGTH_LIMIT as i64,
        })
    }
}

/// Validates that the given pattern is below the size limit.
#[inline]
fn check_pattern<'gc>(func: &str, ctx: Context<'gc>, s: VmString<'_>) -> Result<(), VmError<'gc>> {
    // Clippy: The value is constant and known to be in range.
    #[allow(clippy::cast_possible_wrap)]
    if s.len() > PATTERN_LENGTH_LIMIT as i64 {
        Err(format!(
            "bad argument #2 to '{func}' (pattern is longer than {PATTERN_LENGTH_LIMIT} bytes)"
        )
        .into_value(ctx))?
    } else {
        Ok(())
    }
}

/// Validates that the given string is below the size limit.
#[inline]
fn check_string<'gc>(func: &str, ctx: Context<'gc>, s: VmString<'_>) -> Result<(), VmError<'gc>> {
    // Clippy: The value is constant and known to be in range.
    #[allow(clippy::cast_possible_wrap)]
    if s.len() > STRING_LENGTH_LIMIT as i64 {
        Err(format!(
            "bad argument #1 to '{func}' (string is longer than {STRING_LENGTH_LIMIT} bytes)"
        )
        .into_value(ctx))?
    } else {
        Ok(())
    }
}

/// Creates an iterator of Unicode scalar value codepoints in the string `s`
/// between the 1-based `start` and `end` indices.
fn code_point<'gc>(
    s: VmString<'_>,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<impl Iterator<Item = Value<'gc>>, VmError<'gc>> {
    let start = start.unwrap_or(1);
    let end = end.unwrap_or(start);
    let s = s.to_str()?;

    let (start, count) = calculate_start_count(s, start, end)?.unwrap_or((0, 0));

    Ok(s.chars()
        .skip(start)
        .take(count)
        .map(|c| Value::Integer(u32::from(c).into())))
}
