//! Text replacement transformer.

use super::{Sink, chainable, flush_ws, tokenise};
use crate::StripMarker;
use icu_locale::Locale;
use libmisc::CowExt as _;
use libwikitext_convert::Converter;
use std::borrow::Cow;

/// Converts phrases in text using an automatic language converter.
#[derive(Debug)]
pub(crate) struct ReplaceText<S: Sink> {
    /// The accumulator for a run of text.
    buffer: String,
    /// The language converter.
    converter: Converter,
    /// If true, currently processing an attribute.
    in_attr: bool,
    /// The current number of code contexts.
    ///
    /// Text replacement does not apply in code contexts.
    in_code: u8,
    /// The output.
    next: S,
    /// Manual replacement terms table.
    terms: Dictionary,
    /// Target locale.
    to: Locale,
}

chainable!(ReplaceText);

impl<S: Sink> ReplaceText<S> {
    /// Creates a new `PrettyText` chained to `next`.
    #[inline]
    pub fn new(converter: Converter, to: Locale, next: S) -> Self {
        Self {
            buffer: <_>::default(),
            converter,
            in_attr: <_>::default(),
            in_code: <_>::default(),
            next,
            terms: <_>::default(),
            to,
        }
    }

    /// Flushes the text buffer to the next sink.
    fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        let text = if self.in_attr && self.buffer.contains("://") {
            Cow::Borrowed(self.buffer.as_str())
        } else {
            // TODO: Technically, manually defined terms are supposed to
            // integrate with the converter’s terms dictionary, if it has one,
            // such that the leftmost-longest rule applies to everything and the
            // searches are non-overlapping, and such that default terms can be
            // deleted. As usual, everything in MediaWiki was designed to be
            // maximally slow and annoying to implement. So this should pass a
            // function into the converter that the converter can use to ask
            // if there is a match at each position, thus destroying all
            // optimisations.
            self.terms
                .replace_all(&self.buffer)
                .map(|text| (self.converter)(text, &self.to))
        };
        flush_ws(&mut self.next, &text);
        self.buffer.clear();
    }

    /// Returns `true` if the given tag name is a code tag.
    #[inline]
    fn is_code_tag(name: &str) -> bool {
        matches!(name, "code" | "math" | "pre" | "script" | "style" | "svg")
    }

    /// Returns a mutable reference to the replacement terms table.
    #[inline]
    pub fn terms_mut(&mut self) -> &mut Dictionary {
        &mut self.terms
    }
}

impl<S: Sink> Sink for ReplaceText<S> {
    #[inline]
    fn comment_end(&mut self) {
        self.flush();
        self.in_code -= 1;
        self.next.comment_end();
    }

    #[inline]
    fn comment_start(&mut self) {
        self.flush();
        self.in_code += 1;
        self.next.comment_start();
    }

    #[inline]
    fn entity(&mut self, value: char, raw: &str) {
        if self.in_code == 0 {
            self.buffer.push(value);
        } else {
            self.next.entity(value, raw);
        }
    }

    #[inline]
    fn finish(mut self) -> String {
        self.flush();
        self.next.finish()
    }

    #[inline]
    fn new_line(&mut self) {
        if self.in_code == 0 {
            self.buffer.push('\n');
        } else {
            self.next.new_line();
        }
    }

    #[inline]
    fn strip_marker(&mut self, marker: &StripMarker<'_>) {
        if let StripMarker::General(html) = marker {
            tokenise(self, html);
        } else {
            self.flush();
            self.next.strip_marker(marker);
        }
    }

    #[inline]
    fn tag_attribute_end(&mut self, name: &str) {
        if matches!(name, "alt" | "title") {
            self.flush();
        } else {
            self.in_code -= 1;
        }
        self.next.tag_attribute_end(name);
    }

    #[inline]
    fn tag_attribute_start(&mut self, name: &str) {
        if !matches!(name, "alt" | "title") {
            self.in_code += 1;
        }
        self.next.tag_attribute_start(name);
    }

    #[inline]
    fn tag_end(&mut self, name: &str) {
        self.flush();
        // Because this runs before DomTree, it may receive imbalanced tags.
        // TODO: *Should* this run before DomTree? This is this way because it
        // is this way in the original parser.
        self.in_code = self
            .in_code
            .saturating_sub(u8::from(Self::is_code_tag(name)));
        self.next.tag_end(name);
    }

    #[inline]
    fn tag_start(&mut self, name: &str) {
        self.flush();
        self.in_code += u8::from(Self::is_code_tag(name));
        self.next.tag_start(name);
    }

    #[inline]
    fn tag_start_end(&mut self, name: &str) {
        self.next.tag_start_end(name);
    }

    fn text(&mut self, text: &str) {
        if self.in_code == 0 {
            self.buffer += text;
        } else {
            self.next.text(text);
        }
    }
}

/// A text replacement dictionary.
#[derive(Debug)]
pub(crate) struct Dictionary {
    /// The term keys.
    dict: cedarwood::Cedar,
    /// The term replacements.
    replacements: String,
}

impl Default for Dictionary {
    #[inline]
    fn default() -> Self {
        Self {
            dict: <_>::default(),
            // The replacements needs to always have an empty string in it, so
            // that the value 0 can be used as a sentinel for using the original
            // term. See [`Self::remove`] for details.
            replacements: String::from(Self::TERMINATOR),
        }
    }
}

impl Dictionary {
    /// Hey, it’s your old friend again, the C-string terminator!
    const TERMINATOR: char = '\0';

    /// Inserts a new term replacement to the dictionary.
    #[inline]
    pub fn insert(&mut self, term: &str, repl: &str) {
        let index = i32::try_from(self.replacements.len()).unwrap();
        self.dict.update(term, index).unwrap();
        self.replacements += repl;
        self.replacements.push(Self::TERMINATOR);
    }

    /// Removes a term replacement from the dictionary.
    #[inline]
    pub fn remove(&mut self, term: &str) {
        // TODO: When a term replacement is removed, this might actually be
        // an attempt to remove one of the built-in conversion terms (for the
        // MW `ReplacementArray`-based converters, which is most of them, even
        // the ones that are doing very simple transliteration). In this case,
        // the sentinel value should cause the associated key term to be used as
        // a replacement instead of allowing the built-in dictionary to match at
        // this position.
        self.dict.update(term, 0).unwrap();
    }

    /// Replaces any matching terms in `text`.
    #[inline]
    fn replace_all<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.dict.is_empty() {
            return Cow::Borrowed(text);
        }

        let mut out = String::new();
        let mut pos = 0;
        let mut flushed = 0;
        while pos != text.len() {
            if let Some((index, len)) = self
                .dict
                .common_prefix_iter(&text[pos..])
                .max_by_key(|(_, len)| *len)
                // TODO: Actually use the sentinel value to exclude replacements
                // from the built-in dictionary.
                && index != 0
            {
                out += &text[flushed..pos];
                #[expect(clippy::cast_sign_loss, reason = "index came from usize")]
                let repl = &self.replacements[index as usize..];
                out.extend(repl.chars().take_while(|c| *c != Self::TERMINATOR));
                pos += len;
                flushed = pos;
            } else {
                pos += text.chars().next().unwrap().len_utf8();
            }
        }

        if flushed == 0 {
            Cow::Borrowed(text)
        } else {
            out += &text[flushed..];
            Cow::Owned(out)
        }
    }
}
