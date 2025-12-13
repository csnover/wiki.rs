//! Lua 5.1-compatible `string.format` implementation.

use crate::lua::prelude::*;
use core::fmt::Write as _;
use gc_arena::Gc;
pub(super) use lua::{ConversionSpecifier, ConversionType};
use lua::{FormatElement, parse_format_string};
use piccolo::Function;

/// Formats a string from values taken off the stack.
pub(super) fn format_impl<'gc>(
    ctx: Context<'gc>,
    _: Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, VmError<'gc>> {
    let fmt = stack.from_front::<VmString<'_>>(ctx)?.to_str()?;
    let mut args = stack.into_iter();

    let mut result = String::new();
    for part in parse_format_string(fmt) {
        match part? {
            FormatElement::Verbatim(part) => result.push_str(part),
            FormatElement::Format(spec) => match spec.conversion_type {
                ConversionType::Pointer => {
                    let value = match args.next() {
                        None
                        | Some(
                            Value::Nil | Value::Boolean(_) | Value::Integer(_) | Value::Number(_),
                        ) => None,
                        Some(Value::Function(Function::Closure(c))) => {
                            Some(Gc::as_ptr(c.into_inner()) as usize)
                        }
                        Some(Value::Function(Function::Callback(c))) => {
                            Some(Gc::as_ptr(c.into_inner()) as usize)
                        }
                        Some(Value::String(s)) => Some(Gc::as_ptr(s.into_inner()) as usize),
                        Some(Value::Table(t)) => Some(Gc::as_ptr(t.into_inner()) as usize),
                        Some(Value::Thread(t)) => Some(Gc::as_ptr(t.into_inner()) as usize),
                        Some(Value::UserData(d)) => Some(Gc::as_ptr(d.into_inner()) as usize),
                    };

                    if let Some(value) = value {
                        // Clippy: The spec will reconvert it back to usize.
                        // C-style APIs: wooo!!
                        #[allow(clippy::cast_possible_wrap)]
                        spec.write_i64(&mut result, value as i64)?;
                    } else {
                        result += "(null)";
                    }
                }
                ConversionType::Char
                | ConversionType::DecInt
                | ConversionType::OctInt
                | ConversionType::HexIntLower
                | ConversionType::HexIntUpper
                | ConversionType::DecUint => {
                    let value = args
                        .next()
                        .and_then(|value| {
                            // Lua 5.2 was less strict and would implicitly
                            // convert any float to int, not just whole numbers,
                            // and 'Module:Weather box/colors' relies on this
                            #[allow(clippy::cast_possible_truncation)]
                            match value {
                                Value::Integer(n) => Some(n),
                                Value::Number(n) => Some(n as i64),
                                Value::String(_) => value.to_integer(),
                                _ => None,
                            }
                        })
                        .ok_or_else(|| "not enough arguments".into_value(ctx))?;
                    spec.write_i64(&mut result, value)?;
                }
                ConversionType::SciFloatLower
                | ConversionType::SciFloatUpper
                | ConversionType::DecFloatLower
                | ConversionType::DecFloatUpper
                | ConversionType::CompactFloatLower
                | ConversionType::CompactFloatUpper
                | ConversionType::HexFloatLower
                | ConversionType::HexFloatUpper => {
                    let value = args
                        .next()
                        .and_then(Value::to_number)
                        .ok_or_else(|| "not enough arguments".into_value(ctx))?;
                    spec.write_f64(&mut result, value)?;
                }
                ConversionType::String => {
                    let value = args
                        .next()
                        .and_then(|value| value.into_string(ctx))
                        .map(VmString::to_str)
                        .ok_or_else(|| "not enough arguments".into_value(ctx))??;
                    spec.write_str(&mut result, value)?;
                }
                ConversionType::Serialize => {
                    match args.next() {
                        Some(Value::Boolean(value)) => write!(result, "{value:?}")?,
                        Some(Value::Integer(value)) => write!(result, "{value}")?,
                        // TODO: Technically this emits hexadecimal float format
                        Some(Value::Number(value)) => spec.write_f64(&mut result, value)?,
                        Some(Value::String(value)) => write!(result, r#""{}""#, value.to_str()?)?,
                        Some(Value::Nil) => write!(result, "nil")?,
                        Some(_) => Err("value has no literal form".into_value(ctx))?,
                        None => Err("not enough arguments".into_value(ctx))?,
                    }
                }
            },
        }
    }
    stack.replace(ctx, result);
    Ok(CallbackReturn::Return)
}

// SPDX-SnippetBegin
// SPDX-License-Identifier: MIT
// SPDX-SnippetComment: Adapted from sprintf-rs, PUC-Lua, Spargue/printf, hexfloat2
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
mod lua {
    //! Support functions for `string.format`.

    use core::fmt;

    /// A format string parsing error.
    #[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
    pub(crate) enum PrintfError {
        /// Parsing failed. Womp-womp.
        #[error("Error parsing the format string")]
        ParseError,
    }

    type Result<T, E = PrintfError> = core::result::Result<T, E>;

    /// A part of a format string: either a string to be emitted verbatim, or a
    /// format specifier to be replaced by the next positional argument.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum FormatElement<'a> {
        /// Characters which are copied to the output as-is.
        Verbatim(&'a str),
        /// A format specifier.
        Format(ConversionSpecifier),
    }

    /// A `printf`-style conversion specifier.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[allow(clippy::struct_excessive_bools)]
    pub(crate) struct ConversionSpecifier {
        /// Flag `#`: use `0x`, etc.
        pub alt_form: bool,
        /// Flag `0`: left-pad with zeros.
        pub zero_pad: bool,
        /// Flag `-`: left-adjust (pad with spaces on the right).
        pub left_adj: bool,
        /// Flag `' '` (space): indicate sign with a space.
        pub space_sign: bool,
        /// Flag `+`: Always show the sign (for signed numbers).
        pub force_sign: bool,
        /// Field width.
        pub width: Option<u8>,
        /// Floating point field precision.
        pub precision: Option<u8>,
        /// The conversion type.
        pub conversion_type: ConversionType,
    }

    impl ConversionSpecifier {
        /// Returns the integer base for the specifier.
        fn base(&self) -> u8 {
            match self.conversion_type {
                ConversionType::HexFloatLower
                | ConversionType::HexFloatUpper
                | ConversionType::HexIntLower
                | ConversionType::HexIntUpper => 16,
                ConversionType::OctInt => 8,
                _ => 10,
            }
        }

        /// Returns true if the specifier should emit uppercase characters.
        fn is_upper(&self) -> bool {
            matches!(
                self.conversion_type,
                ConversionType::HexFloatUpper
                    | ConversionType::SciFloatUpper
                    | ConversionType::DecFloatUpper
                    | ConversionType::CompactFloatUpper
                    | ConversionType::HexIntUpper
            )
        }

        /// Writes `value` to `f` as a formatted string according to the
        /// properties of this specifier.
        pub fn write_f64(&self, f: &mut dyn fmt::Write, value: f64) -> fmt::Result {
            let mut buf = [0u8; 256];
            let mut len = 0;

            let (start, zero_pad) = if value.is_nan() {
                let value = if self.is_upper() { b"NAN" } else { b"nan" };
                buf[0..value.len()].copy_from_slice(value);
                len += value.len();
                (0, false)
            } else if value.is_infinite() {
                let value = if self.is_upper() { b"FNI" } else { b"fni" };
                buf[0..value.len()].copy_from_slice(value);
                len += value.len();
                (0, false)
            } else {
                let (use_sci, use_simple) = match self.conversion_type {
                    ConversionType::Serialize
                    | ConversionType::HexFloatLower
                    | ConversionType::HexFloatUpper => {
                        return self.write_f64_hex(f, value);
                    }
                    ConversionType::DecFloatLower | ConversionType::DecFloatUpper => (false, false),
                    ConversionType::SciFloatLower | ConversionType::SciFloatUpper => (true, false),
                    ConversionType::CompactFloatLower | ConversionType::CompactFloatUpper => {
                        (true, true)
                    }
                    _ => unreachable!(),
                };

                let trim = if use_sci {
                    self.fill_f64_exp(&mut buf, &mut len, value, use_simple)
                } else {
                    fill_f64_dec(&mut buf, &mut len, value, self.width, self.precision);
                    false
                };

                (
                    if trim {
                        buf[..len]
                            .iter()
                            .position(|c| *c != b'0' && *c != b'.')
                            .unwrap_or(len)
                    } else {
                        0
                    },
                    true,
                )
            };

            // The extra adjustment argument is necessary to compensate for
            // truncation from the simple mode
            if zero_pad {
                let has_sign =
                    usize::from(value.is_sign_negative() || self.force_sign || self.space_sign);
                self.fill_zeros(&mut buf, &mut len, start.saturating_sub(has_sign));
            }

            if value.is_sign_negative() {
                buf[len] = b'-';
                len += 1;
            } else if self.force_sign {
                buf[len] = b'+';
                len += 1;
            } else if self.space_sign {
                buf[len] = b' ';
                len += 1;
            }

            self.write_buf(f, &buf[start..len], zero_pad)?;

            Ok(())
        }

        /// Fills the backwards buffer with `value` written as an exponential
        /// number. If `use_simple` is true, the value is written using the C
        /// “simple” format instead, according to the rules in the spec.
        fn fill_f64_exp(
            &self,
            buf: &mut [u8],
            len: &mut usize,
            value: f64,
            use_simple: bool,
        ) -> bool {
            let mut abs = value.abs();
            let mut exp = abs.log10().floor().max(0.0) as i32;
            let mut exp_width = Some(if exp < 100 && exp > -100 { 4 } else { 5 });
            let precision = self.precision.unwrap_or(6);
            let precision = if use_simple {
                let prec = if precision == 0 {
                    1
                } else {
                    i32::from(precision)
                };

                // exponent signifies significant digits - we must round now
                // to (re)calculate the exponent
                let factor = 10.0_f64.powf(f64::from(prec - 1 - exp));
                let fixed = (abs * factor).round();
                abs = fixed / factor;
                exp = abs.log10().floor().max(0.0) as i32;

                u8::try_from(if prec > exp && exp >= -4 {
                    exp_width = None;
                    exp = 0;
                    prec - exp - 1
                } else {
                    prec - 1
                })
                .unwrap()
            } else {
                precision
            };

            if precision > 0 {
                let mut normal = abs / 10.0_f64.powf(f64::from(exp));
                let mut int_part = normal.trunc();
                let mut exp_factor = 10.0_f64.powf(f64::from(precision));
                let mut tail = ((normal - int_part) * exp_factor).round() as u64;
                while tail >= exp_factor as u64 {
                    // Overflow, must round
                    int_part += 1.0;
                    tail -= exp_factor as u64;
                    if int_part >= 10.0 {
                        // keep same precision - which means changing exponent
                        exp += 1;
                        exp_factor /= 10.0;
                        normal /= 10.0;
                        int_part = normal.trunc();
                        tail = ((normal - int_part) * exp_factor).round() as u64;
                    }
                }
            }

            let fract_width = if self.left_adj {
                None
            } else if let (Some(width), Some(exp_width)) = (self.width, exp_width)
                && width > exp_width
            {
                Some(width - exp_width)
            } else {
                self.width
            };

            if let Some(exp_width) = exp_width {
                fill_u64_part(buf, len, exp as u64, 10, b'a', Some(exp_width - 2), None);
                buf[*len] = if exp.is_negative() { b'-' } else { b'+' };
                *len += 1;
                buf[*len] = if self.is_upper() { b'E' } else { b'e' };
                *len += 1;
            }

            fill_f64_dec(
                buf,
                len,
                value / 10.0_f64.powf(f64::from(exp)),
                fract_width,
                Some(precision),
            );

            exp_width.is_none() && !self.alt_form
        }

        /// Writes `value` to `f` as a formatted hexadecimal string according to
        /// the properties of this specifier.
        fn write_f64_hex(&self, f: &mut dyn fmt::Write, value: f64) -> fmt::Result {
            const MANTISSA_BITS: u8 = 52;
            const EXPONENT_BIAS: u16 = 1023;

            let (sign, exponent, mantissa) = {
                let bits = value.to_bits();
                let sign = (bits >> 63) == 1;
                let exponent = ((bits >> 52) & 0x7FF) as u16;
                let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;
                (sign, exponent, mantissa)
            };

            let bias = i32::from(EXPONENT_BIAS);
            // The mantissa MSB needs to be shifted up to the nearest nibble.
            let mshift = (4 - u32::from(MANTISSA_BITS) % 4) % 4;
            let mantissa = mantissa << mshift;
            // The width is rounded up to the nearest char (4 bits)
            // TODO: This use of precision is almost certainly wrong since libc
            // rounds the last digit (including the leading digit)
            let mwidth = self
                .precision
                .map_or((MANTISSA_BITS as usize).div_ceil(4), usize::from);
            let sign_char = if sign {
                "-"
            } else if self.force_sign {
                "+"
            } else if self.space_sign {
                " "
            } else {
                ""
            };
            let mut exponent = i32::from(exponent) - bias;
            let leading = if exponent == -bias {
                // subnormal number means we shift our output by 1 bit.
                exponent += 1;
                "0."
            } else {
                "1."
            };

            write!(f, "{sign_char}0x{leading}{mantissa:0mwidth$x}p{exponent}")
        }

        /// Writes `value` to `f` as a formatted string according to the
        /// properties of this specifier.
        pub fn write_i64(&self, f: &mut dyn fmt::Write, value: i64) -> fmt::Result {
            let mut buf = [0_u8; 256];

            if self.conversion_type == ConversionType::Char {
                let value = char::from_u32(value as u32).ok_or(fmt::Error)?;
                return self.write_str(f, value.encode_utf8(&mut buf));
            }

            let base = self.base();
            let (alt_form, hex_base) = if self.is_upper() {
                (b'X', b'A')
            } else {
                (b'x', b'a')
            };

            let mut len = 0;
            {
                let value = if self.conversion_type == ConversionType::DecInt {
                    value.abs()
                } else {
                    value
                } as u64;
                fill_u64_part(
                    &mut buf,
                    &mut len,
                    value,
                    base,
                    hex_base,
                    self.precision,
                    None,
                );
            }

            if self.conversion_type == ConversionType::DecInt {
                self.fill_zeros(&mut buf, &mut len, 0);
            }

            // It is not possible for force_sign or space_sign to be set on
            // specs which do not support them because the parser will have
            // rejected those flags
            if value.is_negative() && base == 10 {
                buf[len] = b'-';
                len += 1;
            } else if self.force_sign {
                buf[len] = b'+';
                len += 1;
            } else if self.space_sign {
                buf[len] = b' ';
                len += 1;
            }

            if self.alt_form {
                buf[len] = alt_form;
                buf[len + 1] = b'0';
                len += 2;
            }

            self.write_buf(f, &buf[..len], self.zero_pad)?;

            Ok(())
        }

        /// Writes the backwards buffer into `f` with padding according to the
        /// properties of this specifier.
        fn write_buf(&self, f: &mut dyn fmt::Write, buf: &[u8], zero_pad: bool) -> fmt::Result {
            if !self.left_adj {
                self.write_pad(f, buf.len(), zero_pad)?;
            }
            for c in buf.iter().rev() {
                // Safety: It is all ASCII characters that we just put there.
                f.write_char(unsafe { char::from_u32_unchecked(u32::from(*c)) })?;
            }
            if self.left_adj {
                self.write_pad(f, buf.len(), zero_pad)?;
            }
            Ok(())
        }

        /// Writes the literal string `value` into `f`.
        pub fn write_str(&self, f: &mut dyn fmt::Write, value: &str) -> fmt::Result {
            let len = self
                .precision
                .map_or(value.len(), usize::from)
                .min(value.len());

            if !self.left_adj {
                self.write_pad(f, len, self.zero_pad)?;
            }

            f.write_str(&value[..len])?;

            if self.left_adj {
                self.write_pad(f, len, self.zero_pad)?;
            }

            Ok(())
        }

        /// Writes padding of length `len` into `f`.
        fn write_pad(&self, f: &mut dyn fmt::Write, len: usize, zero_pad: bool) -> fmt::Result {
            if let Some(width) = self.width
                && let width = usize::from(width)
                && len < width
            {
                let pad = if zero_pad {
                    "0000000000000000"
                } else {
                    "                "
                };
                let mut amount = width - len;
                while amount > pad.len() {
                    write!(f, "{pad}")?;
                    amount -= pad.len();
                }
                write!(f, "{}", &pad[..amount])?;
            }
            Ok(())
        }

        /// Fills the backwards buffer with zeros according to the properties of
        /// this specifier.
        fn fill_zeros(&self, buf: &mut [u8], len: &mut usize, adjust: usize) {
            if !self.left_adj
                && self.zero_pad
                && let Some(width) = self.width.map(usize::from)
            {
                while *len < width + adjust {
                    buf[*len] = b'0';
                    *len += 1;
                }
            }
        }
    }

    /// Returns true if `value` is ~exactly 0.5.
    #[inline]
    #[allow(clippy::double_comparisons)]
    fn is_half(value: f64) -> bool {
        !(value < 0.5 || value > 0.5)
    }

    /// Fills the backwards buffer with `value` formatted as a decimal number.
    fn fill_f64_dec(
        buf: &mut [u8],
        len: &mut usize,
        value: f64,
        min_width: Option<u8>,
        precision: Option<u8>,
    ) {
        let whole = value.trunc();
        let factor = 10.0_f64.powf(f64::from(precision.unwrap_or(6)));
        let frac = (value.fract() * factor).abs();
        let diff = frac.fract();

        let mut whole = whole.abs() as u64;
        let mut frac = frac as u64;

        if diff > 0.5 {
            frac += 1;
            if frac >= factor as u64 {
                frac = 0;
                whole += 1;
            }
        } else if is_half(diff) && (frac == 0 || (frac & 1) != 0) {
            frac += 1;
        }

        let start = *len;
        if precision == Some(0) {
            let diff = value - value.trunc();
            if is_half(diff) && (whole & 1) != 0 {
                whole += 1;
            }
        } else {
            fill_u64_part(buf, len, frac, 10, b'a', precision, None);
            buf[*len] = b'.';
            *len += 1;
        }

        fill_u64_part(buf, len, whole, 10, b'a', None, None);
        if let Some(min_width) = min_width {
            for _ in (*len - start)..usize::from(min_width) {
                buf[*len] = b'0';
                *len += 1;
            }
        }
    }

    /// Fills the backwards buffer with `value` formatted as an unsigned
    /// integer using the given base, precision, and width.
    fn fill_u64_part(
        buf: &mut [u8],
        len: &mut usize,
        mut value: u64,
        base: u8,
        hex_base_char: u8,
        precision: Option<u8>,
        max_width: Option<u8>,
    ) {
        let start = *len;

        let max_width = max_width
            .map_or(buf.len(), |width| start + usize::from(width))
            .min(buf.len());

        while *len < max_width {
            let digit = (value % u64::from(base)) as u8;
            buf[*len] = if digit < 10 {
                b'0' + digit
            } else {
                hex_base_char + digit - 10
            };
            *len += 1;
            value /= u64::from(base);
            if value == 0 {
                break;
            }
        }

        if let Some(precision) = precision {
            let precision = usize::from(precision);
            while (*len - start) < precision {
                buf[*len] = b'0';
                *len += 1;
            }
        }
    }

    /// Printf data type
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum ConversionType {
        /// `a`
        HexFloatLower,
        /// `A`
        HexFloatUpper,
        /// `c`
        Char,
        /// `d`, `i`
        DecInt,
        /// `e`
        SciFloatLower,
        /// `E`
        SciFloatUpper,
        /// `f`
        DecFloatLower,
        /// `F`
        DecFloatUpper,
        /// `g`
        CompactFloatLower,
        /// `G`
        CompactFloatUpper,
        /// `o`
        OctInt,
        /// `p`
        Pointer,
        /// `q`
        Serialize,
        /// `s`
        String,
        /// `u`
        DecUint,
        /// `x`
        HexIntLower,
        /// `X`
        HexIntUpper,
    }

    /// Converts a formatting string into a sequence of [`FormatElement`]s.
    pub(crate) struct FormatIter<'a>(&'a str);

    impl<'a> Iterator for FormatIter<'a> {
        type Item = Result<FormatElement<'a>>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.0.is_empty() {
                return None;
            }

            Some(if let Some(spec) = self.0.find('%') {
                if spec == 0 {
                    if self.0.as_bytes()[1] == b'%' {
                        let lit = &self.0[..1];
                        self.0 = &self.0[2..];
                        Ok(FormatElement::Verbatim(lit))
                    } else {
                        match take_conversion_specifier(&self.0[1..]) {
                            Ok((spec, rest)) => {
                                self.0 = rest;
                                Ok(FormatElement::Format(spec))
                            }
                            Err(err) => Err(err),
                        }
                    }
                } else {
                    let verbatim_prefix = &self.0[..spec];
                    self.0 = &self.0[spec..];
                    Ok(FormatElement::Verbatim(verbatim_prefix))
                }
            } else {
                let rem = self.0;
                self.0 = &self.0[self.0.len()..];
                Ok(FormatElement::Verbatim(rem))
            })
        }
    }

    /// Parses a formatting string into a sequence of [`FormatElement`]s.
    pub fn parse_format_string(fmt: &str) -> impl Iterator<Item = Result<FormatElement<'_>>> {
        FormatIter(fmt)
    }

    /// Parses a single [`ConversionSpecifier`] from the given string.
    ///
    /// Returns the specifier and any remaining unparsed part of the string.
    ///
    /// # Errors
    ///
    /// If the given string does not contain a valid specifier.
    fn take_conversion_specifier(s: &str) -> Result<(ConversionSpecifier, &str)> {
        let mut spec = ConversionSpecifier {
            alt_form: false,
            zero_pad: false,
            left_adj: false,
            space_sign: false,
            force_sign: false,
            width: None,
            precision: None,
            conversion_type: ConversionType::DecInt,
        };

        let mut s = s;

        // parse flags
        loop {
            match s.bytes().next() {
                Some(b'#') => {
                    spec.alt_form = true;
                }
                Some(b'0') => {
                    spec.zero_pad = true;
                }
                Some(b'-') => {
                    spec.left_adj = true;
                }
                Some(b' ') => {
                    spec.space_sign = true;
                }
                Some(b'+') => {
                    spec.force_sign = true;
                }
                _ => {
                    break;
                }
            }
            s = &s[1..];
        }

        // parse width
        let (w, mut s) = take_numeric_param(s);
        spec.width = w;
        // parse precision
        if matches!(s.bytes().next(), Some(b'.')) {
            s = &s[1..];
            let (p, s2) = take_numeric_param(s);
            spec.precision = p;
            s = s2;
        }

        // parse conversion type
        spec.conversion_type = match s.bytes().next() {
            Some(b'a') => ConversionType::HexFloatLower,
            Some(b'A') => ConversionType::HexFloatUpper,
            Some(b'c') => ConversionType::Char,
            Some(b'd' | b'i') => ConversionType::DecInt,
            Some(b'e') => ConversionType::SciFloatLower,
            Some(b'E') => ConversionType::SciFloatUpper,
            Some(b'f') => ConversionType::DecFloatLower,
            Some(b'F') => ConversionType::DecFloatUpper,
            Some(b'g') => ConversionType::CompactFloatLower,
            Some(b'G') => ConversionType::CompactFloatUpper,
            Some(b'o') => ConversionType::OctInt,
            Some(b'p') => ConversionType::Pointer,
            Some(b'q') => ConversionType::Serialize,
            Some(b's') => ConversionType::String,
            Some(b'u') => ConversionType::DecUint,
            Some(b'x') => ConversionType::HexIntLower,
            Some(b'X') => ConversionType::HexIntUpper,
            _ => {
                return Err(PrintfError::ParseError);
            }
        };

        let invalid_specifiers = match spec.conversion_type {
            ConversionType::Char | ConversionType::String | ConversionType::Pointer => {
                spec.alt_form || spec.zero_pad || spec.space_sign || spec.force_sign
            }
            ConversionType::DecInt => spec.alt_form,
            ConversionType::DecUint => spec.alt_form || spec.force_sign || spec.space_sign,
            ConversionType::OctInt | ConversionType::HexIntLower | ConversionType::HexIntUpper => {
                spec.space_sign || spec.force_sign
            }
            ConversionType::Serialize => {
                spec.alt_form
                    || spec.zero_pad
                    || spec.left_adj
                    || spec.space_sign
                    || spec.force_sign
            }
            _ => false,
        };

        if spec.conversion_type == ConversionType::Pointer {
            spec.alt_form = true;
        }

        if invalid_specifiers {
            Err(PrintfError::ParseError)
        } else {
            Ok((spec, &s[1..]))
        }
    }

    /// Parses the number part of a conversion specifier.
    ///
    /// Returns the number, if any, and any remaining unparsed part of the
    /// string.
    fn take_numeric_param(mut s: &str) -> (Option<u8>, &str) {
        // The PUC-Lua implementation is more restrictive than the C standard.
        // “Both width and precision, when present, are limited to two digits”
        let mut w = None;
        let mut c = s.bytes();
        if let Some(digit) = c.next()
            && digit.is_ascii_digit()
        {
            s = &s[1..];
            w = Some(digit - b'0');
            if let Some(digit) = c.next()
                && digit.is_ascii_digit()
            {
                w = w.map(|w| 10 * w + (digit - b'0'));
                s = &s[1..];
            }
        }
        (w, s)
    }
}
// SPDX-SnippetEnd

#[cfg(test)]
mod tests {
    #[track_caller]
    fn check_fmt_i64(fmt: &str, value: i64) -> String {
        let mut out = String::new();

        for fmt in super::lua::parse_format_string(fmt) {
            match fmt.unwrap() {
                super::lua::FormatElement::Verbatim(s) => out += s,
                super::lua::FormatElement::Format(spec) => {
                    spec.write_i64(&mut out, value).unwrap();
                }
            }
        }
        out
    }

    #[track_caller]
    fn check_fmt_f64(fmt: &str, value: f64) -> String {
        let mut out = String::new();
        for fmt in super::lua::parse_format_string(fmt) {
            match fmt.unwrap() {
                super::lua::FormatElement::Verbatim(s) => out += s,
                super::lua::FormatElement::Format(spec) => {
                    spec.write_f64(&mut out, value).unwrap();
                }
            }
        }
        out
    }

    #[track_caller]
    fn check_fmt_str(fmt: &str, value: &str) -> String {
        let mut out = String::new();
        for fmt in super::lua::parse_format_string(fmt) {
            match fmt.unwrap() {
                super::lua::FormatElement::Verbatim(s) => out += s,
                super::lua::FormatElement::Format(spec) => {
                    spec.write_str(&mut out, value).unwrap();
                }
            }
        }
        out
    }

    #[test]
    fn test_int() {
        assert_eq!(check_fmt_i64("%d", 12), "12");
        assert_eq!(check_fmt_i64("%06d", -12), "-000012");
        assert_eq!(check_fmt_i64("~%d~", 148), "~148~");
        assert_eq!(check_fmt_i64("00%dxx", -91232), "00-91232xx");
        assert_eq!(check_fmt_i64("%x", -9232), "ffffffffffffdbf0");
        assert_eq!(check_fmt_i64("%X", 432), "1B0");
        assert_eq!(check_fmt_i64("%09X", 432), "0000001B0");
        assert_eq!(check_fmt_i64("%9X", 432), "      1B0");
        // assert_eq!(check_fmt_i64("%+9X", 492));
        assert_eq!(check_fmt_i64("%#9x", 4589), "   0x11ed");
        assert_eq!(check_fmt_i64("%#9X", 4589), "   0X11ED");
        assert_eq!(check_fmt_i64("%2o", 4), " 4");
        assert_eq!(check_fmt_i64("%o", 10), "12");
        assert_eq!(check_fmt_i64("% 12d", -4), "          -4");
        assert_eq!(check_fmt_i64("% 12d", 48), "          48");
        assert_eq!(check_fmt_i64("%X", -4), "FFFFFFFFFFFFFFFC");
        assert_eq!(check_fmt_i64("%-8d", -12), "-12     ");
        assert_eq!(
            check_fmt_i64("%x", 0x123_4567_89ab_cdef_i64),
            "123456789abcdef"
        );
        assert_eq!(check_fmt_i64("test char %c", b'~'.into()), "test char ~");
        assert_eq!(
            check_fmt_i64("%099d", 1),
            "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001"
        );
        assert_eq!(check_fmt_i64("%4c", b'A'.into()), "   A");
        assert_eq!(check_fmt_i64("%-4cX", b'A'.into()), "A   X");
    }

    #[test]
    fn test_float() {
        assert_eq!(check_fmt_f64("%f", -46.38), "-46.380000");
        assert_eq!(check_fmt_f64("%012.3f", 1.2), "00000001.200");
        assert_eq!(check_fmt_f64("%0012.3f", 1.2), "00000001.200");
        assert_eq!(check_fmt_f64("%012.3e", 1.7), "0001.700e+00");
        assert_eq!(check_fmt_f64("%e", 1e300), "1.000000e+300");
        assert_eq!(check_fmt_f64("%012.3g%%!", 2.6), "0000000002.6%!");
        assert_eq!(check_fmt_f64("%012.5G", -2.69), "-00000002.69");
        assert_eq!(check_fmt_f64("%+7.4f", 42.785), "+42.7850");
        assert_eq!(check_fmt_f64("{}% 7.4E", 493.12), "{} 4.9312E+02");
        assert_eq!(check_fmt_f64("% 7.4E", -120.3), "-1.2030E+02");
        assert_eq!(check_fmt_f64("%-10F", f64::INFINITY), "INF       ");
        assert_eq!(check_fmt_f64("%+010f", f64::INFINITY), "      +inf");
        assert_eq!(check_fmt_f64("%.0f", 9.99), "10");
        assert_eq!(check_fmt_f64("%.1f", 999.99), "1000.0");
        assert_eq!(check_fmt_f64("%.1f", 9.99), "10.0");
        assert_eq!(check_fmt_f64("%.1e", 9.99), "1.0e+01");
        assert_eq!(check_fmt_f64("%.2f", 9.99), "9.99");
        assert_eq!(check_fmt_f64("%.2e", 9.99), "9.99e+00");
        assert_eq!(check_fmt_f64("%.3f", 9.99), "9.990");
        assert_eq!(check_fmt_f64("%.3e", 9.99), "9.990e+00");
        assert_eq!(check_fmt_f64("%.1g", 9.99), "1e+01");
        assert_eq!(check_fmt_f64("%.1G", 9.99), "1E+01");
        assert_eq!(check_fmt_f64("%.1f", 2.99), "3.0");
        assert_eq!(check_fmt_f64("%.1e", 2.99), "3.0e+00");
        assert_eq!(check_fmt_f64("%.1g", 2.99), "3");
        assert_eq!(check_fmt_f64("%.1f", 2.599), "2.6");
        assert_eq!(check_fmt_f64("%.1e", 2.599), "2.6e+00");
        assert_eq!(check_fmt_f64("%.1g", 2.599), "3");
        assert_eq!(check_fmt_f64("% f", f64::NAN), " nan");
        assert_eq!(check_fmt_f64("%+f", f64::NAN), "+nan");
        assert_eq!(check_fmt_f64("%.6g", 1.0), "1");
        assert_eq!(check_fmt_f64("%.6g", -1.0), "-1");
    }

    #[test]
    fn test_str() {
        assert_eq!(
            check_fmt_str("test %% with string: %s yay\n", "FOO"),
            "test % with string: FOO yay\n"
        );
        assert_eq!(check_fmt_str("%4s", "A"), "   A");
        check_fmt_str("%4s", "𒀀"); // multi-byte character test (4 bytes)
        assert_eq!(check_fmt_str("%-4sX", "A"), "A   X");
        check_fmt_str("%-4sX", "𒀀"); // multi-byte character test (4 bytes)
        assert_eq!(check_fmt_str("%1.3s", "ABCDEFG"), "ABC");
        check_fmt_str("%1.4s", "𒀀𒀀"); // multi-byte character test (4 bytes per char)
        assert_eq!(check_fmt_str("%8.4s", "ABCDEFG"), "    ABCD");

        // glibc does not handle UTF-8 strings correctly when truncating, but we cannot produce malformed UTF-8
        // strings in Rust. Instead, we round down to the nearest character boundary.
        // assert_eq!(sprintf!("%1.1s", "𒀀𒀀𒀀").unwrap(), " ");
        // assert_eq!(sprintf!("%1.2s", "𒀀𒀀𒀀").unwrap(), " ");
        // assert_eq!(sprintf!("%1.3s", "𒀀𒀀𒀀").unwrap(), " ");
        // assert_eq!(sprintf!("%1.4s", "𒀀𒀀𒀀").unwrap(), "𒀀");
        // assert_eq!(sprintf!("%1.5s", "𒀀𒀀𒀀").unwrap(), "𒀀");
        // assert_eq!(sprintf!("%1.6s", "𒀀𒀀𒀀").unwrap(), "𒀀");
        // assert_eq!(sprintf!("%1.7s", "𒀀𒀀𒀀").unwrap(), "𒀀");
        // assert_eq!(sprintf!("%1.8s", "𒀀𒀀𒀀").unwrap(), "𒀀𒀀");
    }
}
