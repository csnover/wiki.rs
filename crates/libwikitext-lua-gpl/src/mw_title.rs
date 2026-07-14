//! MediaWiki Scribunto Lua article support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use core::{
    cell::{Cell, Ref, RefCell},
    fmt::Write as _,
};
use libwikitext_common::{
    Messages,
    config::Configuration,
    db::{Article, DatabaseProvider},
    make_url,
    title::{Namespace, Title},
    url::Url,
    url_encode, url_encode_bytes,
};
use piccolo::StashedTable;
use std::borrow::Cow;

/// The article support library.
#[derive(gc_arena::Collect)]
#[collect(require_static)]
pub struct TitleLibrary<Db> {
    /// The base URI to use when generating URLs to articles.
    base_uri: RefCell<Option<Url>>,
    /// The article database.
    db: RefCell<Option<Db>>,
    /// The title of the current article being rendered.
    this_title: Cell<Option<StashedTable>>,
}

impl<Db> Default for TitleLibrary<Db> {
    fn default() -> Self {
        Self {
            base_uri: <_>::default(),
            db: <_>::default(),
            this_title: <_>::default(),
        }
    }
}

impl<Db> TitleLibrary<Db> {
    /// Returns a reference to the base URI.
    ///
    /// # Panics
    ///
    /// * The database is not set
    #[inline]
    fn base_uri(&self) -> Ref<'_, Url> {
        Ref::map(self.base_uri.borrow(), |base_uri| {
            base_uri.as_ref().unwrap()
        })
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

    /// Returns a reference to the database.
    ///
    /// # Panics
    ///
    /// * The database is not set
    #[inline]
    fn db(&self) -> Ref<'_, Db> {
        Ref::map(self.db.borrow(), |db| db.as_ref().unwrap())
    }

    /// Sets static shared state required for the library to function.
    pub fn set_shared(&self, base_uri: Url, db: Db) {
        *self.base_uri.borrow_mut() = Some(base_uri);
        *self.db.borrow_mut() = Some(db);
    }

    /// Sets the title of the current (root) article.
    pub fn set_title(&self, ctx: Context<'_>, title: &Title) {
        let this_title = self.current_title(ctx);
        update_title(this_title, ctx, title, true);
    }
}

impl<Db> TitleLibrary<Db>
where
    Db: DatabaseProvider,
    for<'a> VmError<'a>: From<Db::Error>,
{
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
        Ok(Title::new(self.db().config(), full_text.to_str()?, None)
            .ok()
            .and_then(|title| self.db().get(&title).transpose())
            .transpose()?
            .map_or(Value::Nil, |article| {
                ctx.intern(article.body().as_bytes()).into()
            }))
    }

    /// Gets the ‘expensive’ data for an article.
    fn get_expensive_data<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'_>,
    ) -> Result<Table<'gc>, VmError<'gc>> {
        // log::trace!("getExpensiveData({text:?})");
        let db = self.db();
        let article = Title::new(db.config(), text.to_str()?, None)
            .ok()
            .and_then(|title| db.get(&title).transpose())
            .transpose()?;
        let article = article.as_deref();

        Ok(table! {
            using ctx;

            contentModel = ctx.intern(article
                .map_or("wikitext", |article| article.model())
                .as_bytes()),
            exists = article.is_some(),
            id = i64::try_from(article.map(Article::id).unwrap_or_default())?,
            isRedirect = article.is_some_and(|article| article.redirect().is_some()),
        })
    }

    /// Gets metadata about a file-type article with the given title text.
    /// Returns false if the article is not a file-type article.
    fn get_file_info<'gc>(
        &self,
        ctx: Context<'gc>,
        text: VmString<'_>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        let is_file = Title::new(self.db().config(), text.to_str()?, None)
            .is_ok_and(|title| matches!(title.namespace().id, Namespace::FILE | Namespace::MEDIA));

        Ok(if is_file {
            table! {
                using ctx;
                exists = false
            }
            .into()
        } else {
            false.into()
        })
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

        let base_uri = self.base_uri();
        let proto = match which.as_bytes() {
            b"fullUrl" => match proto.map(VmString::to_str).transpose()? {
                proto @ Some("http" | "https") => proto,
                Some("relative") | None => Some(""),
                Some("canonical") => base_uri.scheme().or(Some("")),
                Some(_) => return Err("invalid 'proto' argument".into_value(ctx).into()),
            },
            b"canonicalUrl" => base_uri.scheme().or(Some("")),
            b"localUrl" => None,
            _ => return Err("invalid 'which' argument".into_value(ctx).into()),
        };

        Ok(
            if let Ok(title) = Title::new(self.db().config(), text.to_str()?, None) {
                make_url(
                    &base_uri,
                    proto,
                    title.partial_url(),
                    query.as_deref(),
                    title.fragment(),
                )
                .into_value(ctx)
            } else {
                Value::Nil
            },
        )
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

        let db = self.db();
        let config = db.config();
        let ns = namespace_from_value(config, ctx, ns)?;
        let text = text.to_str()?;
        let fragment = fragment.map(VmString::to_str).transpose()?;
        let interwiki = interwiki.map(VmString::to_str).transpose()?;
        if let Ok(title) = Title::from_parts(config, ns, text, fragment, interwiki) {
            make_title_table(ctx, self.current_title(ctx), &title)
        } else {
            Ok(Value::Nil)
        }
    }

    /// Makes a new Lua title object for an article.
    ///
    /// `text_or_id` can be the title of an article or an
    /// [article ID](libwikitext_common::db::Article::id).
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

        let db = self.db();
        let config = db.config();
        let text = text.to_str()?;
        let default_ns = default_ns
            .map(|ns| namespace_from_value(config, ctx, ns))
            .transpose()?;
        if let Ok(title) = Title::new(config, text, default_ns.map(|ns| ns.id)) {
            make_title_table(ctx, self.current_title(ctx), &title)
        } else {
            Ok(Value::Nil)
        }
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
        let db = self.db();
        if let Ok(title) = Title::new(db.config(), text.to_str()?, None)
            && let Ok(Some(target)) = db.get(&title)
            && let Some(target) = &target.redirect()
            && let Ok(title) = Title::new(db.config(), target, None)
        {
            make_title_table(ctx, self.current_title(ctx), &title)
        } else {
            Ok(Value::Nil)
        }
    }
}

impl<Db> MwInterface for TitleLibrary<Db>
where
    Db: DatabaseProvider + 'static,
    for<'a> VmError<'a>: From<Db::Error>,
{
    const CODE: &'static [u8] = include_bytes!("./modules/mw.title.lua");
    const NAME: &'static str = "mw.title";

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

    fn setup<'gc, SetupDb: DatabaseProvider>(
        &self,
        _: &Messages<'_, SetupDb>,
        ctx: Context<'gc>,
    ) -> Result<Table<'gc>, RuntimeError> {
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
    );
    Ok(title_table.into())
}

/// Gets a [`Namespace`] from a Lua value.
fn namespace_from_value<'gc>(
    config: &Configuration,
    ctx: Context<'gc>,
    ns: Value<'gc>,
) -> Result<&'static Namespace, VmError<'gc>> {
    let ns = if let Some(id) = ns.to_integer() {
        Namespace::find_by_id(config, id.try_into()?)
    } else if let Some(name) = ns.into_string(ctx) {
        Namespace::find_by_name(config, name.to_str()?)
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
fn update_title<'gc>(table: Table<'gc>, ctx: Context<'gc>, title: &Title, is_current_title: bool) {
    table.set_field(ctx, "isCurrentTitle", is_current_title);
    table.set_field(ctx, "isLocal", true);
    table.set_field(
        ctx,
        "interwiki",
        ctx.intern(title.interwiki().unwrap_or_default().as_bytes()),
    );
    table.set_field(ctx, "namespace", title.namespace().id);
    table.set_field(ctx, "nsText", title.namespace().name);
    table.set_field(ctx, "text", ctx.intern(title.text().as_bytes()));
    table.set_field(
        ctx,
        "fragment",
        ctx.intern(title.fragment().unwrap_or_default().as_bytes()),
    );
    table.set_field(
        ctx,
        "thePartialUrl",
        ctx.intern(title.partial_url().as_bytes()),
    );

    if title.is_in_namespace(Namespace::SPECIAL) {
        table.set_field(ctx, "exists", false);
    }

    if !matches!(title.namespace().id, Namespace::FILE | Namespace::MEDIA) {
        table.set_field(ctx, "file", false);
    }
}
