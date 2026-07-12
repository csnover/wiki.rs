//! Collections for semi-structured article data.

use super::{Error, Result};
use core::fmt;
use libmisc::to_ascii_lower;
use libwikitext_common::{
    Messages,
    db::DatabaseProvider,
    make_url,
    title::{Namespace, Title},
    url::Url,
};
use libwikitext_parse::HeadingLevel;
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap, HashSet},
};

/// A sorted set of categories which the article belongs to.
#[derive(Debug, Default)]
pub struct Categories(BTreeSet<String>);

impl Categories {
    /// Returns true if the given `category` exists in this set of categories.
    pub fn contains(&self, category: &str) -> bool {
        self.0.contains(category)
    }

    /// Emits the categories as an HTML list of links.
    pub fn fmt<W: fmt::Write + ?Sized>(
        &self,
        f: &mut W,
        base_uri: &Url,
        article_path: &str,
    ) -> fmt::Result {
        if !self.0.is_empty() {
            f.write_str(r#"<ul class="wiki-rs-categories">"#)?;
            for category in &self.0 {
                let name = html_escape::encode_text(category);
                let target = Title::url_encode(category);
                let url = make_url(
                    base_uri,
                    None,
                    format_args!("{article_path}/{target}"),
                    None,
                    None,
                );
                write!(f, r#"<li><a href="{url}">{name}</a></li>"#)?;
            }
            f.write_str("</ul>")?;
        }
        Ok(())
    }

    /// Adds a category to the set.
    pub(super) fn insert(&mut self, value: &Title) {
        self.0.insert(value.text().to_owned());
    }

    /// Returns true if there are no categories.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of categories.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Adds a tracking category using `messages` with the given `key`.
    pub(super) fn tracking<Db>(&mut self, messages: &Messages<'_, Db>, key: &str) -> Result
    where
        Db: DatabaseProvider,
        Error: From<Db::Error>,
    {
        if let Some(tracking) = messages.get(key, None, true)?
            && tracking != "-"
        {
            let title = Title::new(messages.db().config(), &tracking, Some(Namespace::CATEGORY))?;
            self.insert(&title);
        }
        Ok(())
    }
}

/// A collection of indicator badges.
#[derive(Debug, Default)]
pub struct Indicators(HashMap<String, String>);

impl Indicators {
    /// Adds an indicator to the collection.
    pub(super) fn insert(&mut self, key: String, value: String) {
        self.0.insert(key, value);
    }
}

impl core::fmt::Display for Indicators {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0.is_empty() {
            return Ok(());
        }

        write!(f, r#"<div class="mw-indicators">"#)?;
        for indicator in self.0.values() {
            f.write_str(indicator)?;
        }
        write!(f, "</div>")
    }
}

/// An article outline (table of contents).
#[derive(Debug, Default)]
pub struct Outline {
    /// The string buffer for the outline.
    buffer: String,
    /// The contents of the outline.
    entries: Vec<OutlineEntry>,
    /// A map from a base anchor ID to the next free suffix for that base ID.
    /// Used to ensure globally unique case-insensitive heading IDs.
    ids: HashSet<String>,
}

/// An outline entry.
#[derive(Debug)]
struct OutlineEntry {
    /// The length of the HTML part.
    html_len: u16,
    /// The length of the ID part.
    id_len: u16,
    /// The level of the entry.
    level: HeadingLevel,
    /// The position of the entry in the string buffer.
    pos: u32,
}

/// An outline iterator item.
#[derive(Clone, Copy, Debug)]
pub struct OutlineIterItem<'a> {
    /// The HTML for the entry.
    pub html: &'a str,
    /// The encoded anchor ID for the entry.
    pub id: &'a str,
    /// The level of the entry.
    pub level: HeadingLevel,
}

impl Outline {
    /// Returns an iterator over the recorded outline.
    pub fn iter(&self) -> impl Iterator<Item = OutlineIterItem<'_>> {
        self.entries.iter().map(|entry| {
            let pos = entry.pos as usize;
            let html_len = entry.html_len as usize;
            let id_len = entry.id_len as usize;
            OutlineIterItem {
                html: &self.buffer[pos..pos + html_len],
                id: &self.buffer[pos + html_len..pos + html_len + id_len],
                level: entry.level,
            }
        })
    }

    /// Returns the number of outline entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Pushes a new entry to the outline at the given heading level.
    pub(super) fn push(&mut self, level: HeadingLevel, html: &str, id: &str) {
        let pos = self.buffer.len();
        self.buffer.push_str(html);

        let id_pos = self.buffer.len();
        self.buffer.push_str(id);

        self.entries.push(OutlineEntry {
            html_len: u16::try_from(html.len()).unwrap(),
            id_len: u16::try_from(self.buffer.len() - id_pos).unwrap(),
            level,
            pos: u32::try_from(pos).unwrap(),
        });
    }

    /// Inserts and returns a unique ID for the given `id`.
    pub(super) fn unique_id<'a>(&mut self, id: &'a str) -> Cow<'a, str> {
        let lower = to_ascii_lower(id);
        if self.ids.contains(lower.as_ref()) {
            let mut suffix = 2;
            while !self.ids.insert(format!("{lower}_{suffix}")) {
                suffix += 1;
            }
            Cow::Owned(format!("{id}_{suffix}"))
        } else {
            self.ids.insert(lower.into_owned());
            Cow::Borrowed(id)
        }
    }
}

impl core::fmt::Display for Outline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.entries.is_empty() {
            return Ok(());
        }

        write!(f, r##"<ul><li><a href="#">(Top)</a></li>"##)?;
        let mut current = 2;
        for OutlineIterItem { html, id, level } in self.iter() {
            while current > u8::from(level) {
                write!(f, "</ul>")?;
                current -= 1;
            }
            while current < u8::from(level) {
                write!(f, "<ul>")?;
                current += 1;
            }
            write!(f, r##"<li><a href="#{id}">{html}</a></li>"##)?;
        }
        while current > 1 {
            write!(f, "</ul>")?;
            current -= 1;
        }
        Ok(())
    }
}
