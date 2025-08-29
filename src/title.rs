//! Types and functions for parsing and formatting MediaWiki title strings.

use html_escape::decode_html_entities;
use percent_encoding::{NON_ALPHANUMERIC, PercentEncode, utf8_percent_encode};
use std::{borrow::Cow, fmt::Write as _};

/// The title casing strategy for a namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamespaceCase {
    /// The first letter of the namespace name is capitalised.
    FirstLetter,
    /// The namespace name is case-sensitive.
    CaseSensitive,
}

/// An article namespace.
#[derive(Debug, Eq)]
#[allow(dead_code)]
pub struct Namespace {
    /// The namespace ID.
    pub id: i32,
    /// The display name of the namespace.
    pub name: &'static str,
    /// The canonical name of the namespace.
    ///
    /// For example, the canonical 'Project' namespace, present on all MW
    /// installations, is normally given a display name matching the name of the
    /// wiki.
    pub canonical: Option<&'static str>,
    /// The case folding strategy for titles in the namespace.
    pub case: NamespaceCase,
    /// The default content model for titles in the namespace.
    pub default_content_model: Option<&'static str>,
    /// Whether the namespace supports subpages.
    pub subpages: bool,
    /// Whether pages within this namespace should be considered the ‘main’
    /// content of the wiki.
    pub content: bool,
    /// Named aliases for the namespace.
    pub aliases: &'static [&'static str],
}

impl PartialEq for Namespace {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Namespace {
    /// The special namespace ID used for direct links to media files.
    pub const MEDIA: i32 = -2;
    /// The special namespace ID used for dynamic pages.
    pub const SPECIAL: i32 = -1;
    /// The main namespace ID.
    pub const MAIN: i32 = 0;
    /// The talk namespace ID.
    #[allow(dead_code)]
    pub const TALK: i32 = 1;
    /// The user namespace ID.
    pub const USER: i32 = 2;
    /// The user talk namespace ID.
    pub const USER_TALK: i32 = 3;
    /// The project namespace ID.
    #[allow(dead_code)]
    pub const PROJECT: i32 = 4;
    /// The project talk namespace ID.
    #[allow(dead_code)]
    pub const PROJECT_TALK: i32 = 5;
    /// The file namespace ID.
    pub const FILE: i32 = 6;
    /// The file talk namespace ID.
    #[allow(dead_code)]
    pub const FILE_TALK: i32 = 7;
    /// The system namespace ID.
    #[allow(dead_code)]
    pub const MEDIAWIKI: i32 = 8;
    /// The system talk namespace ID.
    #[allow(dead_code)]
    pub const MEDIAWIKI_TALK: i32 = 9;
    /// The template namespace ID.
    pub const TEMPLATE: i32 = 10;
    /// The template talk namespace ID.
    #[allow(dead_code)]
    pub const TEMPLATE_TALK: i32 = 11;
    /// The help namespace ID.
    #[allow(dead_code)]
    pub const HELP: i32 = 12;
    /// The help talk namespace ID.
    #[allow(dead_code)]
    pub const HELP_TALK: i32 = 13;
    /// The category namespace ID.
    #[allow(dead_code)]
    pub const CATEGORY: i32 = 14;
    /// The category talk namespace ID.
    #[allow(dead_code)]
    pub const CATEGORY_TALK: i32 = 15;

    /// Returns the associated ID (talk -> subject, or subject -> talk) of this
    /// namespace.
    #[inline]
    pub const fn associated_id(&self) -> i32 {
        if self.is_talk() {
            self.id - 1
        } else {
            self.id + 1
        }
    }

    /// Returns true if this is a talk namespace.
    #[inline]
    pub const fn is_talk(&self) -> bool {
        self.id > Namespace::MAIN && self.id % 2 == 1
    }

    /// Returns the talk namespace for this namespace. If this namespace
    /// is a talk namespace, it is the same as this namespace.
    #[inline]
    pub fn talk(&self) -> Option<&'static Namespace> {
        Namespace::find_by_id(self.talk_id())
    }

    /// Returns the talk namespace ID for this namespace. If this namespace
    /// is a talk namespace, it is the same ID as this namespace ID.
    #[inline]
    pub const fn talk_id(&self) -> i32 {
        if self.is_talk() { self.id } else { self.id + 1 }
    }

    /// Returns the subject namespace for this namespace. If this namespace
    /// is a subject namespace, it is the same as this namespace.
    #[inline]
    pub fn subject(&self) -> Option<&'static Namespace> {
        Namespace::find_by_id(self.subject_id())
    }

    /// Returns the subject namespace ID for this namespace. If this namespace
    /// is a subject namespace, it is the same ID as this namespace ID.
    #[inline]
    pub const fn subject_id(&self) -> i32 {
        if self.is_talk() { self.id - 1 } else { self.id }
    }
}

/// A normalised article title.
#[derive(Clone, Debug, Eq)]
pub struct Title {
    /// The location of the fragment delimiter in the title, if one exists.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                                   ^
    /// ```
    fragment_delimiter: Option<usize>,

    /// The location of the interwiki delimiter in the title, if one exists.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///          ^
    /// ```
    iw_delimiter: Option<usize>,

    /// The namespace of the title.
    namespace: &'static Namespace,

    /// The location of the namespace delimiter in the title, if one exists.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                    ^
    /// ```
    ns_delimiter: Option<usize>,

    /// The full title text.
    text: String,
}

impl PartialEq for Title {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Title {
    /// Creates a title from a known namespace plus text parts.
    pub fn from_parts(
        namespace: &'static Namespace,
        title: &str,
        fragment: Option<&str>,
        interwiki: Option<&str>,
    ) -> Result<Self, core::fmt::Error> {
        let mut text = String::with_capacity(title.len());

        let iw_delimiter = interwiki
            .map(|interwiki| {
                let interwiki = normalize(interwiki);
                let iw_delimiter = interwiki.len();
                write!(text, "{interwiki}:")?;
                Ok(iw_delimiter)
            })
            .transpose()?;

        let ns_delimiter = (!namespace.name.is_empty())
            .then(|| {
                let ns_delimiter = text.len() + namespace.name.len();
                write!(text, "{}:", namespace.name)?;
                Ok(ns_delimiter)
            })
            .transpose()?;

        let title = normalize(title);
        if namespace.case == NamespaceCase::FirstLetter
            && let Some(first) = title.chars().next()
            && first.is_lowercase()
        {
            let rest = &title[first.len_utf8()..];
            write!(text, "{}{rest}", first.to_uppercase())?;
        } else {
            text += &title;
        }

        let fragment_delimiter = fragment
            .map(|fragment| {
                let fragment_delimiter = text.len();
                write!(text, "#{}", normalize(fragment))?;
                Ok(fragment_delimiter)
            })
            .transpose()?;

        Ok(Self {
            fragment_delimiter,
            iw_delimiter,
            namespace,
            ns_delimiter,
            text,
        })
    }

    /// Creates a new [`Title`] from a title string and optional default
    /// namespace.
    ///
    /// In MediaWiki, this is like `newFromText`.
    pub fn new(text: &str, ns: Option<&'static Namespace>) -> Self {
        let text = normalize(text);

        // TODO: Interwiki
        let (ns, text) = text.split_once(':').map_or((ns, &*text), |(lhs, rhs)| {
            if let Some(ns) = Namespace::find_by_name(lhs.trim_end()) {
                (Some(ns), rhs.trim_start())
            } else {
                (None, &*text)
            }
        });
        let ns = ns.unwrap_or_else(Namespace::main);

        let (text, fragment) = text
            .split_once('#')
            .map_or((text, None), |(text, frag)| (text.trim_end(), Some(frag)));

        Self::from_parts(ns, text, fragment, None).unwrap()
    }

    /// The parent path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^^^^^
    /// ```
    pub fn base_text(&self) -> &str {
        let text = self.text();
        text.rsplit_once('/').map_or(text, |(base, _)| base)
    }

    /// The page fragment.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                                    ^^^^^^^^
    /// ```
    pub fn fragment(&self) -> &str {
        let start_at = self.fragment_delimiter.map_or(self.text.len(), |d| d + 1);
        &self.text[start_at..]
    }

    /// The title interwiki identifier.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    /// ^^^^^^^^^
    /// ```
    #[allow(clippy::unused_self)]
    pub fn interwiki(&self) -> &str {
        let end_at = self.iw_delimiter.unwrap_or(0);
        &self.text[..end_at]
    }

    /// The local part of the title.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///           ^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub fn key(&self) -> &str {
        let start_at = self.iw_delimiter.map_or(0, |d| d + 1);
        let end_at = self.fragment_delimiter.unwrap_or(self.text.len());
        &self.text[start_at..end_at]
    }

    /// The title’s namespace object.
    pub fn namespace(&self) -> &'static Namespace {
        self.namespace
    }

    /// The local part of the title, in a URI component encoded form.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///           ^^^^^^^^^^^^^^^^^^^^^^^^
    ///       (Namespace%3ATitle%25Sub%25Page)
    /// ```
    pub fn partial_url(&self) -> PercentEncode<'_> {
        utf8_percent_encode(self.key(), NON_ALPHANUMERIC)
    }

    /// The root path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^
    /// ```
    pub fn root_text(&self) -> &str {
        let text = self.text();
        text.split_once('/').map_or(text, |(root, _)| root)
    }

    /// The subpage path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                               ^^^^
    /// ```
    pub fn subpage_text(&self) -> &str {
        let text = self.text();
        text.rsplit_once('/').map_or(text, |(_, sub)| sub)
    }

    /// The path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^^^^^^^^^^
    /// ```
    pub fn text(&self) -> &str {
        let start_at = self.ns_delimiter.map_or(0, |d| d + 1);
        let end_at = self.fragment_delimiter.unwrap_or(self.text.len());
        &self.text[start_at..end_at]
    }

    /// The full text of the title.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub(crate) fn full_text(&self) -> &str {
        &self.text
    }
}

impl core::fmt::Display for Title {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.key())
    }
}

/// Returns true if the given character `c` is a bidirectional text control
/// character.
fn bidi(c: char) -> bool {
    ('\u{200e}'..='\u{200f}').contains(&c) || ('\u{202a}'..='\u{202e}').contains(&c)
}

/// Normalises a title text part by decoding HTML entities and converting
/// runs of whitespace + underscore to a single space character.
pub fn normalize(text: &str) -> Cow<'_, str> {
    let decoded = decode_html_entities(text);
    let mut out = String::new();
    let mut flushed = 0;
    let mut iter = decoded.char_indices().peekable();

    while let Some((index, c)) = iter.next() {
        // Peek to avoid switching to owned-mode when encountering a single
        // space
        if trimmable(c) && (c != ' ' || matches!(iter.peek(), Some((_, c)) if trimmable(*c))) {
            // Non-space whitespace + underscores are converted to space and
            // runs of whitespace are collapsed into a single character
            while iter.next_if(|(_, c)| trimmable(*c)).is_some() {}

            // This acts like `trim`, not emitting a space at the start
            // (`index == 0`) or end (`peek().is_none()`) of the text.
            if let Some((next_index, _)) = iter.peek() {
                out += &decoded[flushed..index];
                flushed = *next_index;
                // Bidi markers get stripped because “Sometimes they slip
                // into cut-n-pasted page titles”
                if index != 0 && spacelike(c) {
                    out.push(' ');
                }
            }
        }
    }

    if flushed == 0 {
        match decoded {
            Cow::Borrowed(b) => Cow::Borrowed(b.trim_matches(trimmable)),
            Cow::Owned(o) => Cow::Owned(o.trim_matches(trimmable).to_string()),
        }
    } else {
        out += decoded[flushed..].trim_end_matches(trimmable);
        Cow::Owned(out.to_string())
    }
}

/// Returns true if the character `c` is considered like whitespace in title
/// text.
fn spacelike(c: char) -> bool {
    c == '_' || c.is_whitespace()
}

/// Returns true if the character `c` is trimmable in title text.
fn trimmable(c: char) -> bool {
    bidi(c) || spacelike(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(super::normalize("A b"), Cow::Borrowed("A b"));
        assert_eq!(super::normalize("A_b"), "A b");
        assert_eq!(super::normalize("A_______b"), "A b");
        assert_eq!(super::normalize("A__  __b"), "A b");
        assert_eq!(super::normalize("A  b"), "A b");
        assert_eq!(super::normalize("   A b   "), Cow::Borrowed("A b"));
        assert_eq!(super::normalize(" \t A b"), Cow::Borrowed("A b"));
        assert_eq!(super::normalize("A b   "), Cow::Borrowed("A b"));
        assert_eq!(super::normalize("\u{200e}A b   \u{202e}"), "A b");
    }
}
