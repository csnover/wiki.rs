//! MediaWiki Scribunto Lua article support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::{
    common::{make_url, url_encode, url_encode_bytes},
    db::Database,
    title::{Namespace, Title},
};
use arc_cell::OptionalArcCell;
use axum::http::Uri;
use core::cell::Ref;
use piccolo::StashedTable;
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    fmt::Write as _,
};

/// The article support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(crate) struct TitleLibrary {
    /// The base URI to use when generating URLs to articles.
    base_uri: RefCell<Option<Uri>>,
    /// The article database.
    db: OptionalArcCell<Database<'static>>,
    /// The title of the current article being rendered.
    this_title: Cell<Option<StashedTable>>,
}

impl TitleLibrary {
    /// Sets the title of the current (root) article.
    pub fn set_title(&self, ctx: Context<'_>, title: &Title) -> Result<(), RuntimeError> {
        let this_title = self.current_title(ctx);
        update_title(this_title, ctx, title, true)
    }

    /// Sets static shared state required for the library to function.
    pub fn set_shared(&self, base_uri: &Uri, db: &Arc<Database<'static>>) {
        *self.base_uri.borrow_mut() = Some(base_uri.clone());
        self.db.set(Some(Arc::clone(db)));
    }

    mw_unimplemented! {
        getCategories = get_categories,
        getPageLangCode = get_page_lang_code,
    }

    /// Gets information about cascading title protection for the article with
    /// the given title text?
    fn cascading_protection<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'gc>,
    ) -> Result<Table<'gc>, VmError<'gc>> {
        log::warn!("stub: mw.title.cascadingProtection({text:?})");
        Ok(table! {
            using ctx;

            sources = Table::new(&ctx),
            restrictions = Table::new(&ctx),
        })
    }

    /// Makes a new Lua title object for an article.
    ///
    /// `text_or_id` can be the title of an article or an
    /// [article ID](crate::db::Article::id).
    fn new_title<'gc>(
        &self,
        ctx: Context<'gc>,
        (text_or_id, default_ns): (Value<'gc>, Option<Value<'gc>>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        if text_or_id.to_numeric().is_some() {
            return Err("with numeric page id not implemented yet"
                .into_value(ctx)
                .into());
        }

        // log::trace!("newTitle({text_or_id:?}, {default_ns:?})");

        let Some(text) = text_or_id.into_string(ctx) else {
            return Err("wrong type passed to new_title".into_value(ctx).into());
        };

        let text = text.to_str()?;
        let default_ns = default_ns
            .map(|ns| namespace_from_value(ctx, ns))
            .transpose()?;
        let title = Title::new(text, default_ns);

        make_title_table(ctx, self.current_title(ctx), &title)
    }

    /// Gets an attribute for an article with the given title text.
    fn get_attribute_value<'gc>(
        &self,
        ctx: Context<'gc>,
        (prefixed_text, k): (VmString<'_>, VmString<'gc>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        log::trace!("mw.title.getAttributeValue({prefixed_text:?}, {k:?})");

        Ok(match k.as_bytes() {
            b"contentModel" | b"exists" | b"id" | b"isRedirect" => {
                let expensive = self.get_expensive_data(ctx, prefixed_text)?;
                expensive.get_value(ctx, k)
            }
            _ => Value::Nil,
        })
    }

    /// Gets the body of an article with the given title text.
    fn get_content<'gc>(
        &self,
        ctx: Context<'gc>,
        full_text: VmString<'_>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        log::trace!("mw.title.getContent({full_text:?})");
        let title = Title::new(full_text.to_str()?, None);
        let db = self.db.get().unwrap();
        Ok(db.get(title.key()).map_or(Value::Nil, |article| {
            Value::String(ctx.intern(article.body.as_bytes()))
        }))
    }

    /// Gets the ‘expensive’ data for an article.
    fn get_expensive_data<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'_>,
    ) -> Result<Table<'gc>, VmError<'gc>> {
        // log::trace!("getExpensiveData({text:?})");
        let title = Title::new(text.to_str()?, None);
        let article = self.db.get().unwrap().get(title.key()).ok();
        let article = article.as_deref();

        Ok(table! {
            using ctx;

            contentModel = ctx.intern(article
                .map_or("wikitext", |article| &article.model)
                .as_bytes()),
            exists = article.is_some(),
            id = i64::try_from(article.map(|article| article.id).unwrap_or_default())?,
            isRedirect = article.is_some_and(|article| article.redirect.is_some()),
        })
    }

    /// Gets metadata about a file-type article with the given title text.
    /// Returns false if the article is not a file-type article.
    fn get_file_info<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'_>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        let title = Title::new(text.to_str()?, None);
        Ok(
            if [Namespace::FILE, Namespace::MEDIA].contains(&title.namespace().id) {
                table! {
                    using ctx;
                    exists = false
                }
                .into()
            } else {
                false.into()
            },
        )
    }

    /// Creates a URL for an article with the given title text and optional
    /// query string.
    ///
    /// The `which` argument describes the kind of URL to create:
    ///
    /// * 'fullUrl': A fully qualified URL for the title, optionally using
    ///   `proto` to use a specific URL scheme. If `proto` is not specified, the
    ///   URL will be protocol-relative.
    /// * 'canonicalUrl': A fully qualified URL for the title.
    /// * 'localUrl': An URL containing only the path to the title.
    pub(super) fn get_url<'gc>(
        &self,
        ctx: Context<'gc>,
        (text, which, query, proto): (
            VmString<'_>,
            VmString<'_>,
            Option<Value<'gc>>,
            Option<VmString<'_>>,
        ),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::trace!("stub: mw.title.getUrl({text:?}, {which:?}, {query:?}, {proto:?})");

        let query = if let Some(Value::Table(table)) = query {
            Some(Cow::Owned(make_query_string(ctx, table, None)?))
        } else if let Some(Value::String(string)) = query {
            Some(Cow::Borrowed(string.to_str()?))
        } else {
            None
        };

        let base_uri = Ref::filter_map(self.base_uri.borrow(), Option::as_ref)
            .map_err(|_| "missing base uri".into_value(ctx))?;
        let (proto, is_local) = match which.as_bytes() {
            b"fullUrl" => {
                let proto = if let Some(proto) = proto {
                    match proto.to_str()? {
                        proto @ ("http" | "https") => Some(proto),
                        "relative" => None,
                        "canonical" => base_uri.scheme_str(),
                        _ => return Err("invalid 'proto' argument".into_value(ctx).into()),
                    }
                } else {
                    None
                };
                (proto, false)
            }
            b"canonicalUrl" => (base_uri.scheme_str(), false),
            b"localUrl" => (None, true),
            _ => return Err("invalid 'which' argument".into_value(ctx).into()),
        };

        let url = make_url(proto, &base_uri, text.to_str()?, query.as_deref(), is_local)?;
        Ok(url.into_value(ctx))
    }

    /// Makes a new title object for an article with the given title text,
    /// optional fragment, and optional interwiki target.
    fn make_title<'gc>(
        &self,
        ctx: Context<'gc>,
        (ns, text, fragment, interwiki): (
            Value<'gc>,
            VmString<'gc>,
            Option<VmString<'gc>>,
            Option<VmString<'gc>>,
        ),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::trace!("mw.title.makeTitle({ns:?}, {text:?}, {fragment:?}, {interwiki:?})");

        let ns = namespace_from_value(ctx, ns)?;
        let text = text.to_str()?;
        let fragment = fragment.map(VmString::to_str).transpose()?;
        let interwiki = interwiki.map(VmString::to_str).transpose()?;
        let title = Title::from_parts(ns, text, fragment, interwiki)?;
        make_title_table(ctx, self.current_title(ctx), &title)
    }

    /// Returns the protection levels of the article with the given title text?
    fn protection_levels<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'gc>,
    ) -> Result<Table<'gc>, VmError<'gc>> {
        log::warn!("stub: mw.title.protectionLevels({text:?})");
        Ok(table! {
            using ctx;

            create = Table::new(&ctx),
            edit = Table::new(&ctx),
            move = Table::new(&ctx),
            upload = Table::new(&ctx),
            review = Table::new(&ctx),
        })
    }

    /// Sets an arbitrary output flag on the parser if the current title matches
    /// the one given in `text`.
    fn record_vary_flag<'gc>(
        &self,
        _: Context<'gc>,
        (_text, _flag): (VmString<'_>, VmString<'_>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        Ok(Value::Nil)
    }

    /// If the article with the given title text is a redirect article, returns
    /// a title object for the redirect target.
    fn redirect_target<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'_>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::trace!("redirectTarget({text:?})");

        // In MW this will try to inspect the content to get the redirect target
        // using type-specific subclasses. It does not seem to be necessary to
        // do this since the dump includes the redirect target. The cool thing
        // about mw.title.lua is that if this ever fails it returns false and
        // that breaks basically every module since they blindly expect to get
        // a table.
        if let Ok(target) = self
            .db
            .get()
            .unwrap()
            .get(Title::new(text.to_str()?, None).key())
            && let Some(target) = &target.redirect
        {
            let title = Title::new(target, None);
            make_title_table(ctx, self.current_title(ctx), &title)
        } else {
            Ok(Value::Nil)
        }
    }

    /// Gets the current Lua title object from stashed context.
    // TODO: This sucks and comes from before the `Title` struct was a thing.
    // Most of the code to do with changing titles should be uplifted into the
    // Title object.
    fn current_title<'gc>(&self, ctx: Context<'gc>) -> Table<'gc> {
        let stashed_title = self.this_title.take();
        let this_title = ctx.fetch(stashed_title.as_ref().unwrap());
        self.this_title.set(stashed_title);
        this_title
    }
}

impl MwInterface for TitleLibrary {
    const NAME: &str = "mw.title";
    const CODE: &[u8] = include_bytes!("./modules/mw.title.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            cascadingProtection = cascading_protection,
            getAttributeValue = get_attribute_value,
            getCategories = get_categories,
            getContent = get_content,
            getExpensiveData = get_expensive_data,
            getFileInfo = get_file_info,
            getPageLangCode = get_page_lang_code,
            getUrl = get_url,
            makeTitle = make_title,
            newTitle = new_title,
            protectionLevels = protection_levels,
            recordVaryFlag = record_vary_flag,
            redirectTarget = redirect_target,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        // The title will get filled in later by `set_title` when a new page is
        // rendered, but the object is needed now because it is held by
        // reference by the lua script
        let this_title = Table::new(&ctx);
        self.this_title.set(Some(ctx.stash(this_title)));
        Ok(table! {
            using ctx;

            NS_MEDIA = Namespace::MEDIA,
            thisTitle = this_title
        })
    }
}

/// Returns true if the given title appears to be the same as the current title.
// TODO: Again, this sucks. It should be using the proper comparison operator
// in `Title`.
fn is_current_title<'gc>(
    ctx: Context<'gc>,
    current_title: Table<'gc>,
    title: &Title,
) -> Result<bool, VmError<'gc>> {
    let current_ns = current_title.get::<_, VmString<'_>>(ctx, "nsText")?;
    let current_text = current_title.get::<_, VmString<'_>>(ctx, "text")?;
    Ok(title.namespace().name == current_ns.to_str()?
        && title.base_text() == current_text.to_str()?)
}

/// Builds a URL query string from a Lua table.
fn make_query_string<'gc>(
    ctx: Context<'gc>,
    query: Table<'gc>,
    prefix: Option<&str>,
) -> Result<String, VmError<'gc>> {
    let mut out = String::new();
    for (k, v) in query {
        if !v.to_bool() {
            continue;
        }

        let k = prefix.map_or(format!("{}", k.display()), |prefix| {
            format!("{prefix}[{}]", k.display())
        });

        if !out.is_empty() {
            out.push('&');
        }

        if let Value::Table(v) = v {
            out += &make_query_string(ctx, v, Some(&k))?;
        } else if v.is_implicit_string() {
            let v = v.into_string(ctx).unwrap();
            write!(&mut out, "{}", url_encode(&k))?;
            out.push('=');
            write!(&mut out, "{}", url_encode_bytes(&v))?;
        }
    }

    Ok(out)
}

/// Creates a new Lua title object from a [`Title`].
fn make_title_table<'gc>(
    ctx: Context<'gc>,
    current_title: Table<'gc>,
    title: &Title,
) -> Result<Value<'gc>, VmError<'gc>> {
    let title_table = Table::new(&ctx);
    update_title(
        title_table,
        ctx,
        title,
        is_current_title(ctx, current_title, title)?,
    )?;
    Ok(title_table.into())
}

/// Gets a [`Namespace`] from a Lua value.
fn namespace_from_value<'gc>(
    ctx: Context<'gc>,
    ns: Value<'gc>,
) -> Result<&'static Namespace, VmError<'gc>> {
    let ns = if let Some(id) = ns.to_integer() {
        Namespace::find_by_id(id.try_into()?)
    } else if let Some(name) = ns.into_string(ctx) {
        Namespace::find_by_name(name.to_str()?)
    } else {
        return Err(format!("invalid ns type {}", ns.type_name())
            .into_value(ctx)
            .into());
    };

    ns.ok_or_else(|| {
        format!("could not find ns for {ns:?}")
            .into_value(ctx)
            .into()
    })
}

/// Updates the properties of a Lua title object with values from the given
/// [`Title`] object.
fn update_title<'gc>(
    table: Table<'gc>,
    ctx: Context<'gc>,
    title: &Title,
    is_current_title: bool,
) -> Result<(), RuntimeError> {
    table.set_field(ctx, "isCurrentTitle", is_current_title);
    table.set_field(ctx, "isLocal", true);
    table.set_field(ctx, "interwiki", ctx.intern(title.interwiki().as_bytes()));
    table.set_field(ctx, "namespace", title.namespace().id);
    table.set_field(ctx, "nsText", title.namespace().name);
    table.set_field(ctx, "text", ctx.intern(title.text().as_bytes()));
    table.set_field(ctx, "fragment", ctx.intern(title.fragment().as_bytes()));
    table.set_field(
        ctx,
        "thePartialUrl",
        ctx.intern(title.partial_url().to_string().as_bytes()),
    );

    if title.namespace().id == Namespace::SPECIAL {
        table.set_field(ctx, "exists", false);
    }

    if ![Namespace::FILE, Namespace::MEDIA].contains(&title.namespace().id) {
        table.set_field(ctx, "file", false);
    }

    Ok(())
}
