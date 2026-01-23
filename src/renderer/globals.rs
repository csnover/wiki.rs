//! Collections for semi-structured article data.

use super::{Result, text_run};
use crate::{
    common::anchor_encode,
    wikitext::{HeadingLevel, Span, Spanned, Token, helpers::TextContent, visit::Visitor},
};
use core::fmt;
use std::collections::{BTreeSet, HashMap};

/// A sorted set of categories which the article belongs to.
#[derive(Debug, Default)]
pub(crate) struct Categories(BTreeSet<String>);

impl Categories {
    /// Adds a category to the set.
    pub(super) fn insert(&mut self, value: String) {
        self.0.insert(value);
    }

    /// Emits the categories as an HTML list of links, consuming this object.
    pub fn finish<W: fmt::Write + ?Sized>(
        self,
        f: &mut W,
        base_path: &str,
    ) -> Result<(), fmt::Error> {
        if !self.0.is_empty() {
            f.write_str(r#"<ul class="wiki-rs-categories">"#)?;
            for category in self.0 {
                let target = category.trim_start_matches(':');
                let name = target.trim_start_matches("Category:");
                write!(
                    f,
                    r#"<li><a href="{base_path}/article/{target}">{name}</a></li>"#,
                )?;
            }
            f.write_str("</ul>")?;
        }
        Ok(())
    }
}

/// A collection of indicator badges.
#[derive(Debug, Default)]
pub(crate) struct Indicators(HashMap<String, String>);

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
pub(crate) struct Outline(Vec<(HeadingLevel, String)>);

impl Outline {
    /// Push a new entry to the outline at the given heading level.
    pub(super) fn push(
        &mut self,
        source: &str,
        span: Span,
        level: HeadingLevel,
        content: &[Spanned<Token>],
    ) -> Result<String> {
        // TODO: Duplicate headings should get unique IDs.
        let name = {
            let mut name = String::new();
            let mut extractor = TextContent::new(source, String::new());
            extractor.visit_heading(span, level, content)?;
            text_run(&mut name, '\n', &extractor.finish(), false, false)?;
            name
        };

        let id = anchor_encode(&name);
        self.0.push((level, name));
        Ok(id)
    }
}

impl core::fmt::Display for Outline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0.is_empty() {
            return Ok(());
        }

        write!(f, r##"<ul><li><a href="#">(Top)</a></li>"##)?;
        let mut current = 2;
        for (level, name) in &self.0 {
            while current > u8::from(*level) {
                write!(f, "</ul>")?;
                current -= 1;
            }
            while current < u8::from(*level) {
                write!(f, "<ul>")?;
                current += 1;
            }
            write!(
                f,
                r##"<li><a href="#{}">{}</a></li>"##,
                anchor_encode(name),
                html_escape::encode_text_minimal(name)
            )?;
        }
        while current > 1 {
            write!(f, "</ul>")?;
            current -= 1;
        }
        Ok(())
    }
}
