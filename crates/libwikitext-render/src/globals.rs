//! Collections for semi-structured article data.

use core::fmt;
use http::Uri;
use libwikitext_common::make_url;
use libwikitext_parse::HeadingLevel;
use std::collections::{BTreeSet, HashMap};

/// A sorted set of categories which the article belongs to.
#[derive(Debug, Default)]
pub struct Categories(BTreeSet<String>);

impl Categories {
    /// Emits the categories as an HTML list of links.
    pub fn fmt<W: fmt::Write + ?Sized>(
        &self,
        f: &mut W,
        base_uri: &Uri,
        article_path: &str,
    ) -> fmt::Result {
        if !self.0.is_empty() {
            f.write_str(r#"<ul class="wiki-rs-categories">"#)?;
            for category in &self.0 {
                let target = category.trim_start_matches(':');
                let name = target.trim_start_matches("Category:");
                let url = make_url(
                    base_uri,
                    None,
                    format_args!("{article_path}/{target}"),
                    None,
                    None,
                );
                write!(f, r#"<li><a href="{url}">{name}</a></li>"#,)?;
            }
            f.write_str("</ul>")?;
        }
        Ok(())
    }

    /// Adds a category to the set.
    pub(super) fn insert(&mut self, value: String) {
        self.0.insert(value);
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
    /// The contents of the outline.
    entries: Vec<OutlineEntry>,
    /// A map from a base anchor ID to the next free suffix for that base ID.
    /// Used to ensure globally unique case-insensitive heading IDs.
    ids: HashMap<String, u32>,
}

/// An outline entry.
#[derive(Debug)]
pub struct OutlineEntry {
    /// The HTML for the entry.
    pub html: String,
    /// The encoded anchor ID for the entry.
    pub id: String,
    /// The level of the entry.
    pub level: HeadingLevel,
}

impl Outline {
    /// Returns an iterator over the recorded outline.
    pub fn iter(&self) -> impl Iterator<Item = &OutlineEntry> {
        self.entries.iter()
    }

    /// Returns the number of outline entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Pushes a new entry to the outline at the given heading level. If the
    /// given ID conflicted with an existing one, a new unique ID is returned.
    pub(super) fn push(&mut self, level: HeadingLevel, html: String, id: String) -> Option<&str> {
        let lower = id.to_ascii_lowercase();
        let (conflict, id) = if let Some(suffix) = self.ids.get_mut(&lower) {
            *suffix += 1;
            (true, format!("{id}_{suffix}"))
        } else {
            self.ids.insert(lower, 1);
            (false, id)
        };

        self.entries.push(OutlineEntry { html, id, level });

        conflict.then(|| self.entries.last().unwrap().id.as_str())
    }
}

impl core::fmt::Display for Outline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.entries.is_empty() {
            return Ok(());
        }

        write!(f, r##"<ul><li><a href="#">(Top)</a></li>"##)?;
        let mut current = 2;
        for OutlineEntry { html, id, level } in &self.entries {
            while current > u8::from(*level) {
                write!(f, "</ul>")?;
                current -= 1;
            }
            while current < u8::from(*level) {
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
