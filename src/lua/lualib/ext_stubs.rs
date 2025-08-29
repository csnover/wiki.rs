//! Wiki.rs Scribunto extension library stubs.
//!
//! All the things that are not implemented but still need to exist so that Lua
//! scripts do not crash when they assume that these extensions must exist and
//! try to use them.

use super::prelude::*;

/// A fake extension which stubs various `mw` extension modules.
#[derive(gc_arena::Collect, Default)]
#[collect(require_static)]
pub(super) struct WikiRsStubs;

impl MwInterface for WikiRsStubs {
    const NAME: &str = "WikiRsStubs";
    const CODE: &[u8] = br#"
        local wiki_rs_stubs = {}

        local FlaggedRevs = {}
        function FlaggedRevs.getStabilitySettings( pagename )
            return nil
        end

        local TitleBlacklist = {}
        function TitleBlacklist.test( action, title )
            return nil
        end

        local wikibase = {}
        function wikibase.getEntityIdForCurrentPage()
            return nil
        end

        function wikibase.getEntityIdForTitle( pageTitle, globalSiteId )
            return nil
        end

        function wikibase.getEntity(id)
            return nil
        end

        wikibase.getEntityObject = wikibase.getEntity

        function wikibase.getBestStatements(entityId, propertyId)
            return {}
        end

        function wikibase.getAllStatements(entityId, propertyId)
            return {}
        end

        function wikibase.getEntityUrl(id)
            return nil
        end

        function wikibase.getLabelWithLang(id)
            return nil, nil
        end

        function wikibase.getLabel(id)
            return ""
        end

        wikibase.label = wikibase.getLabel

        function wikibase.getLabelByLang(id, languageCode)
            return nil
        end

        function wikibase.getDescriptionByLang(id, languageCode)
            return nil
        end

        function wikibase.getDescriptionWithLang(id)
            return nil, nil
        end

        function wikibase.getDescription( id )
            return nil
        end

        wikibase.description = wikibase.getDescription

        function wikibase.getSitelink( itemId, globalSiteId )
            return nil
        end

        function wikibase.isValidEntityId(entityIdSerialization)
            return nil
        end

        function wikibase.entityExists(entityId)
            return nil
        end

        function wikibase.getBadges(itemId, globalSiteId)
            return nil
        end

        function wikibase.renderSnak(snakSerialization)
            return nil
        end

        function wikibase.formatValue(snakSerialization)
            return nil
        end

        function wikibase.renderSnaks(snakSerialization)
            return nil
        end

        function wikibase.formatValues(snakSerialization)
            return nil
        end

        function wikibase.resolvePropertyId(propertyLabelOrId)
            return nil
        end

        function wikibase.orderProperties(propertyIds)
            return nil
        end

        function wikibase.getPropertyOrder()
            return nil
        end

        function wikibase.getReferencedEntityId(fromEntityId, propertyId, toIds)
            return nil
        end

        function wikibase.getGlobalSiteId()
            return nil
        end

        function wiki_rs_stubs.setupInterface( options )
            wiki_rs_stubs.setupInterface = nil
            php = mw_interface
            mw_interface = nil

            mw = mw or {}
            mw.ext = mw.ext or {}

            mw.ext.TitleBlacklist = TitleBlacklist
            package.loaded['mw.ext.TitleBlacklist'] = TitleBlacklist

            mw.wikibase = wikibase
            package.loaded['mw.wikibase'] = wikibase

            mw.ext.FlaggedRevs = FlaggedRevs
            package.loaded['mw.ext.FlaggedRevs'] = FlaggedRevs
        end

        return wiki_rs_stubs
    "#;

    fn register(ctx: Context<'_>) -> Table<'_> {
        Table::new(&ctx)
    }

    fn setup<'gc>(&self, ctx: Context<'gc>) -> Result<Table<'gc>, RuntimeError> {
        Ok(Table::new(&ctx))
    }
}
