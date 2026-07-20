//! A [`Sink`] for deduplicating, filtering, and sanitising HTML tag attributes.

use super::{Sink, chainable};
use crate::StripMarker;
use core::fmt::Write as _;
use indexmap::IndexMap;
use libwikitext_common::{AnchorEncodeMode, decode_html, escape_id};
use regex::Regex;
use std::{borrow::Cow, collections::HashSet, sync::LazyLock};

/// Deduplicates, filters, and sanitises invalid HTML attributes.
#[derive(Debug)]
pub(crate) struct AttributeFilter<S: Sink> {
    /// The attribute accumulator.
    acc: IndexMap<Cow<'static, str>, String>,
    /// The list of allowed attribute names for the currently processing tag.
    allowed: &'static [&'static phf::Set<&'static str>],
    /// The output.
    next: S,
    /// The current state of the filter.
    state: State,
}

chainable!(AttributeFilter);

impl<S: Sink> AttributeFilter<S> {
    /// Creates a new `AttributeFilter` chained to `next`.
    pub fn new(next: S) -> Self {
        Self {
            acc: <_>::default(),
            allowed: <_>::default(),
            next,
            state: <_>::default(),
        }
    }

    /// Fixes up a list of IDs.
    fn fixup_aria(next: &mut S, ids: &str) {
        let mut first = true;
        for id in ids.split_ascii_whitespace() {
            if first {
                first = false;
            } else {
                next.text(" ");
            }
            next.text(&escape_id(id, AnchorEncodeMode::Html5));
        }
    }

    /// Fixes up a `class` attribute.
    fn fixup_class(next: &mut S, classes: &str) {
        let mut seen = HashSet::new();
        for class in classes.split_ascii_whitespace() {
            if seen.insert(class) {
                if seen.len() != 1 {
                    next.text(" ");
                }
                next.text(class);
            }
        }
    }

    /// Fixes up a `style` attribute.
    fn fixup_style(next: &mut S, style: &str) {
        static RE_UHOH: LazyLock<Regex> = LazyLock::new(|| {
            const UHOH: &str = concat!(
                "expression",
                r"|accelerator\s*:",
                r"|-o-(?:link(?:-source)?|replace)\s*:",
                r"|(?:url|src|image|image-set)\s*\(",
                r"|attr\s*\([^)]+[\s,]+url"
            );
            Regex::new(UHOH).unwrap()
        });

        let style = decode_html(style);
        let Some(style) = decode_css(&style) else {
            next.text("/* invalid control char */");
            return;
        };

        if RE_UHOH.is_match(&style) {
            next.text("/* insecure input */");
            return;
        }

        let mut style = style.trim_ascii();
        while !style.is_empty() {
            // 'Template:Table cell templates' contains a bunch of invalid
            // garbage. When this happens, just try skipping to the next
            // possibly valid declaration.
            if let Ok((decl, rest)) = barely_css::decl(style) {
                style = &style[rest..];
                if let Some((name, value)) = decl {
                    if name.starts_with("--") {
                        next.text(name);
                        next.text(":");
                        next.text(value);
                        next.text(";");
                    } else {
                        next.text("--mw-output-");
                        next.text(name);
                        next.text(":");
                        next.text(value);
                        next.text(";");
                    }
                }
            } else if let Some(rest) = style.find(';') {
                next.text(&style[..=rest]);
                style = &style[rest + 1..];
            } else {
                next.text(style);
                break;
            }
        }
    }

    /// Returns true if the given attribute `name` is allowed.
    fn is_allowed(&self, name: &str) -> Option<Cow<'static, str>> {
        if let Some(name) = self
            .allowed
            .iter()
            .find_map(|allowed| allowed.get_key(name))
        {
            Some(Cow::Borrowed(*name))
        } else {
            // Technically, these are supposed to be only allowed for things
            // which allow the common attributes, but HTML5 does not care, and
            // maybe our own data attributes want to go somewhere unexpected, so
            // it does not matter
            if let Some(suffix) = name.strip_prefix("data-") {
                (!suffix.starts_with("mw")
                    && !suffix.starts_with("ooui")
                    && !suffix.starts_with("parsoid")
                    && !suffix.contains([':', '=', ' ', '\t', '\r', '\n', '/', '>', '\0']))
                .then(|| name.to_owned().into())
            } else if let Some(suffix) = name.strip_prefix("xmlns:") {
                (!suffix.is_empty()).then(|| name.to_owned().into())
            } else {
                None
            }
        }
    }
}

impl<S: Sink> Sink for AttributeFilter<S> {
    #[inline]
    fn comment_end(&mut self) {
        if self.state == State::Idle {
            self.next.comment_end();
        }
    }

    #[inline]
    fn comment_start(&mut self) {
        if self.state == State::Idle {
            self.next.comment_start();
        }
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        match self.state {
            State::Idle => self.next.entity(value, raw),
            State::Buffering(index) => self.acc[index].push(value),
            State::Filtering => {}
        }
    }

    #[inline]
    fn finish(self) -> String {
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.state == State::Idle {
            self.next.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if self.state == State::Idle {
            self.next.strip_marker(marker);
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, _: &str) {
        self.state = State::Idle;
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if let Some(name) = self.is_allowed(name) {
            let entry = self.acc.entry(name);
            let index = entry.index();
            entry.or_default().clear();
            self.state = State::Buffering(index);
        } else {
            self.state = State::Filtering;
        }
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.allowed = ALLOWED_ATTRS.get(name).copied().unwrap_or_default();
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.allowed = <_>::default();

        let has_itemscope = self.acc.contains_key("itemscope");
        for (name, value) in self.acc.drain(..) {
            if name == "tabindex" && value != "0" {
                continue;
            }
            // TODO: Figure out how to get a config in here without blowing up
            // borrowck
            // if matches!(name.as_ref(), "href" | "poster" | "src")
            //     && !self.config.protocols_pattern.is_match(&value) {
            //     continue;
            // }
            if name == "id" && value.is_empty() {
                continue;
            }
            if !has_itemscope && matches!(name.as_ref(), "itemtype" | "itemid" | "itemref") {
                continue;
            }

            self.next.tag_attribute_start(&name);
            match name.as_ref() {
                "aria-describedby" | "aria-flowto" | "aria-labelledby" | "aria-owns" => {
                    Self::fixup_aria(&mut self.next, &value);
                }
                "class" => Self::fixup_class(&mut self.next, &value),
                "id" => self.next.text(&escape_id(&value, AnchorEncodeMode::Html5)),
                "style" => Self::fixup_style(&mut self.next, &value),
                _ => self.next.text(&value),
            }
            self.next.tag_attribute_end(&name);
        }

        self.next.tag_start_end(name);
    }

    #[inline]
    fn text(&mut self, text: &str) {
        match self.state {
            State::Idle => self.next.text(text),
            State::Buffering(index) => self.acc[index] += text,
            State::Filtering => {}
        }
    }
}

/// The [`AttributeFilter`] state.
#[derive(Debug, Default, Eq, PartialEq)]
enum State {
    /// Inactive.
    #[default]
    Idle,
    /// Buffering an attribute at the given index in [`AttributeFilter::acc`].
    Buffering(usize),
    /// Filtering an attribute.
    Filtering,
}

/// Decodes CSS escape sequences according to the MediaWiki rules, returning
/// `None` if `style` contains any “problematic control characters”.
fn decode_css(style: &str) -> Option<Cow<'_, str>> {
    #[inline]
    fn bad_char(c: char) -> bool {
        matches!(c, '\0'..='\x08' | '\x0b' | '\x0e'..='\x1f' | '\x7f' | char::REPLACEMENT_CHARACTER)
    }

    let bytes = style.as_bytes();
    let mut out = String::new();
    let mut cursor = 0;
    let mut flushed = 0;
    let mut seen_non_ws = false;

    // Any slash at the end of a CSS string is a literal slash requiring no
    // processing
    let max = style.len().saturating_sub(1);

    // This has to be a range check because a multi-byte character at the end of
    // the string will cause `cursor` to advance to `style.len()`
    while cursor < max {
        // Technically, comment handling is supposed to be a separate pass
        // *after* decoding, but if someone relied on unescaping CSS to get
        // their CSS comment to look like a CSS comment, ya dun goofed
        if bytes[cursor] == b'/' && bytes[cursor + 1] == b'*' {
            let start = cursor;
            cursor += 2;
            let unclosed = loop {
                if cursor == max {
                    break true;
                }
                if bytes[cursor] == b'*' && bytes[cursor + 1] == b'/' {
                    cursor += 2;
                    break false;
                }
                cursor += 1;
            };

            if unclosed {
                return Some(if flushed == 0 {
                    Cow::Borrowed(&style[..start])
                } else {
                    out += &style[flushed..start];
                    Cow::Owned(out)
                });
            }

            if !seen_non_ws && style[cursor..].bytes().all(|b| b.is_ascii_whitespace()) {
                // This comment is the only thing in the whole style other than
                // some whitespace, so just return it all
                return (style[start..cursor].chars().all(|c| !bad_char(c)))
                    .then_some(Cow::Borrowed(style));
            }

            // …and there is other stuff, so replace the whole comment with
            // whitespace and continue
            seen_non_ws = true;
            out += &style[flushed..start];
            out.push(' ');
            flushed = cursor;
            continue;
        }

        if bytes[cursor] != b'\\' {
            let c = style[cursor..].chars().next()?;
            if bad_char(c) {
                return None;
            }
            seen_non_ws |= !c.is_ascii_whitespace();
            cursor += c.len_utf8();
            continue;
        }

        seen_non_ws = true;
        out += &style[flushed..cursor];
        cursor += 1;

        // It is safe to unconditionally access `bytes[cursor]` here because
        // `max` means there will always be at least one more byte to check
        if bytes[cursor] == b'\r' && bytes.get(cursor + 1) == Some(&b'\n') {
            // Line continuation
            cursor += 2;
        } else if matches!(bytes[cursor], b'\r' | b'\n' | b'\x0c') {
            // Line continuation
            cursor += 1;
        } else if bytes[cursor].is_ascii_hexdigit() {
            // Unicode escape sequence
            let start = cursor;
            for _ in 0..6 {
                cursor += 1;
                if bytes.get(cursor).is_none_or(|b| !b.is_ascii_hexdigit()) {
                    break;
                }
            }

            let c = u32::from_str_radix(&style[start..cursor], 16).unwrap();
            let c = char::from_u32(c)?;
            if bad_char(c) {
                return None;
            } else if matches!(c, '\n' | '"' | '\'' | '\\') {
                write!(out, "\\{:x} ", c as u32).unwrap();
            } else {
                out.push(c);
            }

            if matches!(
                bytes.get(cursor),
                Some(b'\t' | b'\r' | b'\n' | b'\x0c' | b' ')
            ) {
                cursor += 1;
            }
        } else if matches!(bytes[cursor], b'"' | b'\'' | b'\\') {
            // Escape of a character considered special by MediaWiki
            write!(out, "\\{:x} ", bytes[cursor]).unwrap();
            cursor += 1;
        } else {
            // A non-escape escape
        }

        flushed = cursor;
    }

    for c in style[cursor..].chars() {
        if bad_char(c) {
            return None;
        }
    }

    Some(if flushed == 0 {
        Cow::Borrowed(style)
    } else {
        out += &style[flushed..];
        Cow::Owned(out)
    })
}

/// The allowed list of attributes for the given tags.
static ALLOWED_ATTRS: phf::Map<&str, &[&phf::Set<&str>]> = phf::phf_map! {
    "a" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "href", "rel", "rev" }
    ],
    "audio" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "controls", "height", "preload", "width" }
    ],
    "abbr" | "aside" | "b" | "bdi" | "bdo" | "big" | "center" | "cite" | "code"
    | "dd" | "dfn" | "dl" | "dt" | "em" | "figcaption" | "figure" | "i" | "kbd"
    | "mark" | "rb" | "rp" | "rt" | "rtc" | "ruby" | "s" | "samp" | "small"
    | "span" | "strike" | "strong" | "sub" | "sup" | "tbody" | "tfoot"
    | "thead" | "tt" | "u" | "var" | "wbr"
    => &[
        &COMMON_ATTRS
    ],
    "blockquote" | "q" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "cite" }
    ],
    "br" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "clear" }
    ],
    "caption" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "align" }
    ],
    "col" | "colgroup" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "span" }
    ],
    "data" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "value" }
    ],
    "font" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "color", "face", "size" }
    ],
    "hr" | "pre" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "width" }
    ],
    "img" => &[
        &COMMON_ATTRS,
        // For some reason, decoding is not in the Wikitext list, but it is
        // allowed
        &phf::phf_set! { "alt", "decoding", "height", "src", "srcset", "width" }
    ],
    "ins" | "del" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "cite", "datetime" }
    ],
    "li" => &[
        &COMMON_ATTRS, &phf::phf_set! { "type", "value" }
    ],
    "link" => &[
        &phf::phf_set! { "href", "itemprop", "title" }
    ],
    "math" => &[
        &phf::phf_set! { "class", "id", "style", "title" }
    ],
    "meta" => &[
        &phf::phf_set! { "content", "itemprop" }
    ],
    "ol" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "reversed", "start", "type" }
    ],
    "ul" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "type" }
    ],
    "source" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "src", "type" }
    ],
    "table" => &[
        &COMMON_ATTRS,
        &phf::phf_set! {
            "align", "bgcolor", "border", "cellpadding", "cellspacing", "frame",
            "rules", "summary", "width"
        }
    ],
    "td" | "th" => &[
        &COMMON_ATTRS,
        &phf::phf_set! {
            "abbr", "align", "axis", "bgcolor", "colspan", "headers", "height",
            "nowrap", "rowspan", "scope", "valign", "width"
        }
    ],
    "tr" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "align", "bgcolor", "valign" }
    ],
    "time" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "datetime" }
    ],
    "track" => &[
        &COMMON_ATTRS,
        &phf::phf_set! { "kind", "label", "src", "srclang", "type" }
    ],
    "video" => &[
        &COMMON_ATTRS,
        // MediaWiki does not allow muted and loop, but there is no reason not
        // to, and timed media emits HTML with these attributes
        &phf::phf_set! { "controls", "height", "muted", "loop", "poster", "preload", "width" }
    ],
};

/// Common attributes allowed on most tags.
static COMMON_ATTRS: phf::Set<&str> = phf::phf_set! {
    "about",
    "aria-describedby",
    "aria-flowto",
    "aria-hidden",
    "aria-label",
    "aria-labelledby",
    "aria-level",
    "aria-owns",
    "class",
    "datatype",
    "dir",
    "id",
    "itemid",
    "itemprop",
    "itemref",
    "itemscope",
    "itemtype",
    "lang",
    "property",
    "resource",
    "role",
    "style",
    "tabindex",
    "title",
    "typeof",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_css() {
        assert_eq!(decode_css("\\\"").as_deref(), Some("\\22 "));
        assert_eq!(decode_css("\\22 ").as_deref(), Some("\\22 "));
        assert_eq!(decode_css("\\💩").as_deref(), Some("💩"));
        assert_eq!(decode_css("a\\\nb").as_deref(), Some("ab"));
        // CSS is a unique snowflake
        assert_eq!(decode_css("a\\nb").as_deref(), Some("anb"));
        // And so is MediaWiki, which demands "\\\n" be escaped but a raw
        // unescaped one is fine?
        assert_eq!(decode_css("a\nb").as_deref(), Some("a\nb"));
        assert_eq!(decode_css("\\01f4a98").as_deref(), Some("💩8"));
        assert_eq!(decode_css("\\1f4a9 8").as_deref(), Some("💩8"));
        assert_eq!(decode_css("\\1f4a9\t8").as_deref(), Some("💩8"));
        assert_eq!(decode_css("\\1f4a9\r8").as_deref(), Some("💩8"));
        assert_eq!(decode_css("\\1f4a9\n8").as_deref(), Some("💩8"));
        assert_eq!(decode_css("\\1f4a9\x0c8").as_deref(), Some("💩8"));
        assert_eq!(decode_css("\\1f4a9x").as_deref(), Some("💩x"));
        assert_eq!(decode_css("\\ffffff").as_deref(), None);
        assert_eq!(decode_css("\x7f").as_deref(), None);
        assert_eq!(decode_css("\\7f").as_deref(), None);
    }
}
