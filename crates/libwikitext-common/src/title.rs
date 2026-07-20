//! Types and functions for parsing and formatting MediaWiki title strings.

use super::{
    AnchorEncodeMode, config::Configuration, db::DatabaseProvider, decode_html, escape_id_url,
    url_encode,
};
use core::fmt::Write as _;
use libmisc::{CowExt as _, to_lower};
use libphp_rs::strtr;
use std::borrow::Cow;
use unicode_normalization::UnicodeNormalization as _;

/// A title parsing error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The title contains illegal characters.
    #[error("bad characters in title {0:?}")]
    BadChars(String),
    /// The title is empty inside, much like myself.
    #[error("empty title")]
    Empty,
    /// An error occurred writing to an internal buffer.
    #[error(transparent)]
    Fmt(#[from] core::fmt::Error),
    /// The title is too long.
    #[error("title is too long")]
    Length,
    /// The title contains relative path traversal segments.
    #[error("relative path traversal segments in title {0:?}")]
    Path(String),
    /// The title contains a signature insertion sequence.
    #[error("signature insertion sequence in title {0:?}")]
    Signature(String),
}

/// The title casing strategy for a namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamespaceCase {
    /// The namespace name is case-sensitive.
    CaseSensitive,
    /// The first letter of the namespace name is capitalised.
    FirstLetter,
}

/// An article namespace.
#[derive(Debug, Eq)]
pub struct Namespace {
    /// Named aliases for the namespace.
    pub aliases: &'static [&'static str],
    /// The canonical name of the namespace.
    ///
    /// For example, the canonical 'Project' namespace, present on all MW
    /// installations, is normally given a display name matching the name of the
    /// wiki.
    pub canonical: Option<&'static str>,
    /// The case folding strategy for titles in the namespace.
    pub case: NamespaceCase,
    /// Whether pages within this namespace should be considered the ‘main’
    /// content of the wiki.
    pub content: bool,
    /// The default content model for titles in the namespace.
    pub default_content_model: Option<&'static str>,
    /// The namespace ID.
    pub id: i32,
    /// The display name of the namespace.
    pub name: &'static str,
    /// Whether the namespace supports subpages.
    pub subpages: bool,
}

impl core::hash::Hash for Namespace {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
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
    #[allow(
        clippy::allow_attributes,
        reason = "https://github.com/rust-lang/rust-clippy/issues/13358"
    )]
    #[allow(dead_code, reason = "useful for documentation")]
    pub const TALK: i32 = 1;
    /// The user namespace ID.
    pub const USER: i32 = 2;
    /// The user talk namespace ID.
    pub const USER_TALK: i32 = 3;
    /// The project namespace ID.
    pub const PROJECT: i32 = 4;
    /// The project talk namespace ID.
    pub const PROJECT_TALK: i32 = 5;
    /// The file namespace ID.
    pub const FILE: i32 = 6;
    /// The file talk namespace ID.
    pub const FILE_TALK: i32 = 7;
    /// The system namespace ID.
    pub const MEDIAWIKI: i32 = 8;
    /// The system talk namespace ID.
    pub const MEDIAWIKI_TALK: i32 = 9;
    /// The template namespace ID.
    pub const TEMPLATE: i32 = 10;
    /// The template talk namespace ID.
    pub const TEMPLATE_TALK: i32 = 11;
    /// The help namespace ID.
    pub const HELP: i32 = 12;
    /// The help talk namespace ID.
    pub const HELP_TALK: i32 = 13;
    /// The category namespace ID.
    pub const CATEGORY: i32 = 14;
    /// The category talk namespace ID.
    pub const CATEGORY_TALK: i32 = 15;
    /// The ID of the Scribunto `Module` namespace.
    pub const MODULE: i32 = 828;

    /// Returns the associated ID (talk -> subject, or subject -> talk) of this
    /// namespace.
    #[inline]
    #[must_use]
    pub const fn associated_id(&self) -> i32 {
        if self.is_talk() {
            self.id - 1
        } else {
            self.id + 1
        }
    }

    /// Finds the namespace with the given numeric ID.
    #[must_use]
    pub fn find_by_id(config: &Configuration, id: i32) -> Option<&'static Self> {
        config.namespaces.iter().find(|ns| ns.id == id)
    }

    /// Finds the namespace with the given case-insensitive name. Searches the
    /// name and all aliases.
    #[must_use]
    pub fn find_by_name(config: &Configuration, name: &str) -> Option<&'static Self> {
        config.namespaces.iter().find(|ns| {
            ns.name.eq_ignore_ascii_case(name)
                || ns
                    .canonical
                    .is_some_and(|canonical| name.eq_ignore_ascii_case(canonical))
                || ns
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
        })
    }

    /// Returns true if this is a talk namespace.
    #[inline]
    #[must_use]
    pub const fn is_talk(&self) -> bool {
        self.id > Namespace::MAIN && self.id % 2 == 1
    }

    /// Returns the main namespace.
    ///
    /// # Panics
    ///
    /// * `configuration` contains no main namespace
    #[must_use]
    pub fn main(config: &Configuration) -> &'static Self {
        Self::find_by_id(config, Self::MAIN).unwrap()
    }

    /// Returns the subject namespace for this namespace. If this namespace
    /// is a subject namespace, it is the same as this namespace.
    #[inline]
    #[must_use]
    pub fn subject(&self, config: &Configuration) -> Option<&'static Namespace> {
        Self::find_by_id(config, self.subject_id())
    }

    /// Returns the subject namespace ID for this namespace. If this namespace
    /// is a subject namespace, it is the same ID as this namespace ID.
    #[inline]
    #[must_use]
    pub const fn subject_id(&self) -> i32 {
        if self.is_talk() { self.id - 1 } else { self.id }
    }

    /// Returns the talk namespace for this namespace. If this namespace
    /// is a talk namespace, it is the same as this namespace.
    #[inline]
    #[must_use]
    pub fn talk(&self, config: &Configuration) -> Option<&'static Namespace> {
        Self::find_by_id(config, self.talk_id())
    }

    /// Returns the talk namespace ID for this namespace. If this namespace
    /// is a talk namespace, it is the same ID as this namespace ID.
    #[inline]
    #[must_use]
    pub const fn talk_id(&self) -> i32 {
        if self.is_talk() { self.id } else { self.id + 1 }
    }
}

/// A normalised article title.
#[derive(Clone, Eq)]
pub struct Title {
    /// The location of the fragment delimiter in the title, if one exists.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                                   ^
    /// ```
    fragment_delimiter: Option<u16>,

    /// The location of the interwiki delimiter in the title, if one exists.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///          ^
    /// ```
    iw_delimiter: Option<u16>,

    /// The namespace of the title.
    namespace: &'static Namespace,

    /// The location of the namespace delimiter in the title, if one exists.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                    ^
    /// ```
    ns_delimiter: Option<u16>,

    /// The full title text.
    text: String,
}

impl Title {
    /// Creates a title from a known namespace plus text parts which need to be
    /// normalised.
    ///
    /// # Errors
    ///
    /// * the parts are invalid
    /// * writing to the internal buffer fails
    ///
    /// # Panics
    ///
    /// * `title.len() > u16::MAX`
    pub fn from_parts(
        config: &Configuration,
        namespace: &'static Namespace,
        title: &str,
        fragment: Option<&str>,
        interwiki: Option<&str>,
    ) -> Result<Self, Error> {
        let interwiki = interwiki.map(normalize);
        let title = normalize(title);
        let fragment = fragment.map(normalize);
        Self::new_normalized(
            config,
            namespace,
            &title,
            fragment.as_deref(),
            interwiki.as_deref(),
        )
    }

    /// Creates a title from a known namespace plus text parts which are already
    /// normalised, without checking if the parts are valid.
    ///
    /// # Errors
    ///
    /// * writing to the internal buffer fails
    ///
    /// # Panics
    ///
    /// * `title.len() > u16::MAX`
    pub fn from_parts_unchecked(
        namespace: &'static Namespace,
        title: &str,
        fragment: Option<&str>,
        interwiki: Option<&str>,
    ) -> Result<Self, Error> {
        let mut text = String::with_capacity(
            namespace.name.len()
                + ":".len()
                + title.len()
                + fragment.map_or(0, |s| s.len() + "#".len())
                + interwiki.map_or(0, |s| s.len() + ":".len()),
        );

        let iw_delimiter = interwiki
            .map(|interwiki| {
                let iw_delimiter = interwiki.len();
                write!(text, "{interwiki}:")?;
                Ok::<_, core::fmt::Error>(u16::try_from(iw_delimiter).unwrap())
            })
            .transpose()?;

        let ns_delimiter = (!namespace.name.is_empty())
            .then(|| {
                let ns_delimiter = text.len() + namespace.name.len();
                write!(text, "{}:", namespace.name)?;
                Ok::<_, core::fmt::Error>(u16::try_from(ns_delimiter).unwrap())
            })
            .transpose()?;

        if interwiki.is_none()
            && namespace.case == NamespaceCase::FirstLetter
            && let Some(first) = title.chars().next()
            && first.is_lowercase()
        {
            let rest = &title[first.len_utf8()..];
            write!(text, "{}{rest}", first.to_uppercase())?;
        } else {
            text += title;
        }

        let fragment_delimiter = fragment
            .map(|fragment| {
                let fragment_delimiter = text.len();
                write!(text, "#{fragment}")?;
                Ok::<_, core::fmt::Error>(u16::try_from(fragment_delimiter).unwrap())
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
    /// namespace. Returns `None` if the title is not valid.
    ///
    /// In MediaWiki, this is like `newFromText`.
    ///
    /// # Errors
    ///
    /// * `text` is not a valid title string
    /// * writing to the internal buffer fails
    pub fn new(config: &Configuration, text: &str, default_ns: Option<i32>) -> Result<Self, Error> {
        let text = normalize(text);

        // Namespaced & interwiki titles that start with ':' are given special
        // rendering behaviour, but it could also be an explicit main namespace.
        // It is not possible to know at this point, but that does not stop
        // MediaWiki from overriding the default namespace anyway.
        let (default_ns, mut text) = text
            .strip_prefix(':')
            .map_or((default_ns, text.as_ref()), |text| {
                (Some(Namespace::MAIN), text.trim_start_matches(' '))
            });

        let (mut iw, mut ns) = <_>::default();
        while let Some((lhs, rhs)) = text.split_once(':') {
            // Namespaces and interwiki prefixes may have the same name, and
            // namespaces are given priority. (It does not make much sense that
            // namespaces from one wiki are treated as if they might exist on a
            // foreign wiki, but the Lua mw.title interface acts like this is
            // the case, so wiki.rs does too.)
            let lhs = lhs.trim_end_matches(' ');
            let rhs = rhs.trim_start_matches(' ');
            if lhs.is_empty() {
                // This mustn’t match anything since an empty segment for
                // the main namespace was already extracted by the first
                // split. If this matched again, then `::` would be treated
                // as a double namespace, which is not correct. (It is
                // necessary to do the early split separately because
                // otherwise *this* split would not match an interwiki after
                // an empty segment.)
            } else if let lhs @ Some(_) = Namespace::find_by_name(config, lhs) {
                ns = lhs;
                text = rhs;
            } else if let lhs = to_lower(lhs)
                && let prefix @ Some(_) = config.interwiki_map.get_key(&lhs).copied()
            {
                iw = prefix;
                text = rhs;
                if config.interwiki_self.contains(&lhs) {
                    iw = None;
                    if rhs.is_empty() {
                        text = config.main_page;
                    } else {
                        // After a local interwiki, there are potentially
                        // infinite bonus chances to find another namespace or
                        // interwiki prefix.
                        continue;
                    }
                } else if let Some(rhs) = text.strip_prefix(':') {
                    ns = Some(Namespace::main(config));
                    text = rhs;
                }
            }
            break;
        }

        // MediaWiki checked twice for an empty key part with different
        // conditions at different points in the algorithm, but it turns out
        // that those two conditions are really just this one condition, since
        // if the text was empty from the start then neither `iw` nor `ns` will
        // have been set
        if iw.is_none() && ns.is_none_or(|ns| ns.id != Namespace::MAIN) && text.is_empty() {
            return Err(Error::Empty);
        }

        let ns = ns.unwrap_or_else(|| {
            default_ns
                .and_then(|id| Namespace::find_by_id(config, id))
                .unwrap_or_else(|| Namespace::main(config))
        });

        let (text, fragment) = text.split_once('#').map_or((text, None), |(text, frag)| {
            (text.trim_end_matches(' '), Some(frag))
        });

        Self::new_normalized(config, ns, text, fragment, iw)
    }

    /// Creates a title from a known namespace plus text parts which are already
    /// normalised.
    ///
    /// # Errors
    ///
    /// * the given parts are not valid
    /// * writing to the internal buffer fails
    ///
    /// # Panics
    ///
    /// * `title.len() > u16::MAX`
    fn new_normalized(
        config: &Configuration,
        namespace: &'static Namespace,
        title: &str,
        fragment: Option<&str>,
        interwiki: Option<&str>,
    ) -> Result<Self, Error> {
        if !is_valid(config, title)
            || title.starts_with(':')
            || title.contains(char::REPLACEMENT_CHARACTER)
        {
            Err(Error::BadChars(title.to_owned()))
        } else if path_like(title) {
            Err(Error::Path(title.to_owned()))
        } else if title.contains("~~~") {
            Err(Error::Signature(title.to_owned()))
        } else if title.len() > max_namespace_len(namespace) {
            Err(Error::Length)
        } else if title.is_empty() && interwiki.is_none() && namespace.id != Namespace::MAIN {
            Err(Error::Empty)
        } else {
            Self::from_parts_unchecked(namespace, title, fragment, interwiki)
        }
    }

    /// The parent path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn base_text(&self) -> &str {
        let text = self.text();
        text.rsplit_once('/').map_or(text, |(base, _)| base)
    }

    /// The parent path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn base_url(&self) -> Cow<'_, str> {
        Self::url_encode(self.base_text())
    }

    /// The default content model of the title.
    #[inline]
    #[must_use]
    pub fn default_content_model(&self) -> &'static str {
        const EXT_MODELS: phf::Map<&str, &str> = phf::phf_map! {
            "css" => "css",
            "js" => "javascript",
            "json" => "json",
            "vue" => "vue"
        };
        if matches!(self.namespace().id, Namespace::MEDIAWIKI | Namespace::USER)
            && let Some((lhs, ext)) = self.text().rsplit_once('.')
            && let Some(model) = EXT_MODELS.get(ext)
            && (self.namespace().id == Namespace::MEDIAWIKI || lhs.contains('/'))
        {
            model
        } else if let Some(model) = self.namespace().default_content_model {
            model
        } else {
            "wikitext"
        }
    }

    /// Returns true if this title is considered to “exist” in `db`.
    #[inline]
    #[must_use]
    pub fn exists<Db>(&self, db: &Db) -> bool
    where
        Db: DatabaseProvider,
    {
        self.interwiki().is_some() || self.key().is_empty() || db.contains(self)
    }

    /// The page fragment.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                                    ^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.fragment_delimiter.map(|d| {
            let start_at = usize::from(d) + 1;
            &self.text[start_at..]
        })
    }

    /// The page fragment.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                                    ^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn fragment_url(&self, mode: AnchorEncodeMode) -> Option<Cow<'_, str>> {
        self.fragment()
            .map(|fragment| escape_id_url(fragment, mode))
    }

    /// The full text of the title.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn full_text(&self) -> &str {
        &self.text
    }

    /// The full text of the title in a URI component encoded form.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn full_url(&self) -> Cow<'_, str> {
        Self::url_encode(self.full_text())
    }

    /// The title interwiki identifier.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    /// ^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn interwiki(&self) -> Option<&str> {
        self.iw_delimiter
            .map(|end_at| &self.text[..usize::from(end_at)])
    }

    /// Returns true if this title corresponds to a category.
    #[must_use]
    pub fn is_category(&self, config: &Configuration, from_talk_page: bool) -> bool {
        if let Some(interwiki) = self.interwiki() {
            !from_talk_page && config.interlanguage_map.contains_key(interwiki)
        } else {
            self.namespace.id == Namespace::CATEGORY
        }
    }

    /// Returns true if this title corresponds to a non-interwiki media file.
    #[must_use]
    pub fn is_local_file(&self) -> bool {
        self.interwiki().is_none() && self.namespace.id == Namespace::FILE
    }

    /// Returns true if the title is in a namespace with the given `id`.
    #[inline]
    #[must_use]
    pub fn is_in_namespace(&self, id: i32) -> bool {
        self.namespace().id == id
    }

    /// Converts a page-relative title name to an absolute title name using
    /// `self` as the base title, returning a title name and a display text
    /// part.
    #[must_use]
    pub fn join<'a>(&self, partial: &'a str) -> (Cow<'a, str>, Cow<'a, str>) {
        if !self.namespace().subpages {
            return (Cow::Borrowed(partial), Cow::Borrowed(partial));
        }

        let (target, fragment) = if let Some(p) = partial.find('#') {
            partial.split_at(p)
        } else {
            (partial, "")
        };
        let target = target.trim_ascii();

        if let Some(suffix) = target.strip_prefix('/') {
            let suffix = suffix.trim_end_matches('/');
            let prefix = self.prefixed_text();
            let text = if target.ends_with('/') {
                suffix
            } else {
                target
            };
            let suffix = suffix.trim_ascii();
            let title = Cow::Owned(format!("{prefix}/{suffix}{fragment}"));
            let text = if fragment.is_empty() {
                Cow::Borrowed(text)
            } else {
                Cow::Owned(format!("{text}{fragment}"))
            };

            (title, text)
        } else if target.starts_with("../") {
            let suffix = target.trim_start_matches("../");
            let count = (target.len() - suffix.len()) / "../".len();
            let Some(prefix) = self.prefixed_text().rsplitn(count + 1, '/').nth(count) else {
                return (Cow::Borrowed(partial), Cow::Borrowed(partial));
            };

            let suffix = suffix.trim_end_matches('/');
            let suffix = suffix.trim_ascii();
            let delim = if suffix.is_empty() { "" } else { "/" };
            let title = Cow::Owned::<str>(format!("{prefix}{delim}{suffix}{fragment}"));
            let text = if target.ends_with('/') {
                if fragment.is_empty() {
                    Cow::Borrowed(suffix)
                } else {
                    Cow::Owned(format!("{suffix}{fragment}"))
                }
            } else {
                title.clone()
            };

            (title, text)
        } else {
            (Cow::Borrowed(partial), Cow::Borrowed(partial))
        }
    }

    /// The local part of the title.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///           ^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn key(&self) -> &str {
        let start_at = usize::from(self.iw_delimiter.map_or(0, |d| d + 1));
        let end_at = self.fragment_delimiter.map_or(self.text.len(), usize::from);
        &self.text[start_at..end_at]
    }

    /// The title’s namespace object.
    #[inline]
    #[must_use]
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
    #[inline]
    #[must_use]
    pub fn partial_url(&self) -> Cow<'_, str> {
        Self::url_encode(self.key())
    }

    /// The prefixed text of the title.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn prefixed_text(&self) -> &str {
        let end_at = self.fragment_delimiter.map_or(self.text.len(), usize::from);
        &self.text[..end_at]
    }

    /// The local part of the title, in a URI component encoded form.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    /// ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn prefixed_url(&self) -> Cow<'_, str> {
        Self::url_encode(self.prefixed_text())
    }

    /// The root path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn root_text(&self) -> &str {
        let text = self.text();
        text.split_once('/').map_or(text, |(root, _)| root)
    }

    /// The root path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn root_url(&self) -> Cow<'_, str> {
        Self::url_encode(self.root_text())
    }

    /// Gets the subject title for this page, or `None` if this page’s namespace
    /// does not support subjects.
    pub fn subject<'a>(&'a self, config: &Configuration) -> Option<Cow<'a, Self>> {
        if self.namespace.is_talk() {
            self.namespace
                .subject(config)
                .and_then(|ns| Self::from_parts_unchecked(ns, self.text(), None, None).ok())
                .map(Cow::Owned)
        } else {
            Some(Cow::Borrowed(self))
        }
    }

    /// The subpage path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                               ^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn subpage_text(&self) -> &str {
        let text = self.text();
        text.rsplit_once('/').map_or(text, |(_, sub)| sub)
    }

    /// The subpage path of the page, in a URI component encoded form.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                               ^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn subpage_url(&self) -> Cow<'_, str> {
        Self::url_encode(self.subpage_text())
    }

    /// Gets the talk title for this page, or `None` if this page’s namespace
    /// does not support talk pages.
    pub fn talk<'a>(&'a self, config: &Configuration) -> Option<Cow<'a, Self>> {
        if self.namespace.is_talk() {
            Some(Cow::Borrowed(self))
        } else {
            self.namespace
                .talk(config)
                .and_then(|ns| Self::from_parts_unchecked(ns, self.text(), None, None).ok())
                .map(Cow::Owned)
        }
    }

    /// The path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn text(&self) -> &str {
        let start_at = self
            .ns_delimiter
            .or(self.iw_delimiter)
            .map_or(0, |d| usize::from(d) + 1);
        let end_at = self.fragment_delimiter.map_or(self.text.len(), usize::from);
        &self.text[start_at..end_at]
    }

    /// The path of the page.
    ///
    /// ```text
    /// Interwiki:Namespace:Title/Sub/Page#Fragment
    ///                     ^^^^^^^^^^^^^^
    /// ```
    #[inline]
    #[must_use]
    pub fn text_url(&self) -> Cow<'_, str> {
        Self::url_encode(self.text())
    }

    /// Encodes `text` as a URI component.
    #[inline]
    pub fn url_encode(text: &str) -> Cow<'_, str> {
        strtr(text, &[(" ", "_")]).map(url_encode)
    }
}

impl core::fmt::Debug for Title {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Title")
            .field("fragment", &self.fragment())
            .field("interwiki", &self.interwiki())
            .field("namespace", &self.namespace)
            .field("key", &self.key())
            .field("text", &self.text())
            .finish_non_exhaustive()
    }
}

impl core::fmt::Display for Title {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.full_text())
    }
}

impl core::hash::Hash for Title {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.interwiki().unwrap_or_default().hash(state);
        self.namespace.hash(state);
        self.text().hash(state);
    }
}

impl PartialEq for Title {
    fn eq(&self, other: &Self) -> bool {
        self.interwiki().unwrap_or_default() == other.interwiki().unwrap_or_default()
            && self.namespace == other.namespace
            && self.text() == other.text()
    }
}

/// Returns true if the given character `c` is a bidirectional text control
/// character.
#[inline]
fn bidi(c: char) -> bool {
    ('\u{200e}'..='\u{200f}').contains(&c) || ('\u{202a}'..='\u{202e}').contains(&c)
}

/// Returns true if the given `title` string should be forced to render as a
/// link, even if it is a link to a category.
#[must_use]
pub fn is_force_link(config: &Configuration, title: &str) -> bool {
    title.starts_with(':')
        || title.split_once(':').is_some_and(|(prefix, _)| {
            let prefix = normalize(prefix);
            config.interwiki_self.contains(&to_lower(&prefix))
        })
}

/// Returns true if all the bytes in the given `key` are valid for use in a
/// title.
#[must_use]
fn is_valid(config: &Configuration, key: &str) -> bool {
    #[inline]
    fn is_html_entity(bytes: &[u8]) -> bool {
        bytes[0] == b'&'
            && bytes[1..]
                .iter()
                .position(|b| *b == b';')
                .is_some_and(|end| {
                    bytes[1..end]
                        .iter()
                        .all(|b| b.is_ascii_alphanumeric() || *b >= 0x80)
                })
    }

    #[inline]
    fn is_percent_encoding(bytes: &[u8]) -> bool {
        bytes[0] == b'%'
            && bytes
                .get(1..2)
                .is_some_and(|bytes| bytes.iter().all(u8::is_ascii_hexdigit))
    }

    let bytes = key.as_bytes();
    for pos in 0..key.len() {
        if !config.valid_title_bytes.contains(bytes[pos])
            || is_percent_encoding(&bytes[pos..])
            || is_html_entity(&bytes[pos..])
        {
            return false;
        }
    }
    true
}

/// Returns the maximum length of a title for a namespace.
#[inline]
fn max_namespace_len(ns: &Namespace) -> usize {
    if ns.id == Namespace::SPECIAL {
        512
    } else {
        255
    }
}

/// Normalises a title text part by decoding HTML entities, converting runs of
/// whitespace + underscore to a single space character, trimming, and
/// normalising to Unicode NFC.
#[must_use]
pub fn normalize(text: &str) -> Cow<'_, str> {
    decode_html(text)
        .map(|text| super::normalize_whitespace::<true>(text, trimmable, spacelike))
        .map(|text| {
            if unicode_normalization::is_nfc(text) {
                Cow::Borrowed(text)
            } else {
                Cow::Owned(text.nfc().collect())
            }
        })
}

/// Normalises a title fragment part by decoding HTML entities, converting runs
/// of whitespace + underscore to a single space character, and trimming the
/// right side.
///
/// This is *not* the same as MediaWiki `normalizeFragment`, it is the same as
/// calling `Title::splitTitleString` with a '#' at the start, like
/// `Parser::normalizeSectionName`.
#[must_use]
pub fn normalize_fragment(text: &str) -> Cow<'_, str> {
    super::normalize_whitespace::<false>(text, trimmable, spacelike)
}

/// Returns `true` if `text` contains any upward path traversal parts.
#[inline]
fn path_like(text: &str) -> bool {
    text.split('/').any(|part| matches!(part, "." | ".."))
}

/// Returns true if the character `c` is considered like whitespace in title
/// text.
#[inline]
fn spacelike(c: char) -> bool {
    matches!(
        c,
        '_' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{180E}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// Returns true if the character `c` is trimmable in title text.
#[inline]
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
        assert_eq!(super::normalize(" \t A b"), Cow::Borrowed("\t A b"));
        assert_eq!(super::normalize("A b   "), Cow::Borrowed("A b"));
        assert_eq!(super::normalize("\u{200e}A b   \u{202e}"), "A b");
    }
}
