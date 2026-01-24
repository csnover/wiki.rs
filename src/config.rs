//! MediaWiki configuration.
//!
//! Wikitext documents are not self-encapsulated and cannot be parsed without
//! out-of-band configuration data. Most of this configuration data can be
//! acquired by querying the MediaWiki API for a given MediaWiki installation.

use super::title::{Namespace, NamespaceCase::FirstLetter};
use crate::wikitext::{Configuration, ConfigurationSource, MagicLinks};
use std::sync::LazyLock;

impl Namespace {
    /// The ID of the Scribunto `Module:` namespace.
    pub const MODULE: i32 = 828;

    /// Returns a list of all namespaces for this installation.
    pub fn all() -> &'static [Self] {
        CONFIG.namespaces
    }

    /// Finds the namespace with the given numeric ID.
    pub fn find_by_id(id: i32) -> Option<&'static Self> {
        CONFIG.namespaces.iter().find(|ns| ns.id == id)
    }

    /// Finds the namespace with the given case-insensitive name. Searches the
    /// name and all aliases.
    pub fn find_by_name(name: &str) -> Option<&'static Self> {
        CONFIG.namespaces.iter().find(|ns| {
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

    /// Returns the main namespace.
    pub fn main() -> &'static Self {
        Namespace::find_by_id(Namespace::MAIN).unwrap()
    }
}

/// The static source configuration for this MW installation.
static CONFIG_SOURCE: ConfigurationSource = ConfigurationSource {
    annotation_tags: phf::phf_set! {},
    annotations_enabled: false,
    behavior_switch_words: phf::phf_set! {
        "notoc", "nogallery", "forcetoc", "toc", "noeditsection", "newsectionlink",
        "nonewsectionlink", "hiddencat", "expectunusedcategory", "expectunusedtemplate",
        "index", "noindex", "staticredirect", "notitleconvert", "nocontentconvert",
        "noglobal", "disambiguation", "archivedtalk", "notalk", "expectedunconnectedpage"
    },
    extension_tags: phf::phf_set! {
        "pre", "nowiki", "gallery", "indicator", "langconvert", "graph", "timeline",
        "hiero", "charinsert", "ref", "references", "inputbox", "imagemap", "source",
        "syntaxhighlight", "poem", "categorytree", "section", "score", "templatestyles",
        "templatedata", "math", "ce", "chem", "maplink", "mapframe", "page-collection",
        "phonos"
    },
    function_hooks: phf::phf_set! {
        "ns", "nse", "urlencode", "lcfirst", "ucfirst", "lc", "uc", "localurl",
        "localurle", "fullurl", "fullurle", "canonicalurl", "canonicalurle", "formatnum",
        "grammar", "gender", "plural", "formal", "bidi", "numberingroup", "language",
        "padleft", "padright", "anchorencode", "defaultsort", "filepath",
        "pagesincategory", "pagesize", "protectionlevel", "protectionexpiry", "pagename",
        "pagenamee", "fullpagename", "fullpagenamee", "subpagename", "subpagenamee",
        "rootpagename", "rootpagenamee", "basepagename", "basepagenamee", "talkpagename",
        "talkpagenamee", "subjectpagename", "subjectpagenamee", "pageid", "revisionid",
        "revisionday", "revisionday2", "revisionmonth", "revisionmonth1", "revisionyear",
        "revisiontimestamp", "revisionuser", "cascadingsources", "namespace",
        "namespacee", "namespacenumber", "talkspace", "talkspacee", "subjectspace",
        "subjectspacee", "numberofarticles", "numberoffiles", "numberofusers",
        "numberofactiveusers", "numberofpages", "numberofadmins", "numberofedits",
        "bcp47", "dir", "interwikilink", "interlanguagelink", "contentmodel", "int",
        "special", "speciale", "tag", "formatdate", "displaytitle", "if", "ifeq",
        "switch", "ifexist", "ifexpr", "iferror", "time", "timel", "timef", "timefl",
        "expr", "rel2abs", "titleparts", "pendingchangelevel", "categorytree", "lst",
        "lstx", "lsth", "target", "babel", "coordinates", "invoke", "related",
        "noexternallanglinks", "shortdesc", "property", "statements",
        "commaseparatedlist", "assessment", "chart", "mentor"
    },
    interwiki_map: phf::phf_map! {
        "acc" => "https://accounts.wmflabs.org/internal.php/viewRequest?id=$1", "acronym"
        => "https://www.acronymfinder.com/$1.html", "advisory" =>
        "https://advisory.wikimedia.org/wiki/$1", "antwiki" =>
        "https://antwiki.org/wiki/$1", "appropedia" => "https://www.appropedia.org/$1",
        "aquariumwiki" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#AquariumWiki",
        "arborwiki" => "https://localwiki.org/ann-arbor/$1", "arxiv" =>
        "https://arxiv.org/abs/$1", "battlestarwiki" =>
        "https://en.battlestarwiki.org/$1", "bcnbio" =>
        "https://www.bcn.cl/historiapolitica/resenas_parlamentarias/wiki/$1", "beacha" =>
        "https://www.beachapedia.org/$1", "betawiki" =>
        "https://translatewiki.net/wiki/$1", "betawikiversity" =>
        "https://beta.wikiversity.org/wiki/$1", "bibcode" =>
        "https://ui.adsabs.harvard.edu/abs/$1/abstract", "bibliowiki" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#Wikilivres",
        "botwiki" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#Botwiki", "boxrec" =>
        "https://boxrec.com/en/boxer/$1", "bugzilla" =>
        "https://bugzilla.wikimedia.org/show_bug.cgi?id=$1", "bulba" =>
        "https://bulbapedia.bulbagarden.net/wiki/$1", "c2" => "https://wiki.c2.com/?$1",
        "ccorg" => "https://creativecommons.org/$1", "cache" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#Google_Cache",
        "centralwikia" => "https://community.fandom.com/wiki/$1", "choralwiki" =>
        "https://www.cpdl.org/wiki/index.php/$1", "citizendium" =>
        "https://en.citizendium.org/wiki/$1", "commons" =>
        "https://commons.wikimedia.org/wiki/$1", "communitywiki" =>
        "https://communitywiki.org/$1", "comune" =>
        "https://rete.comuni-italiani.it/wiki/$1", "creativecommons" =>
        "https://creativecommons.org/licenses/$1", "creativecommonswiki" =>
        "https://wiki.creativecommons.org/$1", "dbdump" =>
        "https://dumps.wikimedia.org/$1/latest/", "dcdatabase" =>
        "https://dc.fandom.com/$1", "dcw" => "https://dcwwiki.org/$1", "debian" =>
        "https://wiki.debian.org/$1", "devmo" => "https://developer.mozilla.org/docs/$1",
        "dico" => "https://dicoado.org/dico/$1", "dicoado" =>
        "https://fr.dicoado.org/dico/$1", "dict" =>
        "https://www.dict.org/bin/Dict?Database=*&Form=Dict1&Strategy=*&Query=$1",
        "dictionary" =>
        "https://www.dict.org/bin/Dict?Database=*&Form=Dict1&Strategy=*&Query=$1",
        "diffblog" => "https://diff.wikimedia.org/$1", "discord" =>
        "https://discord.com/$1", "disinfopedia" =>
        "https://sourcewatch.org/index.php/$1", "dmoz" => "https://curlie.org/$1",
        "dmozs" => "https://curlie.org/search?q=$1", "doi" => "https://doi.org/$1",
        "donate" => "https://donate.wikimedia.org/wiki/$1", "doom_wiki" =>
        "https://doom.fandom.com/wiki/$1", "download" =>
        "https://releases.wikimedia.org/$1", "dpd" => "https://www.rae.es/dpd/$1", "dpla"
        => "https://dp.la/item/$1", "drae" => "https://dle.rae.es/?w=$1", "elibre" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#elibre", "emacswiki"
        => "https://www.emacswiki.org/emacs?$1", "encyc" => "https://encyc.org/wiki/$1",
        "englyphwiki" => "https://en.glyphwiki.org/wiki/$1", "enkol" =>
        "https://enkol.pl/$1", "esolang" => "https://esolangs.org/wiki/$1", "etherpad" =>
        "https://etherpad.wikimedia.org/$1", "ethnologue" =>
        "https://www.ethnologue.com/language/$1", "ethnologuefamily" =>
        "https://www.ethnologue.com/show_family.asp?subid=$1", "exkcd" =>
        "https://www.explainxkcd.com/wiki/index.php/$1", "exotica" =>
        "https://www.exotica.org.uk/wiki/$1", "fandom" =>
        "https://community.fandom.com/wiki/w:c:$1", "wikia" =>
        "https://community.fandom.com/wiki/w:c:$1", "wikiasite" =>
        "https://community.fandom.com/wiki/w:c:$1", "wikicity" =>
        "https://community.fandom.com/wiki/w:c:$1", "fanimutationwiki" =>
        "https://wiki.animutationportal.com/index.php/$1", "fedora" =>
        "https://fedoraproject.org/wiki/$1", "finalfantasy" =>
        "https://finalfantasy.fandom.com/wiki/$1", "finnix" =>
        "https://www.finnix.org/$1", "flickrphoto" =>
        "https://www.flickr.com/photo.gne?id=$1", "flickruser" =>
        "https://www.flickr.com/people/$1", "foldoc" => "https://foldoc.org/$1",
        "foundation" => "https://foundation.wikimedia.org/wiki/$1", "foundationsite" =>
        "https://wikimediafoundation.org/$1", "freebsdman" =>
        "https://www.FreeBSD.org/cgi/man.cgi?apropos=1&query=$1", "freedomdefined" =>
        "https://freedomdefined.org/$1", "freenode" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#freenode", "freesoft"
        => "https://directory.fsf.org/wiki/$1", "gardenology" =>
        "https://www.gardenology.org/wiki/$1", "gentoo" =>
        "https://wiki.gentoo.org/wiki/$1", "genwiki" =>
        "https://wiki.genealogy.net/index.php/$1", "gerrit" =>
        "https://gerrit.wikimedia.org/r/$1", "git" =>
        "https://gerrit.wikimedia.org/g/$1", "gitiles" =>
        "https://gerrit.wikimedia.org/g/$1", "gitlab" =>
        "https://gitlab.wikimedia.org/$1", "globalcontribs" =>
        "https://guc.toolforge.org/?user=$1", "glottolog" =>
        "https://glottolog.org/glottolog?iso=$1", "glottopedia" =>
        "http://glottopedia.org/index.php/$1", "google" =>
        "https://www.google.com/search?q=$1", "googledefine" =>
        "https://www.google.com/search?q=define:$1", "googlegroups" =>
        "https://groups.google.com/groups?q=$1", "gs" =>
        "https://global-search.toolforge.org/?q=$1", "gucprefix" =>
        "https://guc.toolforge.org/?isPrefixPattern=1&src=rc&user=$1", "guildwarswiki" =>
        "https://wiki.guildwars.com/wiki/$1", "gutenberg" =>
        "https://www.gutenberg.org/ebooks/$1", "gutenbergwiki" =>
        "https://meta.wikimedia.org/w/index.php?title=Interwiki_map/discontinued#Gutenbergwiki",
        "hackerspaces" => "https://wiki.hackerspaces.org/$1", "hammondwiki" =>
        "https://www.dairiki.org/HammondWiki/$1", "hdl" => "https://hdl.handle.net/$1",
        "heraldik" => "https://heraldik-wiki.de/wiki/$1", "horizonlabs" =>
        "https://horizon.wikimedia.org/$1", "hrfwiki" =>
        "http://fanstuff.hrwiki.org/index.php/$1", "hrwiki" =>
        "http://www.hrwiki.org/index.php/$1", "iarchive" =>
        "https://archive.org/details/$1", "imdbcompany" =>
        "https://www.imdb.com/company/co$1/", "imdbname" =>
        "https://www.imdb.com/name/nm$1/", "imdbtitle" =>
        "https://www.imdb.com/title/tt$1/", "incubator" =>
        "https://incubator.wikimedia.org/wiki/$1", "infosphere" =>
        "https://theinfosphere.org/$1", "irc" => "irc://irc.libera.chat/$1", "ircrc" =>
        "irc://irc.wikimedia.org/$1", "ircs" => "ircs://irc.libera.chat/$1", "isni" =>
        "https://isni.org/isni/$1", "iso639-3" => "https://iso639-3.sil.org/code/$1",
        "issn" => "https://www.worldcat.org/issn/$1", "iuridictum" =>
        "https://iuridictum.pecina.cz/w/$1", "jaglyphwiki" =>
        "https://glyphwiki.org/wiki/$1", "jira" =>
        "https://jira.toolserver.org/browse/$1", "jstor" =>
        "https://www.jstor.org/journals/$1", "kamelo" =>
        "http://kamelopedia.net/wiki/$1", "karlsruhe" => "https://ka.stadtwiki.net/$1",
        "komicawiki" => "https://wiki.komica.org/?$1", "lexemes" =>
        "https://www.wikidata.org/w/index.php?search=$1&ns146=1", "liberachat" =>
        "ircs://irc.libera.chat/$1", "libreplanet" => "https://libreplanet.org/wiki/$1",
        "lingualibre" => "https://lingualibre.org/wiki/$1", "linguistlist" =>
        "https://linguistlist.org/forms/langs/LLDescription.cfm?code=$1", "listarchive"
        => "https://lists.wikimedia.org/hyperkitty/$1", "localwiki" =>
        "https://localwiki.org/$1", "lofc" => "https://id.loc.gov/authorities/$1",
        "lojban" => "https://mw.lojban.org/papri/$1", "lokalhistoriewiki" =>
        "https://lokalhistoriewiki.no/wiki/$1", "lostpedia" =>
        "https://lostpedia.fandom.com/wiki/$1", "luxo" =>
        "https://guc.toolforge.org/?user=$1", "mail" =>
        "https://lists.wikimedia.org/postorius/lists/$1.lists.wikimedia.org/",
        "mailarchive" => "https://lists.wikimedia.org/pipermail/$1", "mariowiki" =>
        "https://www.mariowiki.com/$1", "marveldatabase" =>
        "https://marvel.fandom.com/wiki/$1", "mdwiki" => "https://mdwiki.org/wiki/$1",
        "meatball" => "http://meatballwiki.org/wiki/$1", "mw" =>
        "https://www.mediawiki.org/wiki/$1", "mediawikiwiki" =>
        "https://www.mediawiki.org/wiki/$1", "mediazilla" =>
        "https://bugzilla.wikimedia.org/$1", "memoryalpha" =>
        "https://memory-alpha.fandom.com/wiki/$1", "metawiki" =>
        "https://meta.wikimedia.org/wiki/$1", "metawikimedia" =>
        "https://meta.wikimedia.org/wiki/$1", "metawikipedia" =>
        "https://meta.wikimedia.org/wiki/$1", "miraheze" =>
        "https://meta.miraheze.org/wiki/$1", "mineralienatlas" =>
        "https://www.mineralienatlas.de/lexikon/index.php/$1", "mixnmatch" =>
        "https://mix-n-match.toolforge.org/#/catalog/$1", "moinmoin" =>
        "https://moinmo.in/$1", "mosapedia" => "https://mosapedia.de/wiki/index.php/$1",
        "mozillawiki" => "https://wiki.mozilla.org/$1", "mozillazinekb" =>
        "https://kb.mozillazine.org/$1", "mwod" =>
        "https://www.merriam-webster.com/dictionary/$1", "mwot" =>
        "https://www.merriam-webster.com/thesaurus/$1", "nara" =>
        "https://catalog.archives.gov/id/$1", "nlab" =>
        "https://ncatlab.org/nlab/show/$1", "wmnoc" => "https://noc.wikimedia.org/$1",
        "wmnoch" => "https://noc.wikimedia.org/conf/highlight.php?file=$1", "nost" =>
        "https://nostalgia.wikipedia.org/wiki/$1", "nostalgia" =>
        "https://nostalgia.wikipedia.org/wiki/$1", "oclc" =>
        "https://www.worldcat.org/oclc/$1", "oeis" => "https://oeis.org/$1", "oewiki" =>
        "https://oesterreichwiki.org/wiki/$1", "oldwikisource" =>
        "https://wikisource.org/wiki/$1", "olpc" => "https://wiki.laptop.org/go/$1",
        "openlibrary" => "https://openlibrary.org/$1", "openstreetmap" =>
        "https://wiki.openstreetmap.org/wiki/$1", "openwetware" =>
        "https://openwetware.org/wiki/$1", "organicdesign" =>
        "https://www.organicdesign.co.nz/$1", "orthodoxwiki" =>
        "https://orthodoxwiki.org/$1", "osmwiki" =>
        "https://wiki.openstreetmap.org/wiki/$1", "otrs" =>
        "https://ticket.wikimedia.org/otrs/index.pl?Action=AgentTicketZoom&TicketID=$1",
        "otrswiki" => "https://vrt-wiki.wikimedia.org/wiki/$1", "outreach" =>
        "https://outreach.wikimedia.org/wiki/$1", "outreachwiki" =>
        "https://outreach.wikimedia.org/wiki/$1", "owasp" =>
        "https://www.owasp.org/index.php/$1", "paws" =>
        "https://public-paws.wmcloud.org/$1", "petscan" =>
        "https://petscan.wmflabs.org/?psid=$1", "phab" =>
        "https://phabricator.wikimedia.org/$1", "phabricator" =>
        "https://phabricator.wikimedia.org/$1", "planetmath" =>
        "https://planetmath.org/alphabetical.html", "pmid" =>
        "https://www.ncbi.nlm.nih.gov/pubmed/$1?dopt=Abstract", "pokewiki" =>
        "https://pokewiki.de/$1", "pokéwiki" => "https://pokewiki.de/$1", "policy" =>
        "https://policy.wikimedia.org/$1", "proofwiki" =>
        "https://proofwiki.org/wiki/$1", "pyrev" =>
        "https://www.mediawiki.org/wiki/Special:Code/pywikipedia/$1", "pythoninfo" =>
        "https://wiki.python.org/moin/$1", "quality" =>
        "https://quality.wikimedia.org/wiki/$1", "quarry" =>
        "https://quarry.wmcloud.org/$1", "rcirc" => "irc://irc.wikimedia.org/$1",
        "regiowiki" => "https://regiowiki.at/wiki/$1", "rev" =>
        "https://www.mediawiki.org/wiki/Special:Code/MediaWiki/$1", "revo" =>
        "https://reta-vortaro.de/#$1", "rfc" =>
        "https://datatracker.ietf.org/doc/html/rfc$1", "rheinneckar" =>
        "https://rhein-neckar-wiki.de/$1", "rodovid" => "https://en.rodovid.org/wk/$1",
        "rt" => "https://rt.wikimedia.org/Ticket/Display.html?id=$1", "scholar" =>
        "https://scholar.google.com/scholar?q=$1", "schoolwiki" =>
        "https://schoolwiki.in/$1", "schoolswp" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#SchoolsWP", "scores"
        => "https://imslp.org/wiki/$1", "scoutwiki" => "https://en.scoutwiki.org/$1",
        "semantic-mw" => "https://www.semantic-mediawiki.org/wiki/$1", "senseislibrary"
        => "https://senseis.xmp.net/?$1", "sep11" =>
        "https://meta.wikimedia.org/wiki/Sep11wiki", "sharemap" =>
        "https://meta.wikimedia.org/w/index.php?title=Interwiki_map/discontinued#Sharemap",
        "shoutwiki" => "https://www.shoutwiki.com/wiki/$1", "silcode" =>
        "https://iso639-3.sil.org/code/$1", "slashdot" =>
        "https://slashdot.org/article.pl?sid=$1", "sourceforge" =>
        "https://sourceforge.net/$1", "spcom" => "https://spcom.wikimedia.org/wiki/$1",
        "species" => "https://species.wikimedia.org/wiki/$1", "stats" =>
        "https://stats.wikimedia.org/$1", "stewardry" =>
        "https://meta.toolforge.org/stewardry/?wiki=$1", "strategy" =>
        "https://strategy.wikimedia.org/wiki/$1", "strategywiki" =>
        "https://strategywiki.org/wiki/$1", "sulutil" =>
        "https://meta.wikimedia.org/wiki/Special:CentralAuth/$1", "svn" =>
        "https://svn.wikimedia.org/viewvc/mediawiki/$1?view=log", "swtrain" =>
        "https://train.spottingworld.com/$1", "tardis" => "https://tardis.wiki/wiki/$1",
        "tclerswiki" => "https://wiki.tcl-lang.org/page/$1", "tenwiki" =>
        "https://ten.wikipedia.org/wiki/$1", "test2wiki" =>
        "https://test2.wikipedia.org/wiki/$1", "testcommons" =>
        "https://test-commons.wikimedia.org/wiki/$1", "testwiki" =>
        "https://test.wikipedia.org/wiki/$1", "testwikidata" =>
        "https://test.wikidata.org/wiki/$1", "tfwiki" => "https://tfwiki.net/wiki/$1",
        "thelemapedia" => "http://www.thelemapedia.org/index.php/$1", "theopedia" =>
        "https://www.theopedia.com/$1", "ticket" =>
        "https://ticket.wikimedia.org/otrs/index.pl?Action=AgentTicketZoom&TicketNumber=$1",
        "tmbw" => "https://tmbw.net/wiki/$1", "tolkiengateway" =>
        "https://tolkiengateway.net/wiki/$1", "toolforge" =>
        "https://iw.toolforge.org/$1", "toolhub" => "https://toolhub.wikimedia.org/$1",
        "toollabs" => "https://iw.toolforge.org/$1", "tools" =>
        "https://toolserver.org/$1", "translatewiki" =>
        "https://translatewiki.net/wiki/$1", "tswiki" =>
        "https://www.mediawiki.org/wiki/Toolserver:$1", "tviv" =>
        "http://tviv.org/wiki/$1", "twiki" => "https://twiki.org/cgi-bin/view/$1", "twl"
        => "https://wikipedialibrary.wmflabs.org/search/?q=$1", "tyvawiki" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#TyvaWiki", "umap" =>
        "https://umap.openstreetmap.fr/$1", "uncyclopedia" =>
        "https://en.uncyclopedia.co/wiki/$1", "unihan" =>
        "https://www.unicode.org/cgi-bin/GetUnihanData.pl?codepoint=$1", "urbandict" =>
        "https://www.urbandictionary.com/define.php?term=$1", "usability" =>
        "https://usability.wikimedia.org/wiki/$1", "usemod" =>
        "https://www.usemod.org/cgi-bin/wiki.pl?$1", "utrs" =>
        "https://utrs-beta.wmflabs.org/appeal/$1", "viaf" => "https://viaf.org/viaf/$1",
        "vikidia" => "https://fr.vikidia.org/wiki/$1", "vlos" =>
        "https://tusach.thuvienkhoahoc.com/wiki/$1", "votewiki" =>
        "https://vote.wikimedia.org/wiki/$1", "vrts" =>
        "https://ticket.wikimedia.org/otrs/index.pl?Action=AgentTicketZoom&TicketID=$1",
        "vrtwiki" => "https://vrt-wiki.wikimedia.org/wiki/$1", "wcna" =>
        "https://wikiconference.org/wiki/$1", "weirdgloop" =>
        "https://meta.weirdgloop.org/w/$1", "werelate" =>
        "https://www.werelate.org/wiki/$1", "wg" =>
        "https://wg-en.wikipedia.org/wiki/$1", "wikiapiary" =>
        "https://wikiapiary.com/wiki/$1", "wikibooks" =>
        "https://en.wikibooks.org/wiki/$1", "wikicities" =>
        "https://community.fandom.com/wiki/w:$1", "wikiconference" =>
        "https://wikiconference.org/wiki/$1", "wikidata" =>
        "https://www.wikidata.org/wiki/$1", "wikiedudashboard" =>
        "https://dashboard.wikiedu.org/$1", "wikifunctions" =>
        "https://www.wikifunctions.org/wiki/$1", "wikifur" =>
        "https://en.wikifur.com/wiki/$1", "wikihow" => "https://www.wikihow.com/$1",
        "wikiindex" => "https://wikiindex.org/$1", "wikilivres" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#Wikilivres",
        "wikilivresru" => "https://wikilivres.ru/$1", "wikimania" =>
        "https://wikimania.wikimedia.org/wiki/$1", "wikimedia" =>
        "https://foundation.wikimedia.org/wiki/$1", "wikinews" =>
        "https://en.wikinews.org/wiki/$1", "wikinfo" =>
        "https://wikinfo.org/w/index.php/$1", "wikinvest" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#Wikinvest",
        "wikipapers" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#Wikipapers",
        "wikipedia" => "https://en.wikipedia.org/wiki/$1", "wikipediawikipedia" =>
        "https://en.wikipedia.org/wiki/Wikipedia:$1", "wikiquote" =>
        "https://en.wikiquote.org/wiki/$1", "wikiskripta" =>
        "https://www.wikiskripta.eu/index.php/$1", "wikisophia" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#Wikisophia",
        "wikisource" => "https://en.wikisource.org/wiki/$1", "wikisp" =>
        "https://wikisp.org/wiki/$1", "wikispecies" =>
        "https://species.wikimedia.org/wiki/$1", "wikispore" =>
        "https://wikispore.wmflabs.org/wiki/$1", "wikispot" =>
        "http://wikispot.org/?action=gotowikipage&v=$1", "wikitech" =>
        "https://wikitech.wikimedia.org/wiki/$1", "labsconsole" =>
        "https://wikitech.wikimedia.org/wiki/$1", "wikitrek" =>
        "https://wikitrek.org/wiki/$1", "wikiti" =>
        "https://wikiti.brandonw.net/index.php?title=$1", "wikiversity" =>
        "https://en.wikiversity.org/wiki/$1", "wikivoyage" =>
        "https://en.wikivoyage.org/wiki/$1", "wikiwikiweb" => "https://wiki.c2.com/?$1",
        "wiktionary" => "https://en.wiktionary.org/wiki/$1", "wm2005" =>
        "https://wikimania2005.wikimedia.org/wiki/$1", "wm2006" =>
        "https://wikimania2006.wikimedia.org/wiki/$1", "wm2007" =>
        "https://wikimania2007.wikimedia.org/wiki/$1", "wm2008" =>
        "https://wikimania2008.wikimedia.org/wiki/$1", "wm2009" =>
        "https://wikimania2009.wikimedia.org/wiki/$1", "wm2010" =>
        "https://wikimania2010.wikimedia.org/wiki/$1", "wm2011" =>
        "https://wikimania2011.wikimedia.org/wiki/$1", "wm2012" =>
        "https://wikimania2012.wikimedia.org/wiki/$1", "wm2013" =>
        "https://wikimania2013.wikimedia.org/wiki/$1", "wm2014" =>
        "https://wikimania2014.wikimedia.org/wiki/$1", "wm2015" =>
        "https://wikimania2015.wikimedia.org/wiki/$1", "wm2016" =>
        "https://wikimania2016.wikimedia.org/wiki/$1", "wm2017" =>
        "https://wikimania2017.wikimedia.org/wiki/$1", "wm2018" =>
        "https://wikimania2018.wikimedia.org/wiki/$1", "wmam" =>
        "https://am.wikimedia.org/wiki/$1", "wmania" =>
        "https://wikimania.wikimedia.org/wiki/$1", "wmar" =>
        "https://www.wikimedia.org.ar/wiki/$1", "wmat" =>
        "https://mitglieder.wikimedia.at/$1", "wmau" =>
        "https://wikimedia.org.au/wiki/$1", "wmbd" => "https://bd.wikimedia.org/wiki/$1",
        "wmbe" => "https://be.wikimedia.org/wiki/$1", "wmbr" =>
        "https://br.wikimedia.org/wiki/$1", "wmca" => "https://ca.wikimedia.org/wiki/$1",
        "wmch" => "https://www.wikimedia.ch/$1", "wmcl" => "https://wikimedia.cl/$1",
        "wmcn" => "https://cn.wikimedia.org/wiki/$1", "wmco" =>
        "https://co.wikimedia.org/wiki/$1", "wmcz" => "https://www.wikimedia.cz/$1",
        "wmcz_docs" => "https://docs.wikimedia.cz/wiki/$1", "wmcz_old" =>
        "https://old.wikimedia.cz/wiki/$1", "wmdc" => "https://wikimediadc.org/wiki/$1",
        "securewikidc" => "https://wikimediadc.org/wiki/$1", "wmde" =>
        "https://wikimedia.de/$1", "wmdeblog" => "https://blog.wikimedia.de/$1", "wmdk"
        => "https://dk.wikimedia.org/wiki/$1", "wmdoc" => "https://doc.wikimedia.org/$1",
        "wmec" => "https://ec.wikimedia.org/wiki/$1", "wmee" =>
        "https://ee.wikimedia.org/wiki/$1", "wmes" => "https://www.wikimedia.es/wiki/$1",
        "wmet" => "https://ee.wikimedia.org/wiki/$1", "wmf" =>
        "https://foundation.wikimedia.org/wiki/$1", "wmfblog" =>
        "https://diff.wikimedia.org/$1", "wmfdashboard" =>
        "https://outreachdashboard.wmflabs.org/$1", "wmfi" =>
        "https://fi.wikimedia.org/wiki/$1", "wmfr" => "https://wikimedia.fr/$1", "wmge"
        => "https://ge.wikimedia.org/wiki/$1", "wmhi" =>
        "https://hi.wikimedia.org/wiki/$1", "wmhk" =>
        "https://meta.wikimedia.org/wiki/Wikimedia_Hong_Kong", "wmhu" =>
        "https://wikimedia.hu/wiki/$1", "wmid" => "https://id.wikimedia.org/wiki/$1",
        "wmil" => "https://www.wikimedia.org.il/$1", "wmin" =>
        "https://meta.wikimedia.org/wiki/Wikimedia_India", "wmit" =>
        "https://wiki.wikimedia.it/wiki/$1", "wmke" =>
        "https://meta.wikimedia.org/wiki/Wikimedia_Kenya", "wmmk" =>
        "https://mk.wikimedia.org/wiki/$1", "wmmx" => "https://mx.wikimedia.org/wiki/$1",
        "wmnl" => "https://nl.wikimedia.org/wiki/$1", "wmno" =>
        "https://no.wikimedia.org/wiki/$1", "wmnyc" =>
        "https://nyc.wikimedia.org/wiki/$1", "wmpa-us" =>
        "https://pa-us.wikimedia.org/wiki/$1", "wmph" =>
        "https://meta.wikimedia.org/wiki/Wiki_Society_of_the_Philippines", "wmpl" =>
        "https://pl.wikimedia.org/wiki/$1", "wmplsite" => "https://wikimedia.pl/$1",
        "wmpt" => "https://pt.wikimedia.org/wiki/$1", "wmpunjabi" =>
        "https://punjabi.wikimedia.org/wiki/$1", "wmromd" =>
        "https://romd.wikimedia.org/wiki/$1", "wmrs" =>
        "https://rs.wikimedia.org/wiki/$1", "wmru" => "https://ru.wikimedia.org/wiki/$1",
        "wmse" => "https://se.wikimedia.org/wiki/$1", "wmsk" =>
        "https://wikimedia.sk/$1", "wmteam" =>
        "https://wikimaniateam.wikimedia.org/wiki/$1", "wmtr" =>
        "https://tr.wikimedia.org/wiki/$1", "wmtw" =>
        "https://meta.wikimedia.org/wiki/Wikimedia_Taiwan", "wmua" =>
        "https://ua.wikimedia.org/wiki/$1", "wmuk" => "https://wikimedia.org.uk/wiki/$1",
        "wmve" => "https://ve.wikimedia.org/wiki/$1", "wmza" =>
        "https://meta.wikimedia.org/wiki/Wikimedia_South_Africa", "wookieepedia" =>
        "https://starwars.fandom.com/wiki/$1", "wowwiki" =>
        "https://wowpedia.fandom.com/wiki/$1", "wplibrary" =>
        "https://wikipedialibrary.wmflabs.org/search/?q=$1", "wurmpedia" =>
        "https://wurmpedia.com/index.php/$1", "xkcd" => "https://xkcd.com/$1", "xtools"
        => "https://xtools.wmcloud.org/$1", "zum" => "https://wiki.zum.de/$1", "c" =>
        "https://commons.wikimedia.org/wiki/$1", "m" =>
        "https://meta.wikimedia.org/wiki/$1", "meta" =>
        "https://meta.wikimedia.org/wiki/$1", "d" => "https://www.wikidata.org/wiki/$1",
        "f" => "https://www.wikifunctions.org/wiki/$1", "aa" =>
        "https://aa.wikipedia.org/wiki/$1", "ab" => "https://ab.wikipedia.org/wiki/$1",
        "ace" => "https://ace.wikipedia.org/wiki/$1", "ady" =>
        "https://ady.wikipedia.org/wiki/$1", "af" => "https://af.wikipedia.org/wiki/$1",
        "ak" => "https://ak.wikipedia.org/wiki/$1", "als" =>
        "https://als.wikipedia.org/wiki/$1", "alt" =>
        "https://alt.wikipedia.org/wiki/$1", "am" => "https://am.wikipedia.org/wiki/$1",
        "ami" => "https://ami.wikipedia.org/wiki/$1", "an" =>
        "https://an.wikipedia.org/wiki/$1", "ang" => "https://ang.wikipedia.org/wiki/$1",
        "ann" => "https://ann.wikipedia.org/wiki/$1", "anp" =>
        "https://anp.wikipedia.org/wiki/$1", "ar" => "https://ar.wikipedia.org/wiki/$1",
        "arc" => "https://arc.wikipedia.org/wiki/$1", "ary" =>
        "https://ary.wikipedia.org/wiki/$1", "arz" =>
        "https://arz.wikipedia.org/wiki/$1", "as" => "https://as.wikipedia.org/wiki/$1",
        "ast" => "https://ast.wikipedia.org/wiki/$1", "atj" =>
        "https://atj.wikipedia.org/wiki/$1", "av" => "https://av.wikipedia.org/wiki/$1",
        "avk" => "https://avk.wikipedia.org/wiki/$1", "awa" =>
        "https://awa.wikipedia.org/wiki/$1", "ay" => "https://ay.wikipedia.org/wiki/$1",
        "az" => "https://az.wikipedia.org/wiki/$1", "azb" =>
        "https://azb.wikipedia.org/wiki/$1", "ba" => "https://ba.wikipedia.org/wiki/$1",
        "ban" => "https://ban.wikipedia.org/wiki/$1", "bar" =>
        "https://bar.wikipedia.org/wiki/$1", "bat-smg" =>
        "https://bat-smg.wikipedia.org/wiki/$1", "bbc" =>
        "https://bbc.wikipedia.org/wiki/$1", "bcl" =>
        "https://bcl.wikipedia.org/wiki/$1", "bdr" =>
        "https://bdr.wikipedia.org/wiki/$1", "be" => "https://be.wikipedia.org/wiki/$1",
        "be-tarask" => "https://be-tarask.wikipedia.org/wiki/$1", "be-x-old" =>
        "https://be-tarask.wikipedia.org/wiki/$1", "bew" =>
        "https://bew.wikipedia.org/wiki/$1", "bg" => "https://bg.wikipedia.org/wiki/$1",
        "bh" => "https://bh.wikipedia.org/wiki/$1", "bi" =>
        "https://bi.wikipedia.org/wiki/$1", "bjn" => "https://bjn.wikipedia.org/wiki/$1",
        "blk" => "https://blk.wikipedia.org/wiki/$1", "bm" =>
        "https://bm.wikipedia.org/wiki/$1", "bn" => "https://bn.wikipedia.org/wiki/$1",
        "bo" => "https://bo.wikipedia.org/wiki/$1", "bpy" =>
        "https://bpy.wikipedia.org/wiki/$1", "br" => "https://br.wikipedia.org/wiki/$1",
        "bs" => "https://bs.wikipedia.org/wiki/$1", "btm" =>
        "https://btm.wikipedia.org/wiki/$1", "bug" =>
        "https://bug.wikipedia.org/wiki/$1", "bxr" =>
        "https://bxr.wikipedia.org/wiki/$1", "ca" => "https://ca.wikipedia.org/wiki/$1",
        "cbk-zam" => "https://cbk-zam.wikipedia.org/wiki/$1", "cdo" =>
        "https://cdo.wikipedia.org/wiki/$1", "ce" => "https://ce.wikipedia.org/wiki/$1",
        "ceb" => "https://ceb.wikipedia.org/wiki/$1", "ch" =>
        "https://ch.wikipedia.org/wiki/$1", "cho" => "https://cho.wikipedia.org/wiki/$1",
        "chr" => "https://chr.wikipedia.org/wiki/$1", "chy" =>
        "https://chy.wikipedia.org/wiki/$1", "ckb" =>
        "https://ckb.wikipedia.org/wiki/$1", "co" => "https://co.wikipedia.org/wiki/$1",
        "cr" => "https://cr.wikipedia.org/wiki/$1", "crh" =>
        "https://crh.wikipedia.org/wiki/$1", "cs" => "https://cs.wikipedia.org/wiki/$1",
        "csb" => "https://csb.wikipedia.org/wiki/$1", "cu" =>
        "https://cu.wikipedia.org/wiki/$1", "cv" => "https://cv.wikipedia.org/wiki/$1",
        "cy" => "https://cy.wikipedia.org/wiki/$1", "da" =>
        "https://da.wikipedia.org/wiki/$1", "dag" => "https://dag.wikipedia.org/wiki/$1",
        "de" => "https://de.wikipedia.org/wiki/$1", "dga" =>
        "https://dga.wikipedia.org/wiki/$1", "din" =>
        "https://din.wikipedia.org/wiki/$1", "diq" =>
        "https://diq.wikipedia.org/wiki/$1", "dsb" =>
        "https://dsb.wikipedia.org/wiki/$1", "dtp" =>
        "https://dtp.wikipedia.org/wiki/$1", "dty" =>
        "https://dty.wikipedia.org/wiki/$1", "dv" => "https://dv.wikipedia.org/wiki/$1",
        "dz" => "https://dz.wikipedia.org/wiki/$1", "ee" =>
        "https://ee.wikipedia.org/wiki/$1", "el" => "https://el.wikipedia.org/wiki/$1",
        "eml" => "https://eml.wikipedia.org/wiki/$1", "en" =>
        "https://en.wikipedia.org/wiki/$1", "eo" => "https://eo.wikipedia.org/wiki/$1",
        "es" => "https://es.wikipedia.org/wiki/$1", "et" =>
        "https://et.wikipedia.org/wiki/$1", "eu" => "https://eu.wikipedia.org/wiki/$1",
        "ext" => "https://ext.wikipedia.org/wiki/$1", "fa" =>
        "https://fa.wikipedia.org/wiki/$1", "fat" => "https://fat.wikipedia.org/wiki/$1",
        "ff" => "https://ff.wikipedia.org/wiki/$1", "fi" =>
        "https://fi.wikipedia.org/wiki/$1", "fiu-vro" =>
        "https://fiu-vro.wikipedia.org/wiki/$1", "fj" =>
        "https://fj.wikipedia.org/wiki/$1", "fo" => "https://fo.wikipedia.org/wiki/$1",
        "fon" => "https://fon.wikipedia.org/wiki/$1", "fr" =>
        "https://fr.wikipedia.org/wiki/$1", "frp" => "https://frp.wikipedia.org/wiki/$1",
        "frr" => "https://frr.wikipedia.org/wiki/$1", "fur" =>
        "https://fur.wikipedia.org/wiki/$1", "fy" => "https://fy.wikipedia.org/wiki/$1",
        "ga" => "https://ga.wikipedia.org/wiki/$1", "gag" =>
        "https://gag.wikipedia.org/wiki/$1", "gan" =>
        "https://gan.wikipedia.org/wiki/$1", "gcr" =>
        "https://gcr.wikipedia.org/wiki/$1", "gd" => "https://gd.wikipedia.org/wiki/$1",
        "gl" => "https://gl.wikipedia.org/wiki/$1", "glk" =>
        "https://glk.wikipedia.org/wiki/$1", "gn" => "https://gn.wikipedia.org/wiki/$1",
        "gom" => "https://gom.wikipedia.org/wiki/$1", "gor" =>
        "https://gor.wikipedia.org/wiki/$1", "got" =>
        "https://got.wikipedia.org/wiki/$1", "gpe" =>
        "https://gpe.wikipedia.org/wiki/$1", "gsw" =>
        "https://als.wikipedia.org/wiki/$1", "gu" => "https://gu.wikipedia.org/wiki/$1",
        "guc" => "https://guc.wikipedia.org/wiki/$1", "gur" =>
        "https://gur.wikipedia.org/wiki/$1", "guw" =>
        "https://guw.wikipedia.org/wiki/$1", "gv" => "https://gv.wikipedia.org/wiki/$1",
        "ha" => "https://ha.wikipedia.org/wiki/$1", "hak" =>
        "https://hak.wikipedia.org/wiki/$1", "haw" =>
        "https://haw.wikipedia.org/wiki/$1", "he" => "https://he.wikipedia.org/wiki/$1",
        "hi" => "https://hi.wikipedia.org/wiki/$1", "hif" =>
        "https://hif.wikipedia.org/wiki/$1", "ho" => "https://ho.wikipedia.org/wiki/$1",
        "hr" => "https://hr.wikipedia.org/wiki/$1", "hsb" =>
        "https://hsb.wikipedia.org/wiki/$1", "ht" => "https://ht.wikipedia.org/wiki/$1",
        "hu" => "https://hu.wikipedia.org/wiki/$1", "hy" =>
        "https://hy.wikipedia.org/wiki/$1", "hyw" => "https://hyw.wikipedia.org/wiki/$1",
        "hz" => "https://hz.wikipedia.org/wiki/$1", "ia" =>
        "https://ia.wikipedia.org/wiki/$1", "iba" => "https://iba.wikipedia.org/wiki/$1",
        "id" => "https://id.wikipedia.org/wiki/$1", "ie" =>
        "https://ie.wikipedia.org/wiki/$1", "ig" => "https://ig.wikipedia.org/wiki/$1",
        "igl" => "https://igl.wikipedia.org/wiki/$1", "ii" =>
        "https://ii.wikipedia.org/wiki/$1", "ik" => "https://ik.wikipedia.org/wiki/$1",
        "ilo" => "https://ilo.wikipedia.org/wiki/$1", "inh" =>
        "https://inh.wikipedia.org/wiki/$1", "io" => "https://io.wikipedia.org/wiki/$1",
        "is" => "https://is.wikipedia.org/wiki/$1", "it" =>
        "https://it.wikipedia.org/wiki/$1", "iu" => "https://iu.wikipedia.org/wiki/$1",
        "ja" => "https://ja.wikipedia.org/wiki/$1", "jam" =>
        "https://jam.wikipedia.org/wiki/$1", "jbo" =>
        "https://jbo.wikipedia.org/wiki/$1", "jv" => "https://jv.wikipedia.org/wiki/$1",
        "ka" => "https://ka.wikipedia.org/wiki/$1", "kaa" =>
        "https://kaa.wikipedia.org/wiki/$1", "kab" =>
        "https://kab.wikipedia.org/wiki/$1", "kbd" =>
        "https://kbd.wikipedia.org/wiki/$1", "kbp" =>
        "https://kbp.wikipedia.org/wiki/$1", "kcg" =>
        "https://kcg.wikipedia.org/wiki/$1", "kg" => "https://kg.wikipedia.org/wiki/$1",
        "kge" => "https://kge.wikipedia.org/wiki/$1", "ki" =>
        "https://ki.wikipedia.org/wiki/$1", "kj" => "https://kj.wikipedia.org/wiki/$1",
        "kk" => "https://kk.wikipedia.org/wiki/$1", "kl" =>
        "https://kl.wikipedia.org/wiki/$1", "km" => "https://km.wikipedia.org/wiki/$1",
        "kn" => "https://kn.wikipedia.org/wiki/$1", "knc" =>
        "https://knc.wikipedia.org/wiki/$1", "ko" => "https://ko.wikipedia.org/wiki/$1",
        "koi" => "https://koi.wikipedia.org/wiki/$1", "kr" =>
        "https://kr.wikipedia.org/wiki/$1", "krc" => "https://krc.wikipedia.org/wiki/$1",
        "ks" => "https://ks.wikipedia.org/wiki/$1", "ksh" =>
        "https://ksh.wikipedia.org/wiki/$1", "ku" => "https://ku.wikipedia.org/wiki/$1",
        "kus" => "https://kus.wikipedia.org/wiki/$1", "kv" =>
        "https://kv.wikipedia.org/wiki/$1", "kw" => "https://kw.wikipedia.org/wiki/$1",
        "ky" => "https://ky.wikipedia.org/wiki/$1", "la" =>
        "https://la.wikipedia.org/wiki/$1", "lad" => "https://lad.wikipedia.org/wiki/$1",
        "lb" => "https://lb.wikipedia.org/wiki/$1", "lbe" =>
        "https://lbe.wikipedia.org/wiki/$1", "lez" =>
        "https://lez.wikipedia.org/wiki/$1", "lfn" =>
        "https://lfn.wikipedia.org/wiki/$1", "lg" => "https://lg.wikipedia.org/wiki/$1",
        "li" => "https://li.wikipedia.org/wiki/$1", "lij" =>
        "https://lij.wikipedia.org/wiki/$1", "lld" =>
        "https://lld.wikipedia.org/wiki/$1", "lmo" =>
        "https://lmo.wikipedia.org/wiki/$1", "ln" => "https://ln.wikipedia.org/wiki/$1",
        "lo" => "https://lo.wikipedia.org/wiki/$1", "lrc" =>
        "https://lrc.wikipedia.org/wiki/$1", "lt" => "https://lt.wikipedia.org/wiki/$1",
        "ltg" => "https://ltg.wikipedia.org/wiki/$1", "lv" =>
        "https://lv.wikipedia.org/wiki/$1", "lzh" =>
        "https://zh-classical.wikipedia.org/wiki/$1", "mad" =>
        "https://mad.wikipedia.org/wiki/$1", "mai" =>
        "https://mai.wikipedia.org/wiki/$1", "map-bms" =>
        "https://map-bms.wikipedia.org/wiki/$1", "mdf" =>
        "https://mdf.wikipedia.org/wiki/$1", "mg" => "https://mg.wikipedia.org/wiki/$1",
        "mh" => "https://mh.wikipedia.org/wiki/$1", "mhr" =>
        "https://mhr.wikipedia.org/wiki/$1", "mi" => "https://mi.wikipedia.org/wiki/$1",
        "min" => "https://min.wikipedia.org/wiki/$1", "mk" =>
        "https://mk.wikipedia.org/wiki/$1", "ml" => "https://ml.wikipedia.org/wiki/$1",
        "mn" => "https://mn.wikipedia.org/wiki/$1", "mni" =>
        "https://mni.wikipedia.org/wiki/$1", "mnw" =>
        "https://mnw.wikipedia.org/wiki/$1", "mo" => "https://mo.wikipedia.org/wiki/$1",
        "mos" => "https://mos.wikipedia.org/wiki/$1", "mr" =>
        "https://mr.wikipedia.org/wiki/$1", "mrj" => "https://mrj.wikipedia.org/wiki/$1",
        "ms" => "https://ms.wikipedia.org/wiki/$1", "mt" =>
        "https://mt.wikipedia.org/wiki/$1", "mus" => "https://mus.wikipedia.org/wiki/$1",
        "mwl" => "https://mwl.wikipedia.org/wiki/$1", "my" =>
        "https://my.wikipedia.org/wiki/$1", "myv" => "https://myv.wikipedia.org/wiki/$1",
        "mzn" => "https://mzn.wikipedia.org/wiki/$1", "na" =>
        "https://na.wikipedia.org/wiki/$1", "nah" => "https://nah.wikipedia.org/wiki/$1",
        "nan" => "https://zh-min-nan.wikipedia.org/wiki/$1", "nap" =>
        "https://nap.wikipedia.org/wiki/$1", "nds" =>
        "https://nds.wikipedia.org/wiki/$1", "nds-nl" =>
        "https://nds-nl.wikipedia.org/wiki/$1", "ne" =>
        "https://ne.wikipedia.org/wiki/$1", "new" => "https://new.wikipedia.org/wiki/$1",
        "ng" => "https://ng.wikipedia.org/wiki/$1", "nia" =>
        "https://nia.wikipedia.org/wiki/$1", "nl" => "https://nl.wikipedia.org/wiki/$1",
        "nn" => "https://nn.wikipedia.org/wiki/$1", "no" =>
        "https://no.wikipedia.org/wiki/$1", "nov" => "https://nov.wikipedia.org/wiki/$1",
        "nqo" => "https://nqo.wikipedia.org/wiki/$1", "nr" =>
        "https://nr.wikipedia.org/wiki/$1", "nrm" => "https://nrm.wikipedia.org/wiki/$1",
        "nso" => "https://nso.wikipedia.org/wiki/$1", "nup" =>
        "https://nup.wikipedia.org/wiki/$1", "nv" => "https://nv.wikipedia.org/wiki/$1",
        "ny" => "https://ny.wikipedia.org/wiki/$1", "oc" =>
        "https://oc.wikipedia.org/wiki/$1", "olo" => "https://olo.wikipedia.org/wiki/$1",
        "om" => "https://om.wikipedia.org/wiki/$1", "or" =>
        "https://or.wikipedia.org/wiki/$1", "os" => "https://os.wikipedia.org/wiki/$1",
        "pa" => "https://pa.wikipedia.org/wiki/$1", "pag" =>
        "https://pag.wikipedia.org/wiki/$1", "pam" =>
        "https://pam.wikipedia.org/wiki/$1", "pap" =>
        "https://pap.wikipedia.org/wiki/$1", "pcd" =>
        "https://pcd.wikipedia.org/wiki/$1", "pcm" =>
        "https://pcm.wikipedia.org/wiki/$1", "pdc" =>
        "https://pdc.wikipedia.org/wiki/$1", "pfl" =>
        "https://pfl.wikipedia.org/wiki/$1", "pi" => "https://pi.wikipedia.org/wiki/$1",
        "pih" => "https://pih.wikipedia.org/wiki/$1", "pl" =>
        "https://pl.wikipedia.org/wiki/$1", "pms" => "https://pms.wikipedia.org/wiki/$1",
        "pnb" => "https://pnb.wikipedia.org/wiki/$1", "pnt" =>
        "https://pnt.wikipedia.org/wiki/$1", "ps" => "https://ps.wikipedia.org/wiki/$1",
        "pt" => "https://pt.wikipedia.org/wiki/$1", "pwn" =>
        "https://pwn.wikipedia.org/wiki/$1", "qu" => "https://qu.wikipedia.org/wiki/$1",
        "rki" => "https://rki.wikipedia.org/wiki/$1", "rm" =>
        "https://rm.wikipedia.org/wiki/$1", "rmy" => "https://rmy.wikipedia.org/wiki/$1",
        "rn" => "https://rn.wikipedia.org/wiki/$1", "ro" =>
        "https://ro.wikipedia.org/wiki/$1", "roa-rup" =>
        "https://roa-rup.wikipedia.org/wiki/$1", "roa-tara" =>
        "https://roa-tara.wikipedia.org/wiki/$1", "rsk" =>
        "https://rsk.wikipedia.org/wiki/$1", "ru" => "https://ru.wikipedia.org/wiki/$1",
        "rue" => "https://rue.wikipedia.org/wiki/$1", "rup" =>
        "https://roa-rup.wikipedia.org/wiki/$1", "rw" =>
        "https://rw.wikipedia.org/wiki/$1", "sa" => "https://sa.wikipedia.org/wiki/$1",
        "sah" => "https://sah.wikipedia.org/wiki/$1", "sat" =>
        "https://sat.wikipedia.org/wiki/$1", "sc" => "https://sc.wikipedia.org/wiki/$1",
        "scn" => "https://scn.wikipedia.org/wiki/$1", "sco" =>
        "https://sco.wikipedia.org/wiki/$1", "sd" => "https://sd.wikipedia.org/wiki/$1",
        "se" => "https://se.wikipedia.org/wiki/$1", "sg" =>
        "https://sg.wikipedia.org/wiki/$1", "sgs" =>
        "https://bat-smg.wikipedia.org/wiki/$1", "sh" =>
        "https://sh.wikipedia.org/wiki/$1", "shi" => "https://shi.wikipedia.org/wiki/$1",
        "shn" => "https://shn.wikipedia.org/wiki/$1", "shy" =>
        "https://shy.wikipedia.org/wiki/$1", "si" => "https://si.wikipedia.org/wiki/$1",
        "simple" => "https://simple.wikipedia.org/wiki/$1", "sk" =>
        "https://sk.wikipedia.org/wiki/$1", "skr" => "https://skr.wikipedia.org/wiki/$1",
        "sl" => "https://sl.wikipedia.org/wiki/$1", "sm" =>
        "https://sm.wikipedia.org/wiki/$1", "smn" => "https://smn.wikipedia.org/wiki/$1",
        "sn" => "https://sn.wikipedia.org/wiki/$1", "so" =>
        "https://so.wikipedia.org/wiki/$1", "sq" => "https://sq.wikipedia.org/wiki/$1",
        "sr" => "https://sr.wikipedia.org/wiki/$1", "srn" =>
        "https://srn.wikipedia.org/wiki/$1", "ss" => "https://ss.wikipedia.org/wiki/$1",
        "st" => "https://st.wikipedia.org/wiki/$1", "stq" =>
        "https://stq.wikipedia.org/wiki/$1", "su" => "https://su.wikipedia.org/wiki/$1",
        "sv" => "https://sv.wikipedia.org/wiki/$1", "sw" =>
        "https://sw.wikipedia.org/wiki/$1", "syl" => "https://syl.wikipedia.org/wiki/$1",
        "szl" => "https://szl.wikipedia.org/wiki/$1", "szy" =>
        "https://szy.wikipedia.org/wiki/$1", "ta" => "https://ta.wikipedia.org/wiki/$1",
        "tay" => "https://tay.wikipedia.org/wiki/$1", "tcy" =>
        "https://tcy.wikipedia.org/wiki/$1", "tdd" =>
        "https://tdd.wikipedia.org/wiki/$1", "te" => "https://te.wikipedia.org/wiki/$1",
        "tet" => "https://tet.wikipedia.org/wiki/$1", "tg" =>
        "https://tg.wikipedia.org/wiki/$1", "th" => "https://th.wikipedia.org/wiki/$1",
        "ti" => "https://ti.wikipedia.org/wiki/$1", "tig" =>
        "https://tig.wikipedia.org/wiki/$1", "tk" => "https://tk.wikipedia.org/wiki/$1",
        "tl" => "https://tl.wikipedia.org/wiki/$1", "tly" =>
        "https://tly.wikipedia.org/wiki/$1", "tn" => "https://tn.wikipedia.org/wiki/$1",
        "to" => "https://to.wikipedia.org/wiki/$1", "tok" =>
        "https://tok.wikipedia.org/wiki/$1", "tpi" =>
        "https://tpi.wikipedia.org/wiki/$1", "tr" => "https://tr.wikipedia.org/wiki/$1",
        "trv" => "https://trv.wikipedia.org/wiki/$1", "ts" =>
        "https://ts.wikipedia.org/wiki/$1", "tt" => "https://tt.wikipedia.org/wiki/$1",
        "tum" => "https://tum.wikipedia.org/wiki/$1", "tw" =>
        "https://tw.wikipedia.org/wiki/$1", "ty" => "https://ty.wikipedia.org/wiki/$1",
        "tyv" => "https://tyv.wikipedia.org/wiki/$1", "udm" =>
        "https://udm.wikipedia.org/wiki/$1", "ug" => "https://ug.wikipedia.org/wiki/$1",
        "uk" => "https://uk.wikipedia.org/wiki/$1", "ur" =>
        "https://ur.wikipedia.org/wiki/$1", "uz" => "https://uz.wikipedia.org/wiki/$1",
        "ve" => "https://ve.wikipedia.org/wiki/$1", "vec" =>
        "https://vec.wikipedia.org/wiki/$1", "vep" =>
        "https://vep.wikipedia.org/wiki/$1", "vi" => "https://vi.wikipedia.org/wiki/$1",
        "vls" => "https://vls.wikipedia.org/wiki/$1", "vo" =>
        "https://vo.wikipedia.org/wiki/$1", "vro" =>
        "https://fiu-vro.wikipedia.org/wiki/$1", "wa" =>
        "https://wa.wikipedia.org/wiki/$1", "war" => "https://war.wikipedia.org/wiki/$1",
        "wo" => "https://wo.wikipedia.org/wiki/$1", "wuu" =>
        "https://wuu.wikipedia.org/wiki/$1", "xal" =>
        "https://xal.wikipedia.org/wiki/$1", "xh" => "https://xh.wikipedia.org/wiki/$1",
        "xmf" => "https://xmf.wikipedia.org/wiki/$1", "yi" =>
        "https://yi.wikipedia.org/wiki/$1", "yo" => "https://yo.wikipedia.org/wiki/$1",
        "yue" => "https://zh-yue.wikipedia.org/wiki/$1", "za" =>
        "https://za.wikipedia.org/wiki/$1", "zea" => "https://zea.wikipedia.org/wiki/$1",
        "zgh" => "https://zgh.wikipedia.org/wiki/$1", "zh" =>
        "https://zh.wikipedia.org/wiki/$1", "zh-classical" =>
        "https://zh-classical.wikipedia.org/wiki/$1", "zh-min-nan" =>
        "https://zh-min-nan.wikipedia.org/wiki/$1", "zh-yue" =>
        "https://zh-yue.wikipedia.org/wiki/$1", "zu" =>
        "https://zu.wikipedia.org/wiki/$1", "cz" => "https://cs.wikipedia.org/wiki/$1",
        "dk" => "https://da.wikipedia.org/wiki/$1", "epo" =>
        "https://eo.wikipedia.org/wiki/$1", "jp" => "https://ja.wikipedia.org/wiki/$1",
        "zh-cn" => "https://zh.wikipedia.org/wiki/$1", "zh-tw" =>
        "https://zh.wikipedia.org/wiki/$1", "cmn" => "https://zh.wikipedia.org/wiki/$1",
        "egl" => "https://eml.wikipedia.org/wiki/$1", "en-simple" =>
        "https://simple.wikipedia.org/wiki/$1", "nb" =>
        "https://no.wikipedia.org/wiki/$1", "w" => "https://en.wikipedia.org/wiki/$1",
        "wikt" => "https://en.wiktionary.org/wiki/$1", "q" =>
        "https://en.wikiquote.org/wiki/$1", "b" => "https://en.wikibooks.org/wiki/$1",
        "n" => "https://en.wikinews.org/wiki/$1", "s" =>
        "https://en.wikisource.org/wiki/$1", "chapter" =>
        "https://en.wikimedia.org/wiki/$1", "v" => "https://en.wikiversity.org/wiki/$1",
        "voy" => "https://en.wikivoyage.org/wiki/$1"
    },
    language_conversion_enabled: true,
    link_trail: "/^([a-z]+)(.*)$/sD",
    magic_links: MagicLinks {
        isbn: false,
        pmid: false,
        rfc: false,
    },
    namespaces: &[
        Namespace {
            id: -1i32,
            name: "Special",
            canonical: Some("Special"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: -2i32,
            name: "Media",
            canonical: Some("Media"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 0i32,
            name: "",
            canonical: None,
            case: FirstLetter,
            content: true,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 1i32,
            name: "Talk",
            canonical: Some("Talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 10i32,
            name: "Template",
            canonical: Some("Template"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["TM"],
        },
        Namespace {
            id: 100i32,
            name: "Portal",
            canonical: Some("Portal"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 101i32,
            name: "Portal talk",
            canonical: Some("Portal talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 11i32,
            name: "Template talk",
            canonical: Some("Template talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 118i32,
            name: "Draft",
            canonical: Some("Draft"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 119i32,
            name: "Draft talk",
            canonical: Some("Draft talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 12i32,
            name: "Help",
            canonical: Some("Help"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 126i32,
            name: "MOS",
            canonical: Some("MOS"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 127i32,
            name: "MOS talk",
            canonical: Some("MOS talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 13i32,
            name: "Help talk",
            canonical: Some("Help talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 14i32,
            name: "Category",
            canonical: Some("Category"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 15i32,
            name: "Category talk",
            canonical: Some("Category talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 1728i32,
            name: "Event",
            canonical: Some("Event"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 1729i32,
            name: "Event talk",
            canonical: Some("Event talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 2i32,
            name: "User",
            canonical: Some("User"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 3i32,
            name: "User talk",
            canonical: Some("User talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 4i32,
            name: "Wikipedia",
            canonical: Some("Project"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["WP"],
        },
        Namespace {
            id: 5i32,
            name: "Wikipedia talk",
            canonical: Some("Project talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["WT"],
        },
        Namespace {
            id: 6i32,
            name: "File",
            canonical: Some("File"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &["Image"],
        },
        Namespace {
            id: 7i32,
            name: "File talk",
            canonical: Some("File talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["Image talk"],
        },
        Namespace {
            id: 710i32,
            name: "TimedText",
            canonical: Some("TimedText"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 711i32,
            name: "TimedText talk",
            canonical: Some("TimedText talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 8i32,
            name: "MediaWiki",
            canonical: Some("MediaWiki"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 828i32,
            name: "Module",
            canonical: Some("Module"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 829i32,
            name: "Module talk",
            canonical: Some("Module talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 9i32,
            name: "MediaWiki talk",
            canonical: Some("MediaWiki talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
    ],
    protocols: phf::phf_set! {
        "bitcoin:", "ftp://", "ftps://", "geo:", "git://", "gopher://", "http://",
        "https://", "irc://", "ircs://", "magnet:", "mailto:", "matrix:", "mms://",
        "news:", "nntp://", "redis://", "sftp://", "sip:", "sips:", "sms:", "ssh://",
        "svn://", "tel:", "telnet://", "urn:", "wikipedia://", "worldwind://", "xmpp:",
        "//"
    },
    redirect_magic_words: phf::phf_set! {
        "#redirect"
    },
    valid_title_bytes: " %!\"$&'()*,\\-.\\/0-9:;=?@A-Z\\\\^_`a-z~\\x80-\\xFF+",
    variables: phf::phf_set! {
        "!", "=", "currentmonth", "currentmonth1", "currentmonthname",
        "currentmonthnamegen", "currentmonthabbrev", "currentday", "currentday2",
        "currentdayname", "currentyear", "currenttime", "currenthour", "localmonth",
        "localmonth1", "localmonthname", "localmonthnamegen", "localmonthabbrev",
        "localday", "localday2", "localdayname", "localyear", "localtime", "localhour",
        "numberofarticles", "numberoffiles", "numberofedits", "articlepath", "pageid",
        "sitename", "server", "servername", "scriptpath", "stylepath", "pagename",
        "pagenamee", "fullpagename", "fullpagenamee", "namespace", "namespacee",
        "namespacenumber", "currentweek", "currentdow", "localweek", "localdow",
        "revisionid", "revisionday", "revisionday2", "revisionmonth", "revisionmonth1",
        "revisionyear", "revisiontimestamp", "revisionuser", "revisionsize",
        "subpagename", "subpagenamee", "talkspace", "talkspacee", "subjectspace",
        "subjectspacee", "talkpagename", "talkpagenamee", "subjectpagename",
        "subjectpagenamee", "numberofusers", "numberofactiveusers", "numberofpages",
        "currentversion", "rootpagename", "rootpagenamee", "basepagename",
        "basepagenamee", "currenttimestamp", "localtimestamp", "directionmark",
        "contentlanguage", "userlanguage", "pagelanguage", "numberofadmins",
        "cascadingsources", "bcp47", "contentmodel", "dir", "language", "numberofwikis",
        "pendingchangelevel", "noexternallanglinks", "wbreponame"
    },
};

/// The installation configuration, suitable for runtime use.
pub static CONFIG: LazyLock<Configuration> = LazyLock::new(|| Configuration::new(&CONFIG_SOURCE));
