//! MediaWiki Scribunto Lua site information support library.

// This code is (very, very loosely) adapted from mediawiki-extensions-Scribunto
// <https://github.com/wikimedia/mediawiki-extensions-Scribunto>.
//
// The upstream copyright is:
//
// SPDX-License-Identifier: GPL-2.0-or-later

use super::prelude::*;
use crate::title::{Namespace, NamespaceCase};
use regex::Regex;
use std::sync::LazyLock;

/// The site information support library.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct SiteLibrary;

impl SiteLibrary {
    /// Returns the ID of the namespace with the given name, if one exists.
    fn get_ns_index<'gc>(
        &self,
        _: Context<'gc>,
        name: VmString<'_>,
    ) -> Result<Value<'gc>, VmError<'gc>> {
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s_]+").unwrap());
        let name = RE.replace_all(name.to_str()?, "_");
        let name = name.trim_matches('_');
        Ok(Namespace::all()
            .iter()
            .find_map(|ns| {
                ns.name
                    .eq_ignore_ascii_case(name)
                    .then_some(i64::from(ns.id))
            })
            .map_or(Value::Nil, Value::from))
    }

    /// Returns a list containing metadata about all the configured interwikis
    /// for this installation.
    fn interwiki_map<'gc>(
        &self,
        ctx: Context<'gc>,
        filter: Option<VmString<'_>>,
    ) -> Result<Table<'gc>, VmError<'gc>> {
        // log::warn!("stub: interwikiMap({filter:?})");
        let _local = filter
            .map(|filter| match filter.as_bytes() {
                b"local" => Ok(true),
                b"!local" => Ok(false),
                _ => Err(format!(
                    "bad argument #1 to 'interwikiMap' (unknown filter '{}')",
                    filter.display_lossy()
                )
                .into_value(ctx)),
            })
            .transpose()?;

        Ok(Table::new(&ctx))
    }

    /// Returns statistics about the number of things in the named category.
    ///
    /// The `which` argument can be:
    ///
    /// * 'all': Members in the category
    /// * 'subcats': Subcategories in the category
    /// * 'files': Files in the category
    /// * 'pages': Content pages in the category
    /// * '*': Return a table containing all of the above
    fn pages_in_category<'gc>(
        &self,
        ctx: Context<'gc>,
        (_category, which): (VmString<'_>, Option<VmString<'_>>),
    ) -> Result<Value<'gc>, VmError<'gc>> {
        // log::warn!("stub: pagesInCategory({category:?}, {which:?})");
        if let Some(which) = which
            && which != "*"
        {
            return Ok(1.into());
        }

        Ok(table! {
            using ctx;

            pages = 1,
            subcats = 1,
            files = 0,
            all = 1,
        }
        .into())
    }

    /// Returns the number of pages in the given namespace.
    fn pages_in_namespace<'gc>(&self, _: Context<'gc>, _ns: i64) -> Result<i64, VmError<'gc>> {
        // log::warn!("stub: pagesInNamespace({ns:?})");
        Ok(1)
    }

    /// Returns the number of users in a named group.
    fn users_in_group<'gc>(
        &self,
        _: Context<'gc>,
        _group: VmString<'_>,
    ) -> Result<i64, VmError<'gc>> {
        // log::warn!("stub: usersInGroup({group:?})");
        Ok(0)
    }
}

impl MwInterface for SiteLibrary {
    const NAME: &str = "mw.site";
    const CODE: &[u8] = include_bytes!("./modules/mw.site.lua");

    fn register(ctx: Context<'_>) -> Table<'_> {
        interface! {
            using Self, ctx;

            getNsIndex = get_ns_index,
            interwikiMap = interwiki_map,
            pagesInCategory = pages_in_category,
            pagesInNamespace = pages_in_namespace,
            usersInGroup = users_in_group,
        }
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        let namespaces = make_namespaces(ctx)?;

        // TODO: Correct path information should come from base_uri
        Ok(table! {
            using ctx;

            currentVersion = env!("CARGO_PKG_VERSION"),
            namespaces = namespaces,
            scriptPath = "/",
            server = "",
            siteName = "wiki.rs",
            stats = table! {
                using ctx;

                pages = 1,
                articles = 1,
                files = 0,
                edits = 1,
                users = 1,
                activeUsers = 1,
                admins = 1,
            },
            stylePath = "/",
        })
    }
}

/// Builds the table of namespaces to be exposed to Lua.
fn make_namespaces(ctx: Context<'_>) -> Result<Table<'_>, RuntimeError> {
    let namespaces = Table::new(&ctx);
    for ns in Namespace::all() {
        let aliases = Table::new(&ctx);
        for (index, alias) in ns.aliases.iter().enumerate() {
            aliases.set(ctx, i32::try_from(index)?, *alias)?;
        }

        let info = table! {
            using ctx;

            id = ns.id,
            name = ns.name,
            canonicalName = ns.canonical.unwrap_or(ns.name),
            hasSubpages = ns.subpages,
            hasGenderDistinction =
                [Namespace::USER, Namespace::USER_TALK].contains(&ns.id),
            isCapitalized = ns.case == NamespaceCase::FirstLetter,
            isContent = ns.id == Namespace::MAIN || ns.content,
            isIncludable = true,
            isMovable = ns.id >= Namespace::MAIN,
            isSubject = !ns.is_talk(),
            isTalk = ns.is_talk(),
            defaultContentModel = ns.default_content_model.unwrap_or_default(),
            aliases = aliases,
            subject = ns.subject_id(),
        };

        if ns.id >= Namespace::MAIN {
            info.set_field(ctx, "talk", ns.talk_id());
            info.set_field(ctx, "associated", ns.associated_id());
        }

        namespaces.set(ctx, ns.id, info)?;
    }

    Ok(namespaces)
}
