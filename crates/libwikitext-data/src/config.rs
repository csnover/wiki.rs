//! MediaWiki configuration.
//!
//! Wikitext documents are not self-encapsulated and cannot be parsed without
//! out-of-band configuration data. Most of this configuration data can be
//! acquired by querying the MediaWiki API for a given MediaWiki installation.

use libwikitext_common::{
    config::{Configuration, ConfigurationSource, MagicLinks, SpecialPages},
    title::{Namespace, NamespaceCase::FirstLetter},
};
use std::sync::LazyLock;

/// The static source configuration for this MW installation.
static CONFIG_SOURCE: ConfigurationSource = ConfigurationSource {
    annotation_tags: phf::phf_set! {},
    annotations_enabled: false,
    behavior_switch_words: phf::phf_map! {
        "__archivedtalk__" => "archivedtalk", "__disambig__" => "disambiguation",
        "__expected_unconnected_page__" => "expectedunconnectedpage",
        "__expectunusedcategory__" => "expectunusedcategory", "__expectunusedtemplate__"
        => "expectunusedtemplate", "__forcetoc__" => "forcetoc", "__hiddencat__" =>
        "hiddencat", "__index__" => "index", "__newsectionlink__" => "newsectionlink",
        "__nocontentconvert__" => "nocontentconvert", "__nocc__" => "nocontentconvert",
        "__noeditsection__" => "noeditsection", "__nogallery__" => "nogallery",
        "__noglobal__" => "noglobal", "__noindex__" => "noindex", "__nonewsectionlink__"
        => "nonewsectionlink", "__notalk__" => "notalk", "__notitleconvert__" =>
        "notitleconvert", "__notc__" => "notitleconvert", "__notoc__" => "notoc",
        "__staticredirect__" => "staticredirect", "__toc__" => "toc"
    },
    extension_tags: phf::phf_set! {
        "categorytree", "ce", "charinsert", "chem", "gallery", "graph", "hiero",
        "imagemap", "indicator", "inputbox", "langconvert", "mapframe", "maplink",
        "math", "nowiki", "page-collection", "phonos", "poem", "pre", "ref",
        "references", "score", "section", "source", "syntaxhighlight", "templatedata",
        "templatestyles", "timeline"
    },
    extra_words: phf::phf_map! {
        "#default" => & ["default",], "$1px" => & ["img_width",],
        "__expectwithoutscans__" => & ["expectwithoutscans",], "__keeptitle__" => &
        ["keeptitle",], "all" => & ["pagesincategory_all",], "alt=$1" => & ["img_alt",],
        "arg:" => & ["chart_arg",], "aya" => & ["a/ya",], "baseline" => &
        ["img_baseline",], "bcp47" => & ["language_option_bcp47",], "border" => &
        ["img_border",], "both" => & ["timef-both",], "bottom" => & ["img_bottom",],
        "calendar" => & ["calendar",], "canonical" => & ["contentmodel_canonical",],
        "center" => & ["img_center",], "centre" => & ["img_center",], "class=$1" => &
        ["img_class",], "communityrequests" => & ["communityrequests",], "count" => &
        ["count",], "data" => & ["chart_data",], "date" => & ["timef-date",], "dd2dms" =>
        & ["dd2dms",], "deg2dd" => & ["deg2dd",], "disablecontrols=$1" => &
        ["timedmedia_disablecontrols",], "e" => & ["e/",], "ega" => & ["e/ga",], "end=$1"
        => & ["timedmedia_endtime",], "enframed" => & ["img_framed",], "eulruel" => &
        ["eul/ruel",], "eunneun" => & ["eun/neun",], "euro" => & ["euro/ro",], "explode"
        => & ["explode",], "files" => & ["pagesincategory_files",], "frame" => &
        ["img_framed",], "framed" => & ["img_framed",], "frameless" => &
        ["img_frameless",], "function" => & ["function",], "geolink" => & ["geolink",],
        "gwawa" => & ["gwa/wa",], "insider" => & ["insider",], "isin" => & ["isin",],
        "lang=$1" => & ["img_lang",], "left" => & ["img_left",], "len" => & ["len",],
        "link=$1" => & ["img_link",], "local" => & ["contentmodel_local",], "loop" => &
        ["timedmedia_loop",], "lossless" => & ["lossless",], "lossy=$1" => &
        ["img_lossy",], "lqtpagelimit" => & ["lqtpagelimit",], "middle" => &
        ["img_middle",], "msg:" => & ["msg",], "msgnw:" => & ["msgnw",], "muted" => &
        ["timedmedia_muted",], "noerror" => & ["defaultsort_noerror",
        "displaytitle_noerror",], "none" => & ["img_none",], "noreplace" => &
        ["defaultsort_noreplace", "shortdesc_noreplace", "displaytitle_noreplace",],
        "nosep" => & ["nocommafysuffix",], "numberoffunctions" => &
        ["magic_count_functions",], "numberofimplementations" => &
        ["magic_count_implementations",], "numberoflanguages" => &
        ["magic_count_languages",], "numberofobjects" => & ["magic_count_all",],
        "numberoftestcases" => & ["magic_count_testers",], "numberoftypes" => &
        ["magic_count_types",], "page $1" => & ["img_page",], "page=$1" => &
        ["img_page",], "pagebanner" => & ["pagebanner",], "pages" => &
        ["pagesincategory_pages",], "pagesinnamespace:" => & ["pagesinnamespace",],
        "pagesinns:" => & ["pagesinnamespace",], "pagesusingpendingchanges" => &
        ["pagesusingpendingchanges",], "path" => & ["url_path",], "phonos" => &
        ["phonos",], "pos" => & ["pos",], "pretty" => & ["timef-pretty",], "primary" => &
        ["primary",], "query" => & ["url_query",], "r" => & ["rawsuffix",], "raw:" => &
        ["raw",], "replace" => & ["replace",], "right" => & ["img_right",], "rpos" => &
        ["rpos",], "safesubst" => & ["safesubst",], "start=$1" => &
        ["timedmedia_starttime",], "sub" => & ["img_sub", "sub",], "subcats" => &
        ["pagesincategory_subcats",], "subst" => & ["subst",], "sup" => &
        ["img_super",], "super" => & ["img_super",], "switchcountry" => &
        ["switchcountry",], "switchlanguage" => & ["switchlanguage",], "text-bottom" => &
        ["img_text_bottom",], "text-top" => & ["img_text_top",], "thumb" => &
        ["img_thumbnail",], "thumb=$1" => & ["img_manualthumb",], "thumbnail" => &
        ["img_thumbnail",], "thumbnail=$1" => & ["img_manualthumb",], "thumbtime=$1" => &
        ["timedmedia_thumbtime",], "time" => & ["timef-time",], "top" => & ["img_top",],
        "translatablepage" => & ["translatablepage",], "translation" => &
        ["translation",], "upright" => & ["img_upright",], "upright $1" => &
        ["img_upright",], "upright=$1" => & ["img_upright",], "urldecode" => &
        ["urldecode",], "useliquidthreads" => & ["useliquidthreads",], "usertestwiki" =>
        & ["usertestwiki",], "wiki" => & ["url_wiki",], "wikifunctionlabel" => &
        ["wikifunctionlabel",], "wikifunctionlabeldesc" => & ["wikifunctionlabeldesc",]
    },
    function_hooks: phf::phf_map! {
        "anchorencode" => "anchorencode", "assessment" => "assessment", "babel" =>
        "babel", "basepagename" => "basepagename", "basepagenamee" => "basepagenamee",
        "bcp47" => "bcp47", "bidi" => "bidi", "canonicalurl" => "canonicalurl",
        "canonicalurle" => "canonicalurle", "cascadingsources" => "cascadingsources",
        "categorytree" => "categorytree", "chart" => "chart", "commaseparatedlist" =>
        "commaseparatedlist", "contentmodel" => "contentmodel", "coordinates" =>
        "coordinates", "defaultsort" => "defaultsort", "defaultsortkey" => "defaultsort",
        "defaultcategorysort" => "defaultsort", "dir" => "dir", "displaytitle" =>
        "displaytitle", "expr" => "expr", "filepath" => "filepath", "formal" => "formal",
        "formatdate" => "formatdate", "dateformat" => "formatdate", "formatnum" =>
        "formatnum", "fullpagename" => "fullpagename", "fullpagenamee" =>
        "fullpagenamee", "fullurl" => "fullurl", "fullurle" => "fullurle", "gender" =>
        "gender", "grammar" => "grammar", "if" => "if", "ifeq" => "ifeq", "iferror" =>
        "iferror", "ifexist" => "ifexist", "ifexpr" => "ifexpr", "int" => "int",
        "interlanguagelink" => "interlanguagelink", "interwikilink" => "interwikilink",
        "invoke" => "invoke", "language" => "language", "lc" => "lc", "lcfirst" =>
        "lcfirst", "localurl" => "localurl", "localurle" => "localurle", "lst" => "lst",
        "section" => "lst", "lsth" => "lsth", "section-h" => "lsth", "lstx" => "lstx",
        "section-x" => "lstx", "mentor" => "mentor", "namespace" => "namespace",
        "namespacee" => "namespacee", "namespacenumber" => "namespacenumber",
        "noexternallanglinks" => "noexternallanglinks", "ns" => "ns", "nse" => "nse",
        "numberingroup" => "numberingroup", "numingroup" => "numberingroup",
        "numberofactiveusers" => "numberofactiveusers", "numberofadmins" =>
        "numberofadmins", "numberofarticles" => "numberofarticles", "numberofedits" =>
        "numberofedits", "numberoffiles" => "numberoffiles", "numberofpages" =>
        "numberofpages", "numberofusers" => "numberofusers", "padleft" => "padleft",
        "padright" => "padright", "pageid" => "pageid", "pagename" => "pagename",
        "pagenamee" => "pagenamee", "pagesincategory" => "pagesincategory", "pagesincat"
        => "pagesincategory", "pagesize" => "pagesize", "pendingchangelevel" =>
        "pendingchangelevel", "plural" => "plural", "property" => "property",
        "protectionexpiry" => "protectionexpiry", "protectionlevel" => "protectionlevel",
        "rel2abs" => "rel2abs", "related" => "related", "revisionday" => "revisionday",
        "revisionday2" => "revisionday2", "revisionid" => "revisionid", "revisionmonth"
        => "revisionmonth", "revisionmonth1" => "revisionmonth1", "revisiontimestamp" =>
        "revisiontimestamp", "revisionuser" => "revisionuser", "revisionyear" =>
        "revisionyear", "rootpagename" => "rootpagename", "rootpagenamee" =>
        "rootpagenamee", "shortdesc" => "shortdesc", "special" => "special", "speciale"
        => "speciale", "statements" => "statements", "subjectpagename" =>
        "subjectpagename", "articlepagename" => "subjectpagename", "subjectpagenamee" =>
        "subjectpagenamee", "articlepagenamee" => "subjectpagenamee", "subjectspace" =>
        "subjectspace", "articlespace" => "subjectspace", "subjectspacee" =>
        "subjectspacee", "articlespacee" => "subjectspacee", "subpagename" =>
        "subpagename", "subpagenamee" => "subpagenamee", "switch" => "switch", "tag" =>
        "tag", "talkpagename" => "talkpagename", "talkpagenamee" => "talkpagenamee",
        "talkspace" => "talkspace", "talkspacee" => "talkspacee", "target" => "target",
        "time" => "time", "timef" => "timef", "timefl" => "timefl", "timel" => "timel",
        "titleparts" => "titleparts", "uc" => "uc", "ucfirst" => "ucfirst", "urlencode"
        => "urlencode"
    },
    interlanguage_map: phf::phf_map! {
        "aa" => "aa", "ab" => "ab", "ace" => "ace", "ady" => "ady", "af" => "af", "ak" =>
        "ak", "als" => "gsw", "alt" => "alt", "am" => "am", "ami" => "ami", "an" => "an",
        "ang" => "ang", "ann" => "ann", "anp" => "anp", "ar" => "ar", "arc" => "arc",
        "ary" => "ary", "arz" => "arz", "as" => "as", "ast" => "ast", "atj" => "atj",
        "av" => "av", "avk" => "avk", "awa" => "awa", "ay" => "ay", "az" => "az", "azb"
        => "azb", "ba" => "ba", "ban" => "ban", "bar" => "bar", "bat-smg" => "sgs", "bbc"
        => "bbc", "bcl" => "bcl", "bdr" => "bdr", "be" => "be", "be-tarask" =>
        "be-tarask", "be-x-old" => "be-tarask", "bew" => "bew", "bg" => "bg", "bh" =>
        "bh", "bi" => "bi", "bjn" => "bjn", "blk" => "blk", "bm" => "bm", "bn" => "bn",
        "bo" => "bo", "bpy" => "bpy", "br" => "br", "bs" => "bs", "btm" => "btm", "bug"
        => "bug", "bxr" => "bxr", "ca" => "ca", "cbk-zam" => "cbk", "cdo" => "cdo", "ce"
        => "ce", "ceb" => "ceb", "ch" => "ch", "cho" => "cho", "chr" => "chr", "chy" =>
        "chy", "ckb" => "ckb", "co" => "co", "cr" => "cr", "crh" => "crh", "cs" => "cs",
        "csb" => "csb", "cu" => "cu", "cv" => "cv", "cy" => "cy", "da" => "da", "dag" =>
        "dag", "de" => "de", "dga" => "dga", "din" => "din", "diq" => "diq", "dsb" =>
        "dsb", "dtp" => "dtp", "dty" => "dty", "dv" => "dv", "dz" => "dz", "ee" => "ee",
        "el" => "el", "eml" => "egl", "en" => "en", "eo" => "eo", "es" => "es", "et" =>
        "et", "eu" => "eu", "ext" => "ext", "fa" => "fa", "fat" => "fat", "ff" => "ff",
        "fi" => "fi", "fiu-vro" => "vro", "fj" => "fj", "fo" => "fo", "fon" => "fon",
        "fr" => "fr", "frp" => "frp", "frr" => "frr", "fur" => "fur", "fy" => "fy", "ga"
        => "ga", "gag" => "gag", "gan" => "gan", "gcr" => "gcr", "gd" => "gd", "gl" =>
        "gl", "glk" => "glk", "gn" => "gn", "gom" => "gom", "gor" => "gor", "got" =>
        "got", "gpe" => "gpe", "gsw" => "gsw", "gu" => "gu", "guc" => "guc", "gur" =>
        "gur", "guw" => "guw", "gv" => "gv", "ha" => "ha", "hak" => "hak", "haw" =>
        "haw", "he" => "he", "hi" => "hi", "hif" => "hif", "ho" => "ho", "hr" => "hr",
        "hsb" => "hsb", "ht" => "ht", "hu" => "hu", "hy" => "hy", "hyw" => "hyw", "hz" =>
        "hz", "ia" => "ia", "iba" => "iba", "id" => "id", "ie" => "ie", "ig" => "ig",
        "igl" => "igl", "ii" => "ii", "ik" => "ik", "ilo" => "ilo", "inh" => "inh", "io"
        => "io", "is" => "is", "it" => "it", "iu" => "iu", "ja" => "ja", "jam" => "jam",
        "jbo" => "jbo", "jv" => "jv", "ka" => "ka", "kaa" => "kaa", "kab" => "kab", "kai"
        => "kai", "kaj" => "kaj", "kbd" => "kbd", "kbp" => "kbp", "kcg" => "kcg", "kg" =>
        "kg", "kge" => "kge", "ki" => "ki", "kj" => "kj", "kk" => "kk", "kl" => "kl",
        "km" => "km", "kn" => "kn", "knc" => "knc", "ko" => "ko", "koi" => "koi", "kr" =>
        "kr", "krc" => "krc", "ks" => "ks", "ksh" => "ksh", "ku" => "ku", "kus" => "kus",
        "kv" => "kv", "kw" => "kw", "ky" => "ky", "la" => "la", "lad" => "lad", "lb" =>
        "lb", "lbe" => "lbe", "lez" => "lez", "lfn" => "lfn", "lg" => "lg", "li" => "li",
        "lij" => "lij", "lld" => "lld", "lmo" => "lmo", "ln" => "ln", "lo" => "lo", "lrc"
        => "lrc", "lt" => "lt", "ltg" => "ltg", "lv" => "lv", "lzh" => "lzh", "mad" =>
        "mad", "mai" => "mai", "map-bms" => "jv-x-bms", "mdf" => "mdf", "mg" => "mg",
        "mh" => "mh", "mhr" => "mhr", "mi" => "mi", "min" => "min", "mk" => "mk", "ml" =>
        "ml", "mn" => "mn", "mni" => "mni", "mnw" => "mnw", "mo" => "ro-Cyrl-MD", "mos"
        => "mos", "mr" => "mr", "mrj" => "mrj", "ms" => "ms", "mt" => "mt", "mus" =>
        "mus", "mwl" => "mwl", "my" => "my", "myv" => "myv", "mzn" => "mzn", "na" =>
        "na", "nah" => "nah", "nan" => "nan", "nap" => "nap", "nds" => "nds", "nds-nl" =>
        "nds-NL", "ne" => "ne", "new" => "new", "ng" => "ng", "nia" => "nia", "nl" =>
        "nl", "nn" => "nn", "no" => "no", "nov" => "nov", "nqo" => "nqo", "nr" => "nr",
        "nrm" => "nrf", "nso" => "nso", "nup" => "nup", "nv" => "nv", "ny" => "ny", "oc"
        => "oc", "olo" => "olo", "om" => "om", "or" => "or", "os" => "os", "pa" => "pa",
        "pag" => "pag", "pam" => "pam", "pap" => "pap", "pcd" => "pcd", "pcm" => "pcm",
        "pdc" => "pdc", "pfl" => "pfl", "pi" => "pi", "pih" => "pih", "pl" => "pl", "pms"
        => "pms", "pnb" => "pnb", "pnt" => "pnt", "ppl" => "ppl", "ps" => "ps", "pt" =>
        "pt", "pwn" => "pwn", "qu" => "qu", "rki" => "rki", "rm" => "rm", "rmy" => "rmy",
        "rn" => "rn", "ro" => "ro", "roa-rup" => "rup", "roa-tara" => "nap-x-tara", "rsk"
        => "rsk", "ru" => "ru", "rue" => "rue", "rup" => "rup", "rw" => "rw", "sa" =>
        "sa", "sah" => "sah", "sat" => "sat", "sc" => "sc", "scn" => "scn", "sco" =>
        "sco", "sd" => "sd", "se" => "se", "sg" => "sg", "sgs" => "sgs", "sh" => "sh",
        "shi" => "shi", "shn" => "shn", "shy" => "shy", "si" => "si", "simple" =>
        "en-simple", "sk" => "sk", "skr" => "skr", "sl" => "sl", "sm" => "sm", "smn" =>
        "smn", "sn" => "sn", "so" => "so", "sq" => "sq", "sr" => "sr", "srn" => "srn",
        "ss" => "ss", "st" => "st", "stq" => "stq", "su" => "su", "sv" => "sv", "sw" =>
        "sw", "syl" => "syl", "szl" => "szl", "szy" => "szy", "ta" => "ta", "tay" =>
        "tay", "tcy" => "tcy", "tdd" => "tdd", "te" => "te", "tet" => "tet", "tg" =>
        "tg", "th" => "th", "ti" => "ti", "tig" => "tig", "tk" => "tk", "tl" => "tl",
        "tly" => "tly", "tn" => "tn", "to" => "to", "tok" => "tok", "tpi" => "tpi", "tr"
        => "tr", "trv" => "trv", "ts" => "ts", "tt" => "tt", "tum" => "tum", "tw" =>
        "tw", "ty" => "ty", "tyv" => "tyv", "udm" => "udm", "ug" => "ug", "uk" => "uk",
        "ur" => "ur", "uz" => "uz", "ve" => "ve", "vec" => "vec", "vep" => "vep", "vi" =>
        "vi", "vls" => "vls", "vo" => "vo", "vro" => "vro", "wa" => "wa", "war" => "war",
        "wo" => "wo", "wuu" => "wuu", "xal" => "xal", "xh" => "xh", "xmf" => "xmf", "yi"
        => "yi", "yo" => "yo", "yue" => "yue", "za" => "za", "zea" => "zea", "zgh" =>
        "zgh", "zh" => "zh", "zh-classical" => "lzh", "zh-min-nan" => "nan", "zh-yue" =>
        "yue", "zu" => "zu", "zh-cn" => "zh-Hans-CN", "zh-tw" => "zh-Hant-TW", "egl" =>
        "egl", "nb" => "nb"
    },
    interwiki_map: phf::phf_map! {
        "acc" => "https://accounts.wmflabs.org/internal.php/viewRequest?id=$1", "acronym"
        => "https://www.acronymfinder.com/$1.html", "advisory" =>
        "https://advisory.wikimedia.org/wiki/$1", "antwiki" =>
        "https://antwiki.org/wiki/$1", "appropedia" => "https://www.appropedia.org/$1",
        "aquariumwiki" =>
        "https://meta.wikimedia.org/wiki/Interwiki_map/discontinued#AquariumWiki",
        "arborwiki" => "https://localwiki.org/ann-arbor/$1", "arbcom-zh" =>
        "https://wikipedia-zh-arbcom.wikimedia.org/wiki/$1", "arxiv" =>
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
        "https://sourceforge.net/$1", "spamcheck" =>
        "https://spamcheck.toolforge.org/by-domain?q=$1", "spcom" =>
        "https://spcom.wikimedia.org/wiki/$1", "species" =>
        "https://species.wikimedia.org/wiki/$1", "stats" =>
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
        "https://kab.wikipedia.org/wiki/$1", "kaj" =>
        "https://kaj.wikipedia.org/wiki/$1", "kbd" =>
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
    language: "en",
    language_bcp47: phf::phf_map! {
        "aa" => 0, "aae" => 1, "ab" => 2, "abr" => 3, "abs" => 4, "ace" => 5, "acf" => 6,
        "acm" => 7, "ady" => 8, "ady-Cyrl" => 9, "aeb" => 10, "aeb-Arab" => 11,
        "aeb-Latn" => 12, "af" => 13, "aig" => 14, "ak" => 15, "aln" => 16, "gsw" => 17,
        "alt" => 18, "am" => 19, "ami" => 20, "an" => 21, "ang" => 22, "ann" => 23, "anp"
        => 24, "apc" => 25, "ar" => 26, "arc" => 27, "arn" => 28, "arq" => 29, "ary" =>
        30, "arz" => 31, "as" => 32, "ase" => 33, "ast" => 34, "atj" => 35, "av" => 36,
        "avk" => 37, "awa" => 38, "ay" => 39, "az" => 40, "azb" => 41, "ba" => 42, "ban"
        => 43, "ban-Bali" => 44, "bar" => 45, "sgs" => 46, "bbc" => 47, "bbc-Latn" => 48,
        "bcc" => 49, "bci" => 50, "bcl" => 51, "bdr" => 52, "be" => 53, "be-tarask" =>
        54, "bew" => 56, "bg" => 57, "bgc" => 58, "bgn" => 59, "bh" => 60, "bho" => 61,
        "bi" => 62, "bjn" => 63, "blk" => 64, "bm" => 65, "bn" => 66, "bo" => 67, "bol"
        => 68, "bpy" => 69, "bqi" => 70, "br" => 71, "brh" => 72, "bs" => 73, "btm" =>
        74, "bto" => 75, "bug" => 76, "bug-Bugi" => 77, "bxr" => 78, "ca" => 79, "cbk" =>
        80, "ccp" => 81, "cdo" => 82, "cdo-Hant" => 83, "cdo-Latn" => 84, "ce" => 85,
        "ceb" => 86, "ch" => 87, "chn" => 88, "cho" => 89, "chr" => 90, "chy" => 91,
        "ckb" => 92, "co" => 93, "cop" => 94, "cps" => 95, "cpx" => 96, "cpx-Hans" => 97,
        "cpx-Hant" => 98, "cpx-Latn" => 99, "cr" => 100, "crh" => 101, "crh-Cyrl" => 102,
        "crh-Latn" => 103, "crh-Latn-RO" => 104, "cs" => 105, "csb" => 106, "cu" => 107,
        "cv" => 108, "cy" => 109, "da" => 110, "dag" => 111, "de" => 112, "de-AT" => 113,
        "de-CH" => 114, "de-x-formal" => 115, "dga" => 116, "din" => 117, "diq" => 118,
        "dlg" => 119, "dsb" => 120, "dtp" => 121, "dty" => 122, "dua" => 123, "dv" =>
        124, "dz" => 125, "ee" => 126, "efi" => 127, "egl" => 128, "el" => 129, "en" =>
        131, "en-CA" => 132, "en-GB" => 133, "eo" => 134, "es" => 135, "es-419" => 136,
        "es-x-formal" => 137, "et" => 138, "eu" => 139, "ext" => 140, "fa" => 141, "fat"
        => 142, "ff" => 143, "fi" => 144, "fit" => 145, "vro" => 146, "fj" => 147, "fo"
        => 148, "fon" => 149, "fr" => 150, "frc" => 151, "frp" => 152, "frr" => 153,
        "frs" => 154, "fur" => 155, "fvr" => 156, "fy" => 157, "ga" => 158, "gaa" => 159,
        "gag" => 160, "gan" => 161, "gan-Hans" => 162, "gan-Hant" => 163, "gcf" => 164,
        "gcr" => 165, "gd" => 166, "gl" => 167, "gld" => 168, "glk" => 169, "gn" => 170,
        "gom" => 171, "gom-Deva" => 172, "gom-Latn" => 173, "gor" => 174, "got" => 175,
        "gpe" => 176, "grc" => 177, "gu" => 179, "guc" => 180, "gur" => 181, "guw" =>
        182, "gv" => 183, "ha" => 184, "hak" => 185, "hak-Hans" => 186, "hak-Hant" =>
        187, "hak-Latn" => 188, "haw" => 189, "he" => 190, "hi" => 191, "hif" => 192,
        "hif-Latn" => 193, "hil" => 194, "hke" => 195, "hno" => 196, "ho" => 197,
        "hoc-Latn" => 198, "hr" => 199, "hrx" => 200, "hsb" => 201, "hsn" => 202, "ht" =>
        203, "hu" => 204, "hu-x-formal" => 205, "hy" => 206, "hyw" => 207, "hz" => 208,
        "ia" => 209, "iba" => 210, "ibb" => 211, "id" => 212, "ie" => 213, "ig" => 214,
        "igl" => 215, "ii" => 216, "ik" => 217, "ike-Cans" => 218, "ike-Latn" => 219,
        "ilo" => 220, "inh" => 221, "io" => 222, "is" => 223, "isv" => 224, "isv-Cyrl" =>
        225, "isv-Latn" => 226, "it" => 227, "iu" => 228, "ja" => 229, "jam" => 230,
        "jbo" => 231, "jut" => 232, "jv" => 233, "jv-Java" => 234, "ka" => 235, "kaa" =>
        236, "kab" => 237, "kai" => 238, "kaj" => 239, "kbd" => 240, "kbd-Cyrl" => 241,
        "kbp" => 242, "kcg" => 243, "kea" => 244, "kg" => 245, "kge" => 246, "khw" =>
        247, "ki" => 248, "kiu" => 249, "kj" => 250, "kjh" => 251, "kjp" => 252, "kk" =>
        253, "kk-Arab" => 254, "kk-Arab-CN" => 255, "kk-Cyrl" => 256, "kk-KZ" => 257,
        "kk-Latn" => 258, "kk-Latn-TR" => 259, "kl" => 260, "km" => 261, "kn" => 262,
        "knc" => 263, "ko" => 264, "ko-KP" => 265, "koi" => 266, "kr" => 267, "krc" =>
        268, "kri" => 269, "krj" => 270, "krl" => 271, "ks" => 272, "ks-Arab" => 273,
        "ks-Deva" => 274, "ksh" => 275, "ksw" => 276, "ku" => 277, "ku-Arab" => 278,
        "ku-Latn" => 279, "kum" => 280, "kus" => 281, "kv" => 282, "kw" => 283, "ky" =>
        284, "la" => 285, "lad" => 286, "lb" => 287, "lbe" => 288, "lez" => 289, "lfn" =>
        290, "lg" => 291, "li" => 292, "lij" => 293, "liv" => 294, "ljp" => 295, "lki" =>
        296, "lkt" => 297, "lld" => 298, "lmo" => 299, "ln" => 300, "lo" => 301, "loz" =>
        302, "lrc" => 303, "lt" => 304, "ltg" => 305, "lua" => 306, "lus" => 307, "luz"
        => 308, "lv" => 309, "lzh" => 310, "lzz" => 311, "mad" => 312, "mag" => 313,
        "mai" => 314, "jv-x-bms" => 315, "mdf" => 316, "mg" => 317, "mh" => 318, "mhr" =>
        319, "mi" => 320, "min" => 321, "mk" => 322, "ml" => 323, "mn" => 324, "mnc" =>
        325, "mnc-Latn" => 326, "mnc-Mong" => 327, "mni" => 328, "mnw" => 329,
        "ro-Cyrl-MD" => 330, "mos" => 331, "mr" => 332, "mrh" => 333, "mrj" => 334, "ms"
        => 335, "ms-Arab" => 336, "mt" => 337, "mui" => 338, "mus" => 339, "mwl" => 340,
        "my" => 341, "myv" => 342, "mzn" => 343, "na" => 344, "nah" => 345, "nan" => 346,
        "nan-Hant" => 347, "nan-Latn-pehoeji" => 348, "nan-Latn-tailo" => 349, "nap" =>
        350, "nb" => 351, "nds" => 352, "nds-NL" => 353, "ne" => 354, "new" => 355, "ng"
        => 356, "nia" => 357, "nit" => 358, "niu" => 359, "nl" => 360, "nl-x-informal" =>
        361, "nmz" => 362, "nn" => 363, "no" => 364, "nod" => 365, "nog" => 366, "nov" =>
        367, "nqo" => 368, "nr" => 369, "nrf" => 370, "nso" => 371, "nup" => 372, "nv" =>
        373, "ny" => 374, "nyn" => 375, "nyo" => 376, "nys" => 377, "oc" => 378, "ojb" =>
        379, "olo" => 380, "om" => 381, "or" => 382, "os" => 383, "pa" => 384, "pag" =>
        385, "pam" => 386, "pap" => 387, "pap-AW" => 388, "pcd" => 389, "pcm" => 390,
        "pdc" => 391, "pdt" => 392, "pfl" => 393, "pi" => 394, "pih" => 395, "pl" => 396,
        "pms" => 397, "pnb" => 398, "pnt" => 399, "ppl" => 400, "prg" => 401, "ps" =>
        402, "pt" => 403, "pt-BR" => 404, "pwn" => 405, "qu" => 406, "qug" => 407, "rgn"
        => 408, "rif" => 409, "rki" => 410, "rm" => 411, "rmc" => 412, "rmy" => 413, "rn"
        => 414, "ro" => 415, "rup" => 416, "nap-x-tara" => 417, "rsk" => 418, "ru" =>
        419, "rue" => 420, "ruq" => 422, "ruq-Cyrl" => 423, "ruq-Latn" => 424, "rut" =>
        425, "rw" => 426, "ryu" => 427, "sa" => 428, "sah" => 429, "sas" => 430, "sat" =>
        431, "sc" => 432, "scn" => 433, "sco" => 434, "sd" => 435, "sdc" => 436, "sdh" =>
        437, "se" => 438, "se-FI" => 439, "se-NO" => 440, "se-SE" => 441, "sei" => 442,
        "ses" => 443, "sg" => 444, "sh" => 446, "sh-Cyrl" => 447, "sh-Latn" => 448, "shi"
        => 449, "shi-Latn" => 450, "shi-Tfng" => 451, "shn" => 452, "shy" => 453,
        "shy-Latn" => 454, "si" => 455, "en-simple" => 456, "sjd" => 457, "sje" => 458,
        "sk" => 459, "skr" => 460, "skr-Arab" => 461, "sl" => 462, "sli" => 463, "sm" =>
        464, "sma" => 465, "smn" => 466, "sms" => 467, "sn" => 468, "so" => 469, "sq" =>
        470, "sr" => 471, "sr-Cyrl" => 472, "sr-Latn" => 473, "srn" => 474, "sro" => 475,
        "ss" => 476, "st" => 477, "stq" => 478, "sty" => 479, "su" => 480, "sv" => 481,
        "sw" => 482, "syl" => 483, "szl" => 484, "szy" => 485, "ta" => 486, "tay" => 487,
        "tcy" => 488, "tdd" => 489, "te" => 490, "tet" => 491, "tg" => 492, "tg-Cyrl" =>
        493, "tg-Latn" => 494, "th" => 495, "ti" => 496, "tig" => 497, "tk" => 498, "tl"
        => 499, "tly" => 500, "tly-Cyrl" => 501, "tn" => 502, "to" => 503, "tok" => 504,
        "tpi" => 505, "tr" => 506, "tru" => 507, "trv" => 508, "ts" => 509, "tt" => 510,
        "tt-Cyrl" => 511, "tt-Latn" => 512, "ttj" => 513, "tum" => 514, "tw" => 515, "ty"
        => 516, "tyv" => 517, "tzm" => 518, "udm" => 519, "ug" => 520, "ug-Arab" => 521,
        "ug-Latn" => 522, "uk" => 523, "ur" => 524, "uz" => 525, "uz-Cyrl" => 526,
        "uz-Latn" => 527, "ve" => 528, "vec" => 529, "vep" => 530, "vi" => 531, "vls" =>
        532, "vmf" => 533, "vmw" => 534, "vo" => 535, "vot" => 536, "wa" => 538, "wal" =>
        539, "war" => 540, "wls" => 541, "wlx" => 542, "wo" => 543, "wuu" => 544,
        "wuu-Hans" => 545, "wuu-Hant" => 546, "xal" => 547, "xh" => 548, "xmf" => 549,
        "xsy" => 550, "yi" => 551, "yo" => 552, "yrl" => 553, "yua" => 554, "yue" => 555,
        "yue-Hans" => 556, "yue-Hant" => 557, "za" => 558, "zea" => 559, "zgh" => 560,
        "zgh-Latn" => 561, "zh" => 562, "zh-Hans-CN" => 564, "zh-Hans" => 565, "zh-Hant"
        => 566, "zh-Hant-HK" => 567, "zh-Hant-MO" => 569, "zh-Hans-MY" => 570,
        "zh-Hans-SG" => 571, "zh-Hant-TW" => 572, "zu" => 574
    },
    language_code: phf::phf_map! {
        "aa" => 0, "aae" => 1, "ab" => 2, "abr" => 3, "abs" => 4, "ace" => 5, "acf" => 6,
        "acm" => 7, "ady" => 8, "ady-cyrl" => 9, "aeb" => 10, "aeb-arab" => 11,
        "aeb-latn" => 12, "af" => 13, "aig" => 14, "ak" => 15, "aln" => 16, "als" => 17,
        "alt" => 18, "am" => 19, "ami" => 20, "an" => 21, "ang" => 22, "ann" => 23, "anp"
        => 24, "apc" => 25, "ar" => 26, "arc" => 27, "arn" => 28, "arq" => 29, "ary" =>
        30, "arz" => 31, "as" => 32, "ase" => 33, "ast" => 34, "atj" => 35, "av" => 36,
        "avk" => 37, "awa" => 38, "ay" => 39, "az" => 40, "azb" => 41, "ba" => 42, "ban"
        => 43, "ban-bali" => 44, "bar" => 45, "bat-smg" => 46, "bbc" => 47, "bbc-latn" =>
        48, "bcc" => 49, "bci" => 50, "bcl" => 51, "bdr" => 52, "be" => 53, "be-tarask"
        => 54, "be-x-old" => 55, "bew" => 56, "bg" => 57, "bgc" => 58, "bgn" => 59, "bh"
        => 60, "bho" => 61, "bi" => 62, "bjn" => 63, "blk" => 64, "bm" => 65, "bn" => 66,
        "bo" => 67, "bol" => 68, "bpy" => 69, "bqi" => 70, "br" => 71, "brh" => 72, "bs"
        => 73, "btm" => 74, "bto" => 75, "bug" => 76, "bug-bugi" => 77, "bxr" => 78, "ca"
        => 79, "cbk-zam" => 80, "ccp" => 81, "cdo" => 82, "cdo-hant" => 83, "cdo-latn" =>
        84, "ce" => 85, "ceb" => 86, "ch" => 87, "chn" => 88, "cho" => 89, "chr" => 90,
        "chy" => 91, "ckb" => 92, "co" => 93, "cop" => 94, "cps" => 95, "cpx" => 96,
        "cpx-hans" => 97, "cpx-hant" => 98, "cpx-latn" => 99, "cr" => 100, "crh" => 101,
        "crh-cyrl" => 102, "crh-latn" => 103, "crh-ro" => 104, "cs" => 105, "csb" => 106,
        "cu" => 107, "cv" => 108, "cy" => 109, "da" => 110, "dag" => 111, "de" => 112,
        "de-at" => 113, "de-ch" => 114, "de-formal" => 115, "dga" => 116, "din" => 117,
        "diq" => 118, "dlg" => 119, "dsb" => 120, "dtp" => 121, "dty" => 122, "dua" =>
        123, "dv" => 124, "dz" => 125, "ee" => 126, "efi" => 127, "egl" => 128, "el" =>
        129, "eml" => 130, "en" => 131, "en-ca" => 132, "en-gb" => 133, "eo" => 134, "es"
        => 135, "es-419" => 136, "es-formal" => 137, "et" => 138, "eu" => 139, "ext" =>
        140, "fa" => 141, "fat" => 142, "ff" => 143, "fi" => 144, "fit" => 145, "fiu-vro"
        => 146, "fj" => 147, "fo" => 148, "fon" => 149, "fr" => 150, "frc" => 151, "frp"
        => 152, "frr" => 153, "frs" => 154, "fur" => 155, "fvr" => 156, "fy" => 157, "ga"
        => 158, "gaa" => 159, "gag" => 160, "gan" => 161, "gan-hans" => 162, "gan-hant"
        => 163, "gcf" => 164, "gcr" => 165, "gd" => 166, "gl" => 167, "gld" => 168, "glk"
        => 169, "gn" => 170, "gom" => 171, "gom-deva" => 172, "gom-latn" => 173, "gor" =>
        174, "got" => 175, "gpe" => 176, "grc" => 177, "gsw" => 178, "gu" => 179, "guc"
        => 180, "gur" => 181, "guw" => 182, "gv" => 183, "ha" => 184, "hak" => 185,
        "hak-hans" => 186, "hak-hant" => 187, "hak-latn" => 188, "haw" => 189, "he" =>
        190, "hi" => 191, "hif" => 192, "hif-latn" => 193, "hil" => 194, "hke" => 195,
        "hno" => 196, "ho" => 197, "hoc-latn" => 198, "hr" => 199, "hrx" => 200, "hsb" =>
        201, "hsn" => 202, "ht" => 203, "hu" => 204, "hu-formal" => 205, "hy" => 206,
        "hyw" => 207, "hz" => 208, "ia" => 209, "iba" => 210, "ibb" => 211, "id" => 212,
        "ie" => 213, "ig" => 214, "igl" => 215, "ii" => 216, "ik" => 217, "ike-cans" =>
        218, "ike-latn" => 219, "ilo" => 220, "inh" => 221, "io" => 222, "is" => 223,
        "isv" => 224, "isv-cyrl" => 225, "isv-latn" => 226, "it" => 227, "iu" => 228,
        "ja" => 229, "jam" => 230, "jbo" => 231, "jut" => 232, "jv" => 233, "jv-java" =>
        234, "ka" => 235, "kaa" => 236, "kab" => 237, "kai" => 238, "kaj" => 239, "kbd"
        => 240, "kbd-cyrl" => 241, "kbp" => 242, "kcg" => 243, "kea" => 244, "kg" => 245,
        "kge" => 246, "khw" => 247, "ki" => 248, "kiu" => 249, "kj" => 250, "kjh" => 251,
        "kjp" => 252, "kk" => 253, "kk-arab" => 254, "kk-cn" => 255, "kk-cyrl" => 256,
        "kk-kz" => 257, "kk-latn" => 258, "kk-tr" => 259, "kl" => 260, "km" => 261, "kn"
        => 262, "knc" => 263, "ko" => 264, "ko-kp" => 265, "koi" => 266, "kr" => 267,
        "krc" => 268, "kri" => 269, "krj" => 270, "krl" => 271, "ks" => 272, "ks-arab" =>
        273, "ks-deva" => 274, "ksh" => 275, "ksw" => 276, "ku" => 277, "ku-arab" => 278,
        "ku-latn" => 279, "kum" => 280, "kus" => 281, "kv" => 282, "kw" => 283, "ky" =>
        284, "la" => 285, "lad" => 286, "lb" => 287, "lbe" => 288, "lez" => 289, "lfn" =>
        290, "lg" => 291, "li" => 292, "lij" => 293, "liv" => 294, "ljp" => 295, "lki" =>
        296, "lkt" => 297, "lld" => 298, "lmo" => 299, "ln" => 300, "lo" => 301, "loz" =>
        302, "lrc" => 303, "lt" => 304, "ltg" => 305, "lua" => 306, "lus" => 307, "luz"
        => 308, "lv" => 309, "lzh" => 310, "lzz" => 311, "mad" => 312, "mag" => 313,
        "mai" => 314, "map-bms" => 315, "mdf" => 316, "mg" => 317, "mh" => 318, "mhr" =>
        319, "mi" => 320, "min" => 321, "mk" => 322, "ml" => 323, "mn" => 324, "mnc" =>
        325, "mnc-latn" => 326, "mnc-mong" => 327, "mni" => 328, "mnw" => 329, "mo" =>
        330, "mos" => 331, "mr" => 332, "mrh" => 333, "mrj" => 334, "ms" => 335,
        "ms-arab" => 336, "mt" => 337, "mui" => 338, "mus" => 339, "mwl" => 340, "my" =>
        341, "myv" => 342, "mzn" => 343, "na" => 344, "nah" => 345, "nan" => 346,
        "nan-hant" => 347, "nan-latn-pehoeji" => 348, "nan-latn-tailo" => 349, "nap" =>
        350, "nb" => 351, "nds" => 352, "nds-nl" => 353, "ne" => 354, "new" => 355, "ng"
        => 356, "nia" => 357, "nit" => 358, "niu" => 359, "nl" => 360, "nl-informal" =>
        361, "nmz" => 362, "nn" => 363, "no" => 364, "nod" => 365, "nog" => 366, "nov" =>
        367, "nqo" => 368, "nr" => 369, "nrm" => 370, "nso" => 371, "nup" => 372, "nv" =>
        373, "ny" => 374, "nyn" => 375, "nyo" => 376, "nys" => 377, "oc" => 378, "ojb" =>
        379, "olo" => 380, "om" => 381, "or" => 382, "os" => 383, "pa" => 384, "pag" =>
        385, "pam" => 386, "pap" => 387, "pap-aw" => 388, "pcd" => 389, "pcm" => 390,
        "pdc" => 391, "pdt" => 392, "pfl" => 393, "pi" => 394, "pih" => 395, "pl" => 396,
        "pms" => 397, "pnb" => 398, "pnt" => 399, "ppl" => 400, "prg" => 401, "ps" =>
        402, "pt" => 403, "pt-br" => 404, "pwn" => 405, "qu" => 406, "qug" => 407, "rgn"
        => 408, "rif" => 409, "rki" => 410, "rm" => 411, "rmc" => 412, "rmy" => 413, "rn"
        => 414, "ro" => 415, "roa-rup" => 416, "roa-tara" => 417, "rsk" => 418, "ru" =>
        419, "rue" => 420, "rup" => 421, "ruq" => 422, "ruq-cyrl" => 423, "ruq-latn" =>
        424, "rut" => 425, "rw" => 426, "ryu" => 427, "sa" => 428, "sah" => 429, "sas" =>
        430, "sat" => 431, "sc" => 432, "scn" => 433, "sco" => 434, "sd" => 435, "sdc" =>
        436, "sdh" => 437, "se" => 438, "se-fi" => 439, "se-no" => 440, "se-se" => 441,
        "sei" => 442, "ses" => 443, "sg" => 444, "sgs" => 445, "sh" => 446, "sh-cyrl" =>
        447, "sh-latn" => 448, "shi" => 449, "shi-latn" => 450, "shi-tfng" => 451, "shn"
        => 452, "shy" => 453, "shy-latn" => 454, "si" => 455, "simple" => 456, "sjd" =>
        457, "sje" => 458, "sk" => 459, "skr" => 460, "skr-arab" => 461, "sl" => 462,
        "sli" => 463, "sm" => 464, "sma" => 465, "smn" => 466, "sms" => 467, "sn" => 468,
        "so" => 469, "sq" => 470, "sr" => 471, "sr-ec" => 472, "sr-el" => 473, "srn" =>
        474, "sro" => 475, "ss" => 476, "st" => 477, "stq" => 478, "sty" => 479, "su" =>
        480, "sv" => 481, "sw" => 482, "syl" => 483, "szl" => 484, "szy" => 485, "ta" =>
        486, "tay" => 487, "tcy" => 488, "tdd" => 489, "te" => 490, "tet" => 491, "tg" =>
        492, "tg-cyrl" => 493, "tg-latn" => 494, "th" => 495, "ti" => 496, "tig" => 497,
        "tk" => 498, "tl" => 499, "tly" => 500, "tly-cyrl" => 501, "tn" => 502, "to" =>
        503, "tok" => 504, "tpi" => 505, "tr" => 506, "tru" => 507, "trv" => 508, "ts" =>
        509, "tt" => 510, "tt-cyrl" => 511, "tt-latn" => 512, "ttj" => 513, "tum" => 514,
        "tw" => 515, "ty" => 516, "tyv" => 517, "tzm" => 518, "udm" => 519, "ug" => 520,
        "ug-arab" => 521, "ug-latn" => 522, "uk" => 523, "ur" => 524, "uz" => 525,
        "uz-cyrl" => 526, "uz-latn" => 527, "ve" => 528, "vec" => 529, "vep" => 530, "vi"
        => 531, "vls" => 532, "vmf" => 533, "vmw" => 534, "vo" => 535, "vot" => 536,
        "vro" => 537, "wa" => 538, "wal" => 539, "war" => 540, "wls" => 541, "wlx" =>
        542, "wo" => 543, "wuu" => 544, "wuu-hans" => 545, "wuu-hant" => 546, "xal" =>
        547, "xh" => 548, "xmf" => 549, "xsy" => 550, "yi" => 551, "yo" => 552, "yrl" =>
        553, "yua" => 554, "yue" => 555, "yue-hans" => 556, "yue-hant" => 557, "za" =>
        558, "zea" => 559, "zgh" => 560, "zgh-latn" => 561, "zh" => 562, "zh-classical"
        => 563, "zh-cn" => 564, "zh-hans" => 565, "zh-hant" => 566, "zh-hk" => 567,
        "zh-min-nan" => 568, "zh-mo" => 569, "zh-my" => 570, "zh-sg" => 571, "zh-tw" =>
        572, "zh-yue" => 573, "zu" => 574
    },
    language_conversion_enabled: true,
    language_conversions: phf::phf_map! {
        "ban" => & ["ban"], "ban-bali" => & ["ban"], "ban-x-dharma" => & ["ban"],
        "ban-x-palmleaf" => & ["ban"], "ban-x-pku" => & ["ban"], "crh" => & ["crh-latn"],
        "crh-cyrl" => & ["crh-latn"], "crh-latn" => & ["crh-cyrl"], "gan" => &
        ["gan-hans", "gan-hant"], "gan-hans" => & ["gan"], "gan-hant" => & ["gan"],
        "ike-cans" => & ["iu"], "ike-latn" => & ["iu"], "iu" => & ["ike-cans"], "ku" => &
        ["ku-latn"], "ku-arab" => & ["ku-latn"], "ku-latn" => & ["ku-arab"], "mni" => &
        ["mni"], "mni-beng" => & ["mni"], "sh-cyrl" => & ["sh-latn"], "sh-latn" => &
        ["sh-latn"], "shi" => & ["shi-latn", "shi-tfng"], "shi-latn" => & ["shi"],
        "shi-tfng" => & ["shi"], "sr" => & ["sr-ec"], "sr-ec" => & ["sr"], "sr-el" => &
        ["sr"], "tg" => & ["tg"], "tg-latn" => & ["tg"], "tly" => & ["tly"], "tly-cyrl"
        => & ["tly"], "uz" => & ["uz-latn"], "uz-cyrl" => & ["uz"], "uz-latn" => &
        ["uz"], "wuu" => & ["wuu-hans", "wuu-hant"], "wuu-hans" => & ["wuu"], "wuu-hant"
        => & ["wuu"], "zgh" => & ["zgh"], "zgh-latn" => & ["zgh"], "zh" => & ["zh-hans",
        "zh-hant", "zh-cn", "zh-tw", "zh-hk", "zh-sg", "zh-mo", "zh-my"], "zh-cn" => &
        ["zh-hans", "zh-sg", "zh-my"], "zh-hans" => & ["zh-cn", "zh-sg", "zh-my"],
        "zh-hant" => & ["zh-tw", "zh-hk", "zh-mo"], "zh-hk" => & ["zh-mo", "zh-hant",
        "zh-tw"], "zh-mo" => & ["zh-hk", "zh-hant", "zh-tw"], "zh-my" => & ["zh-sg",
        "zh-hans", "zh-cn"], "zh-sg" => & ["zh-my", "zh-hans", "zh-cn"], "zh-tw" => &
        ["zh-hant", "zh-hk", "zh-mo"]
    },
    language_names: &[
        "Qafár af",
        "Arbërisht",
        "аԥсшәа",
        "Abron",
        "bahasa ambon",
        "Acèh",
        "Kwéyòl Sent Lisi",
        "عراقي",
        "адыгабзэ",
        "адыгабзэ",
        "تونسي / Tûnsî",
        "تونسي",
        "Tûnsî",
        "Afrikaans",
        "Aanteegan an' Baabyuudan",
        "Akan",
        "Gegë",
        "Alemannisch",
        "алтай тил",
        "አማርኛ",
        "Pangcah",
        "aragonés",
        "Ænglisc",
        "Obolo",
        "अ\u{902}गिका",
        "شامي",
        "العربية",
        "ܐܪܡܝܐ",
        "mapudungun",
        "جازايرية",
        "الدارجة",
        "مصرى",
        "অসমীয\u{9bc}\u{9be}",
        "American sign language",
        "asturianu",
        "Atikamekw",
        "авар",
        "Kotava",
        "अवधी",
        "Aymar aru",
        "azərbaycanca",
        "تۆرکجه",
        "башҡортса",
        "Basa Bali",
        "ᬩᬲᬩᬮ\u{1b36}",
        "Boarisch",
        "žemaitėška",
        "Batak Toba",
        "Batak Toba",
        "جهلسری بلوچی",
        "wawle",
        "Bikol Central",
        "Bajau Sama",
        "беларуская",
        "беларуская (тарашкевіца)",
        "беларуская (тарашкевіца)",
        "Betawi",
        "български",
        "हरियाणवी",
        "روچ کپتین بلوچی",
        "भोजप\u{941}री",
        "भोजप\u{941}री",
        "Bislama",
        "Banjar",
        "ပအ\u{102d}\u{102f}ဝ\u{103a}ႏဘာႏသာႏ",
        "bamanankan",
        "ব\u{9be}ংল\u{9be}",
        "བ\u{f7c}ད་ཡ\u{f72}ག",
        "bòo pìkkà",
        "বিষ\u{9cd}ণ\u{9c1}প\u{9cd}রিয\u{9bc}\u{9be} মণিপ\u{9c1}রী",
        "بختیاری",
        "brezhoneg",
        "Bráhuí",
        "bosanski",
        "Batak Mandailing",
        "Iriga Bicolano",
        "Basa Ugi",
        "ᨅᨔ ᨕ\u{1a18}ᨁ\u{1a17}",
        "буряад",
        "català",
        "Chavacano de Zamboanga",
        "𑄌𑄋\u{11134}𑄟\u{11133}𑄦",
        "閩東語 / Mìng-dĕ\u{324}ng-ngṳ\u{304}",
        "閩東語（傳統漢字）",
        "Mìng-dĕ\u{324}ng-ngṳ\u{304} (Bàng-uâ-cê)",
        "нохчийн",
        "Cebuano",
        "Chamoru",
        "chinuk wawa",
        "Chahta anumpa",
        "ᏣᎳᎩ",
        "Tsetsêhestâhese",
        "کوردی",
        "corsu",
        "ϯⲙⲉⲧⲣⲉⲙⲛ\u{300}ⲭⲏⲙⲓ",
        "Capiceño",
        "莆仙語 / Pó-sing-gṳ\u{302}",
        "莆仙语（简体）",
        "莆仙語（繁體）",
        "Pó-sing-gṳ\u{302} (Báⁿ-uā-ci\u{30d})",
        "Nēhiyawēwin / ᓀᐦᐃᔭᐍᐏᐣ",
        "qırımtatarca",
        "къырымтатарджа (Кирилл)",
        "qırımtatarca (Latin)",
        "tatarşa",
        "čeština",
        "kaszëbsczi",
        "словѣньскъ / ⰔⰎⰑⰂⰡⰐⰠⰔⰍⰟ",
        "чӑвашла",
        "Cymraeg",
        "dansk",
        "dagbanli",
        "Deutsch",
        "Österreichisches Deutsch",
        "Schweizer Hochdeutsch",
        "Deutsch (Sie-Form)",
        "Dagaare",
        "Thuɔŋjäŋ",
        "Zazaki",
        "долган тыла",
        "dolnoserbski",
        "Kadazandusun",
        "डोट\u{947}ली",
        "Duálá",
        "ދ\u{7a8}ވ\u{7ac}ހ\u{7a8}ބ\u{7a6}ސ\u{7b0}",
        "ཇ\u{f7c}ང་ཁ",
        "eʋegbe",
        "Efịk",
        "Emiliàn",
        "Ελληνικά",
        "emiliàn e rumagnòl",
        "English",
        "Canadian English",
        "British English",
        "Esperanto",
        "español",
        "español de América Latina",
        "español (formal)",
        "eesti",
        "euskara",
        "estremeñu",
        "فارسی",
        "mfantse",
        "Fulfulde",
        "suomi",
        "meänkieli",
        "võro",
        "Na Vosa Vakaviti",
        "føroyskt",
        "fɔ\u{300}ngbè",
        "français",
        "français cadien",
        "arpetan",
        "Nordfriisk",
        "Oostfräisk",
        "furlan",
        "poor’íŋ belé’ŋ",
        "Frysk",
        "Gaeilge",
        "Ga",
        "Gagauz",
        "贛語",
        "赣语（简体）",
        "贛語（繁體）",
        "kréyòl Gwadloup",
        "kriyòl gwiyannen",
        "Gàidhlig",
        "galego",
        "на\u{304}ни",
        "گیلکی",
        "Avañe'ẽ",
        "गो\u{902}यची को\u{902}कणी / Gõychi Konknni",
        "गो\u{902}यची को\u{902}कणी",
        "Gõychi Konknni",
        "Bahasa Hulontalo",
        "𐌲𐌿𐍄𐌹𐍃𐌺",
        "Ghanaian Pidgin",
        "Ἀρχαία ἑλληνικὴ",
        "Alemannisch",
        "ગ\u{ac1}જરાતી",
        "wayuunaiki",
        "farefare",
        "gungbe",
        "Gaelg",
        "Hausa",
        "客家語 / Hak-kâ-ngî",
        "客家语（简体）",
        "客家語（繁體）",
        "Hak-kâ-ngî (Pha\u{30d}k-fa-sṳ)",
        "Hawaiʻi",
        "עברית",
        "हिन\u{94d}दी",
        "Fiji Hindi",
        "Fiji Hindi",
        "Ilonggo",
        "kihunde",
        "ہندکو",
        "Hiri Motu",
        "Ho",
        "hrvatski",
        "Hunsrik",
        "hornjoserbsce",
        "湘語",
        "Kreyòl ayisyen",
        "magyar",
        "magyar (formal)",
        "հայերեն",
        "Արեւմտահայերէն",
        "Otsiherero",
        "interlingua",
        "Jaku Iban",
        "ibibio",
        "Bahasa Indonesia",
        "Interlingue",
        "Igbo",
        "Igala",
        "ꆇꉙ",
        "Iñupiatun",
        "ᐃᓄᒃᑎᑐᑦ",
        "inuktitut",
        "Ilokano",
        "гӀалгӀай",
        "Ido",
        "íslenska",
        "medžuslovjansky",
        "меджусловјанскы",
        "medžuslovjansky",
        "italiano",
        "ᐃᓄᒃᑎᑐᑦ / inuktitut",
        "日本語",
        "Patois",
        "la .lojban.",
        "jysk",
        "Jawa",
        "ꦗꦮ",
        "ქართული",
        "Qaraqalpaqsha",
        "Taqbaylit",
        "Karai-karai",
        "Jju",
        "адыгэбзэ",
        "адыгэбзэ",
        "Kabɩyɛ",
        "Tyap",
        "kabuverdianu",
        "Kongo",
        "Kumoring",
        "کھوار",
        "Gĩkũyũ",
        "Kırmancki",
        "Kwanyama",
        "хакас",
        "ဖ\u{1060}\u{102f}\u{1036}လ\u{102d}က\u{103a}",
        "қазақша",
        "قازاقشا (تٴوتە)",
        "قازاقشا (جۇنگو)",
        "қазақша (кирил)",
        "қазақша (Қазақстан)",
        "qazaqşa (latın)",
        "qazaqşa (Türkïya)",
        "kalaallisut",
        "ភាសាខ\u{17d2}មែរ",
        "ಕನ\u{ccd}ನಡ",
        "Yerwa Kanuri",
        "한국어",
        "조선말",
        "перем коми",
        "kanuri",
        "къарачай-малкъар",
        "Krio",
        "Kinaray-a",
        "karjal",
        "کٲش\u{64f}ر",
        "کٲش\u{64f}ر",
        "कॉश\u{941}र",
        "Ripoarisch",
        "စ\u{103e}\u{102e}ၤ",
        "kurdî",
        "کوردی (عەرەبی)",
        "kurdî (latînî)",
        "къумукъ",
        "Kʋsaal",
        "коми",
        "kernowek",
        "кыргызча",
        "Latina",
        "Ladino",
        "Lëtzebuergesch",
        "лакку",
        "лезги",
        "Lingua Franca Nova",
        "Luganda",
        "Limburgs",
        "Ligure",
        "Līvõ kēļ",
        "Lampung Api",
        "لەکی",
        "Lakȟótiyapi",
        "Ladin",
        "lombard",
        "lingála",
        "ລາວ",
        "Silozi",
        "لۊری شومالی",
        "lietuvių",
        "latgaļu",
        "ciluba",
        "Mizo ţawng",
        "لئری دو\u{659}مینی",
        "latviešu",
        "文言",
        "Lazuri",
        "Madhurâ",
        "मगही",
        "म\u{948}थिली",
        "Basa Banyumasan",
        "мокшень",
        "Malagasy",
        "Ebon",
        "олык марий",
        "Māori",
        "Minangkabau",
        "македонски",
        "മലയ\u{d3e}ളം",
        "монгол",
        "manju gisun",
        "manju gisun",
        "ᠮᠠᠨᠵᡠ ᡤᡳᠰᡠᠨ",
        "ꯃꯤꯇꯩ ꯂꯣꯟ",
        "ဘာသာမန\u{103a}",
        "молдовеняскэ",
        "moore",
        "मराठी",
        "Mara",
        "кырык мары",
        "Bahasa Melayu",
        "بهاس ملايو",
        "Malti",
        "Baso Palembang",
        "Mvskoke",
        "Mirandés",
        "မြန\u{103a}မာဘာသာ",
        "эрзянь",
        "ماز\u{650}رونی",
        "Dorerin Naoero",
        "Nāhuatl",
        "閩南語 / Bân-lâm-gí",
        "閩南語（傳統漢字）",
        "Bân-lâm-gí (Pe\u{30d}h-ōe-jī)",
        "Bân-lâm-gí (Tâi-lô)",
        "Napulitano",
        "norsk bokmål",
        "Plattdüütsch",
        "Nedersaksies",
        "न\u{947}पाली",
        "न\u{947}पाल भाषा",
        "Oshiwambo",
        "Li Niha",
        "క\u{c4a}ల\u{c3e}మ\u{c3f}",
        "Niuē",
        "Nederlands",
        "Nederlands (informeel)",
        "nawdm",
        "norsk nynorsk",
        "norsk",
        "ᨣᩤ\u{1a74}ᨾᩮ\u{1a6c}\u{1a65}ᨦ",
        "ногайша",
        "Novial",
        "ߒߞߏ",
        "isiNdebele seSewula",
        "Nouormand",
        "Sesotho sa Leboa",
        "Nupe",
        "Diné bizaad",
        "Chi-Chewa",
        "runyankore",
        "Orunyoro",
        "Nyunga",
        "occitan",
        "Ojibwemowin",
        "livvinkarjala",
        "Oromoo",
        "ଓଡ\u{b3c}\u{b3f}ଆ",
        "ирон",
        "ਪ\u{a70}ਜਾਬੀ",
        "Pangasinan",
        "Kapampangan",
        "Papiamentu",
        "Papiamento (Aruba)",
        "Picard",
        "Naijá",
        "Deitsch",
        "Plautdietsch",
        "Pälzisch",
        "पालि",
        "Norfuk / Pitkern",
        "polski",
        "Piemontèis",
        "پنجابی",
        "Ποντιακά",
        "Nawat",
        "prūsiskan",
        "پښتو",
        "português",
        "português do Brasil",
        "pinayuanan",
        "Runa Simi",
        "Runa shimi",
        "Rumagnôl",
        "Tarifit",
        "ရခ\u{102d}\u{102f}င\u{103a}",
        "rumantsch",
        "romaňi čhib",
        "romani čhib",
        "ikirundi",
        "română",
        "armãneashti",
        "tarandíne",
        "руски",
        "русский",
        "русиньскый",
        "armãneashti",
        "Vlăheşte",
        "Влахесте",
        "Vlăheşte",
        "мыхаӀбишды",
        "Ikinyarwanda",
        "うちなーぐち",
        "स\u{902}स\u{94d}क\u{943}तम\u{94d}",
        "саха тыла",
        "Sasak",
        "ᱥᱟᱱᱛᱟᱲᱤ",
        "sardu",
        "sicilianu",
        "Scots",
        "سنڌي",
        "Sassaresu",
        "کوردی خوارگ",
        "davvisámegiella",
        "davvisámegiella (Suoma bealde)",
        "davvisámegiella (Norgga bealde)",
        "davvisámegiella (Ruoŧa bealde)",
        "Cmique Itom",
        "Koyraboro Senni",
        "Sängö",
        "žemaitėška",
        "srpskohrvatski / српскохрватски",
        "српскохрватски (ћирилица)",
        "srpskohrvatski (latinica)",
        "Taclḥit",
        "Taclḥit",
        "ⵜⴰⵛⵍⵃⵉⵜ",
        "တ\u{1086}း",
        "tacawit",
        "tacawit",
        "ස\u{dd2}ංහල",
        "Simple English",
        "кӣллт са\u{304}мь кӣлл",
        "bidumsámegiella",
        "slovenčina",
        "سرائیکی",
        "سرائیکی",
        "slovenščina",
        "Schläsch",
        "Gagana Samoa",
        "åarjelsaemien",
        "anarâškielâ",
        "nuõrttsääʹmǩiõll",
        "chiShona",
        "Soomaaliga",
        "shqip",
        "српски / srpski",
        "српски (ћирилица)",
        "srpski (latinica)",
        "Sranantongo",
        "sardu campidanesu",
        "SiSwati",
        "Sesotho",
        "Seeltersk",
        "себертатар",
        "Sunda",
        "svenska",
        "Kiswahili",
        "ꠍꠤꠟꠐꠤ",
        "ślůnski",
        "Sakizaya",
        "தமிழ\u{bcd}",
        "Tayal",
        "ತುಳು",
        "ᥖᥭᥰ ᥖᥬᥲ ᥑᥨᥒᥰ",
        "త\u{c46}లుగు",
        "tetun",
        "тоҷикӣ",
        "тоҷикӣ",
        "tojikī",
        "ไทย",
        "ትግርኛ",
        "ትግሬ",
        "Türkmençe",
        "Tagalog",
        "tolışi",
        "толыши",
        "Setswana",
        "lea faka-Tonga",
        "toki pona",
        "Tok Pisin",
        "Türkçe",
        "Ṫuroyo",
        "Seediq",
        "Xitsonga",
        "татарча / tatarça",
        "татарча",
        "tatarça",
        "Orutooro",
        "chiTumbuka",
        "Twi",
        "reo tahiti",
        "тыва дыл",
        "ⵜⴰⵎⴰⵣⵉⵖⵜ",
        "удмурт",
        "ئۇيغۇرچە / Uyghurche",
        "ئۇيغۇرچە",
        "Uyghurche",
        "українська",
        "اردو",
        "oʻzbekcha / ўзбекча",
        "ўзбекча",
        "oʻzbekcha",
        "Tshivenda",
        "vèneto",
        "vepsän kel’",
        "Tiếng Việt",
        "West-Vlams",
        "Mainfränkisch",
        "emakhuwa",
        "Volapük",
        "Vaďďa",
        "võro",
        "walon",
        "wolaytta",
        "Winaray",
        "Fakaʻuvea",
        "waale",
        "Wolof",
        "吴语",
        "吴语（简体）",
        "吳語（正體）",
        "хальмг",
        "isiXhosa",
        "მარგალური",
        "saisiyat",
        "יי\u{5b4}דיש",
        "Yorùbá",
        "Nhẽẽgatú",
        "maaya t’aan",
        "粵語",
        "粵语（简体）",
        "粵語（繁體）",
        "Vahcuengh",
        "Zeêuws",
        "ⵜⴰⵎⴰⵣⵉⵖⵜ ⵜⴰⵏⴰⵡⴰⵢⵜ",
        "tamaziɣt tanawayt",
        "中文",
        "文言",
        "中文（中国大陆）",
        "中文（简体）",
        "中文（繁體）",
        "中文（香港）",
        "Bân-lâm-gú",
        "中文（澳門）",
        "中文（马来西亚）",
        "中文（新加坡）",
        "中文（臺灣）",
        "粵語",
        "isiZulu",
    ],
    link_prefix: "",
    link_trail: "/^([a-z]+)(.*)$/sD",
    magic_links: MagicLinks {
        isbn: false,
        pmid: false,
        rfc: false,
    },
    namespaces: &[
        Namespace {
            id: -1,
            name: "Special",
            canonical: Some("Special"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: -2,
            name: "Media",
            canonical: Some("Media"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 0,
            name: "",
            canonical: None,
            case: FirstLetter,
            content: true,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 1,
            name: "Talk",
            canonical: Some("Talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 10,
            name: "Template",
            canonical: Some("Template"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["TM"],
        },
        Namespace {
            id: 100,
            name: "Portal",
            canonical: Some("Portal"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 101,
            name: "Portal talk",
            canonical: Some("Portal talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 11,
            name: "Template talk",
            canonical: Some("Template talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 118,
            name: "Draft",
            canonical: Some("Draft"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 119,
            name: "Draft talk",
            canonical: Some("Draft talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 12,
            name: "Help",
            canonical: Some("Help"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 126,
            name: "MOS",
            canonical: Some("MOS"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 127,
            name: "MOS talk",
            canonical: Some("MOS talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 13,
            name: "Help talk",
            canonical: Some("Help talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 14,
            name: "Category",
            canonical: Some("Category"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 15,
            name: "Category talk",
            canonical: Some("Category talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 1728,
            name: "Event",
            canonical: Some("Event"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 1729,
            name: "Event talk",
            canonical: Some("Event talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 2,
            name: "User",
            canonical: Some("User"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 3,
            name: "User talk",
            canonical: Some("User talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 4,
            name: "Wikipedia",
            canonical: Some("Project"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["WP"],
        },
        Namespace {
            id: 5,
            name: "Wikipedia talk",
            canonical: Some("Project talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["WT"],
        },
        Namespace {
            id: 6,
            name: "File",
            canonical: Some("File"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &["Image"],
        },
        Namespace {
            id: 7,
            name: "File talk",
            canonical: Some("File talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &["Image talk"],
        },
        Namespace {
            id: 710,
            name: "TimedText",
            canonical: Some("TimedText"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 711,
            name: "TimedText talk",
            canonical: Some("TimedText talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 8,
            name: "MediaWiki",
            canonical: Some("MediaWiki"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: false,
            aliases: &[],
        },
        Namespace {
            id: 828,
            name: "Module",
            canonical: Some("Module"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 829,
            name: "Module talk",
            canonical: Some("Module talk"),
            case: FirstLetter,
            content: false,
            default_content_model: None,
            subpages: true,
            aliases: &[],
        },
        Namespace {
            id: 9,
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
        "//", "bitcoin:", "ftp://", "ftps://", "geo:", "git://", "gopher://", "http://",
        "https://", "irc://", "ircs://", "magnet:", "mailto:", "matrix:", "mms://",
        "news:", "nntp://", "redis://", "sftp://", "sip:", "sips:", "sms:", "ssh://",
        "svn://", "tel:", "telnet://", "urn:", "wikipedia://", "worldwind://", "xmpp:"
    },
    redirect_magic_words: phf::phf_set! {
        "#redirect"
    },
    special_pages: SpecialPages {
        aliases: phf::phf_map! {
            "abusefilter" => "AbuseFilter", "abuselog" => "AbuseLog", "accountrecovery"
            => "AccountRecovery", "accountsecurity" => "OATHManage", "activeusers" =>
            "Activeusers", "allevents" => "AllEvents", "allmessages" => "Allmessages",
            "allmyfiles" => "AllMyUploads", "allmyuploads" => "AllMyUploads", "allpages"
            => "Allpages", "ancientpages" => "Ancientpages", "apifeatureusage" =>
            "ApiFeatureUsage", "apihelp" => "ApiHelp", "apisandbox" => "ApiSandbox",
            "authenticationpopupsuccess" => "AuthenticationPopupSuccess", "autoblocklist"
            => "AutoblockList", "automatictranslation" => "AutomaticTranslation",
            "bannerloader" => "BannerLoader", "bannerrandom" => "BannerRandom",
            "betafeatures" => "BetaFeatures", "blankpage" => "Blankpage", "block" =>
            "Block", "blockedexternaldomains" => "BlockedExternalDomains", "blockip" =>
            "Block", "blocklist" => "BlockList", "blockuser" => "Block", "book" =>
            "Book", "booksources" => "Booksources", "botpasswords" => "BotPasswords",
            "brokenlinks" => "Wantedpages", "brokenredirects" => "BrokenRedirects", "ca"
            => "CentralAuth", "canceleventregistration" => "CancelEventRegistration",
            "captcha" => "Captcha", "categories" => "Categories", "categorytree" =>
            "CategoryTree", "centralauth" => "CentralAuth", "centralautologin" =>
            "CentralAutoLogin", "centrallogin" => "CentralLogin", "changecontentmodel" =>
            "ChangeContentModel", "changecredentials" => "ChangeCredentials",
            "changeemail" => "ChangeEmail", "changepassword" => "ChangePassword",
            "checkuser" => "CheckUser", "checkuserlog" => "CheckUserLog", "cite" =>
            "CiteThisPage", "citethispage" => "CiteThisPage", "claimmentee" =>
            "ClaimMentee", "collab_pad" => "CollabPad", "collabpad" => "CollabPad",
            "collection" => "Book", "communityconfig" => "CommunityConfiguration",
            "communityconfiguration" => "CommunityConfiguration", "comparepages" =>
            "ComparePages", "confirmemail" => "Confirmemail", "contact" => "Contact",
            "contenttranslation" => "ContentTranslation", "contribs" => "Contributions",
            "contribute" => "Contribute", "contributions" => "Contributions",
            "createaccount" => "CreateAccount", "createlocalaccount" =>
            "CreateLocalAccount", "createmassmessagelist" => "CreateMassMessageList",
            "cx" => "ContentTranslation", "deadendpages" => "Deadendpages", "delete" =>
            "DeletePage", "deletedcontribs" => "DeletedContributions",
            "deletedcontributions" => "DeletedContributions", "deleteeventregistration"
            => "DeleteEventRegistration", "deletepage" => "DeletePage", "diff" => "Diff",
            "disableglobalblock" => "GlobalBlockStatus", "disableoathforuser" =>
            "DisableOATHForUser", "disambiguationpagelinks" => "DisambiguationPageLinks",
            "disambiguationpages" => "DisambiguationPages", "discussiontoolsdebug" =>
            "DiscussionToolsDebug", "displaynotificationsconfiguration" =>
            "DisplayNotificationsConfiguration", "doubleredirects" => "DoubleRedirects",
            "downloadaspdf" => "DownloadAsPdf", "edit" => "EditPage", "edit_checks" =>
            "EditChecks", "editchecks" => "EditChecks", "editeventregistration" =>
            "EditEventRegistration", "editgrowthconfig" => "EditGrowthConfig",
            "editmassmessagelist" => "EditMassMessageList", "editpage" => "EditPage",
            "editrecovery" => "EditRecovery", "edittags" => "EditTags", "editwatchlist"
            => "EditWatchlist", "editwikisets" => "WikiSets", "electronpdf" =>
            "DownloadAsPdf", "email" => "Emailuser", "emailuser" => "Emailuser",
            "enableeventregistration" => "EnableEventRegistration", "enrollasmentor" =>
            "EnrollAsMentor", "entityusage" => "EntityUsage", "entityusagedata" =>
            "EntityUsage", "eventdetails" => "EventDetails", "expandtemplates" =>
            "ExpandTemplates", "export" => "Export", "externalguidance" =>
            "ExternalGuidance", "feeditem" => "FeedItem", "fewestrevisions" =>
            "Fewestrevisions", "fileduplicatesearch" => "FileDuplicateSearch", "filelist"
            => "Listfiles", "filepath" => "Filepath", "findcomment" => "FindComment",
            "gadgets" => "Gadgets", "gadgetusage" => "GadgetUsage",
            "generateinvitationlist" => "GenerateInvitationList", "globalaccount" =>
            "CentralAuth", "globalblock" => "GlobalBlock", "globalblocklist" =>
            "GlobalBlockList", "globalblockstatus" => "GlobalBlockStatus",
            "globalblockwhitelist" => "GlobalBlockStatus", "globalcontribs" =>
            "GlobalContributions", "globalcontributions" => "GlobalContributions",
            "globalgroupmembership" => "GlobalGroupMembership", "globalgrouppermissions"
            => "GlobalGroupPermissions", "globaljsonlinks" => "GlobalJsonLinks",
            "globallywantedfiles" => "GloballyWantedFiles", "globalpreferences" =>
            "GlobalPreferences", "globalrenameprogress" => "GlobalRenameProgress",
            "globalrenamequeue" => "GlobalRenameQueue", "globalrenamerequest" =>
            "GlobalRenameRequest", "globalrenameuser" => "GlobalRenameUser",
            "globalunblock" => "RemoveGlobalBlock", "globalusage" => "GlobalUsage",
            "globaluserrights" => "GlobalGroupMembership", "globalusers" =>
            "GlobalUsers", "globalvanishrequest" => "GlobalVanishRequest", "gotocomment"
            => "GoToComment", "gotointerwiki" => "GoToInterwiki", "hidebanners" =>
            "HideBanners", "hieroglyphs" => "Hieroglyphs", "history" => "History",
            "homepage" => "Homepage", "imagelist" => "Listfiles", "impact" => "Impact",
            "import" => "Import", "info" => "PageInfo", "interwiki" => "Interwiki",
            "invalidateemail" => "Invalidateemail", "investigate" => "Investigate",
            "investigateblock" => "InvestigateBlock", "invitationlist" =>
            "InvitationList", "ipblocklist" => "BlockList", "ipcontribs" =>
            "IPContributions", "ipcontributions" => "IPContributions", "ipinfo" =>
            "IPInfo", "linkaccounts" => "LinkAccounts", "linksearch" => "LinkSearch",
            "linterrors" => "LintErrors", "linttemplateerrors" => "LintTemplateErrors",
            "listadmins" => "Listadmins", "listautoblocks" => "AutoblockList",
            "listblocks" => "BlockList", "listbots" => "Listbots", "listduplicatedfiles"
            => "ListDuplicatedFiles", "listfileduplicates" => "ListDuplicatedFiles",
            "listfiles" => "Listfiles", "listglobalblocks" => "GlobalBlockList",
            "listgrants" => "Listgrants", "listgrouprights" => "Listgrouprights",
            "listredirects" => "Listredirects", "listusers" => "Listusers", "lockdb" =>
            "Lockdb", "log" => "Log", "login" => "Userlogin", "logout" => "Userlogout",
            "logs" => "Log", "lonelypages" => "Lonelypages", "longpages" => "Longpages",
            "makebot" => "Userrights", "makesysop" => "Userrights",
            "manage_two-factor_authentication" => "OATHManage", "managementors" =>
            "ManageMentors", "manageshorturls" => "ManageShortUrls", "map" => "Map",
            "massglobalblock" => "MassGlobalBlock", "massmessage" => "MassMessage",
            "mathshowimage" => "MathShowImage", "mathstatus" => "MathStatus",
            "mathwikibase" => "MathWikibase", "mediastatistics" => "MediaStatistics",
            "mediastats" => "MediaStatistics", "mentordashboard" => "MentorDashboard",
            "mergeaccount" => "MergeAccount", "mergehistory" => "MergeHistory",
            "mimesearch" => "MIMEsearch", "mint" => "AutomaticTranslation", "mobilediff"
            => "MobileDiff", "mobilelanguages" => "MobileLanguages", "mobileoptions" =>
            "MobileOptions", "mostcategories" => "Mostcategories", "mostfiles" =>
            "Mostimages", "mostgloballylinkedfiles" => "MostGloballyLinkedFiles",
            "mostimages" => "Mostimages", "mostinterwikis" => "Mostinterwikis",
            "mostlinked" => "Mostlinked", "mostlinkedcategories" =>
            "Mostlinkedcategories", "mostlinkedfiles" => "Mostimages", "mostlinkedpages"
            => "Mostlinked", "mostlinkedtemplates" => "Mostlinkedtemplates",
            "mostrevisions" => "Mostrevisions", "mosttranscludedpages" =>
            "Mostlinkedtemplates", "mostusedcategories" => "Mostlinkedcategories",
            "mostusedtemplates" => "Mostlinkedtemplates", "movepage" => "Movepage",
            "multiglobalblock" => "MassGlobalBlock", "multilock" => "MultiLock", "mute"
            => "Mute", "muteuser" => "Mute", "mwoauth" => "OAuth", "mycontribs" =>
            "Mycontributions", "mycontributions" => "Mycontributions", "myevents" =>
            "MyEvents", "myfiles" => "Myuploads", "myinvitationlists" =>
            "MyInvitationLists", "mylanguage" => "MyLanguage", "mylog" => "Mylog",
            "mypage" => "Mypage", "mytalk" => "Mytalk", "myuploads" => "Myuploads",
            "namespaceinfo" => "NamespaceInfo", "nearby" => "Nearby", "newcomertasksinfo"
            => "NewcomerTasksInfo", "newfiles" => "Newimages", "newimages" =>
            "Newimages", "newpages" => "Newpages", "newpagesfeed" => "NewPagesFeed",
            "newsection" => "NewSection", "notifications" => "Notifications",
            "notificationsmarkread" => "NotificationsMarkRead", "nuke" => "Nuke", "oath"
            => "OATHManage", "oath_manage" => "OATHManage", "oathauth" => "OATHManage",
            "oathmanage" => "OATHManage", "oauth" => "OAuth", "oauthconsumerregistration"
            => "OAuthConsumerRegistration", "oauthgrants" => "OAuthManageMyGrants",
            "oauthlistconsumers" => "OAuthListConsumers", "oauthmanageconsumers" =>
            "OAuthManageConsumers", "oauthmanagemygrants" => "OAuthManageMyGrants",
            "oauthregistration" => "OAuthConsumerRegistration", "oldreviewedpages" =>
            "PendingChanges", "oresmodels" => "ORESModels", "orphanedpages" =>
            "Lonelypages", "orphanedtimedtext" => "OrphanedTimedText", "pageassessments"
            => "PageAssessments", "pagedata" => "PageData", "pagehistory" =>
            "PageHistory", "pageinfo" => "PageInfo", "pagesbyprop" => "PagesWithProp",
            "pageswithbadges" => "PagesWithBadges", "pageswithprop" => "PagesWithProp",
            "passwordpolicies" => "PasswordPolicies", "passwordreset" => "PasswordReset",
            "pendingchanges" => "PendingChanges", "permalink" => "PermanentLink",
            "permanentlink" => "PermanentLink", "personal_dashboard" =>
            "PersonalDashboard", "personaldashboard" => "PersonalDashboard",
            "preferences" => "Preferences", "prefixindex" => "Prefixindex", "protect" =>
            "ProtectPage", "protectedpages" => "Protectedpages", "protectedtitles" =>
            "Protectedtitles", "protectpage" => "ProtectPage", "providesubmittedtext" =>
            "TwoColConflictProvideSubmittedText", "purge" => "Purge", "qrcode" =>
            "QrCode", "querybadges" => "PagesWithBadges", "quitmentorship" =>
            "QuitMentorship", "random" => "Randompage", "randomincategory" =>
            "RandomInCategory", "randompage" => "Randompage", "randomredirect" =>
            "Randomredirect", "randomrootpage" => "Randomrootpage", "readinglists" =>
            "ReadingLists", "recentchanges" => "Recentchanges", "recentchangeslinked" =>
            "Recentchangeslinked", "recordimpression" => "RecordImpression",
            "recover2faforuser" => "Recover2FAForUser", "redirect" => "Redirect",
            "registerforevent" => "RegisterForEvent", "relatedchanges" =>
            "Recentchangeslinked", "removecredentials" => "RemoveCredentials",
            "removeglobalblock" => "RemoveGlobalBlock", "renameuser" => "Renameuser",
            "resetpass" => "ChangePassword", "resetpassword" => "ChangePassword",
            "resettokens" => "ResetTokens", "restsandbox" => "RestSandbox",
            "revisiondelete" => "Revisiondelete", "revisionreview" => "RevisionReview",
            "runjobs" => "RunJobs", "search" => "Search", "securepoll" => "SecurePoll",
            "securepolllog" => "SecurePollLog", "shortpages" => "Shortpages",
            "sitematrix" => "SiteMatrix", "specialpages" => "Specialpages", "stablepages"
            => "StablePages", "statistics" => "Statistics", "stats" => "Statistics",
            "suggestedinvestigations" => "SuggestedInvestigations", "tags" => "Tags",
            "talkpage" => "TalkPage", "template_discovery" => "TemplateDiscovery",
            "templatediscovery" => "TemplateDiscovery", "templatesandbox" =>
            "TemplateSandbox", "templatesearch" => "TemplateDiscovery", "thanks" =>
            "Thanks", "timedmediahandler" => "TranscodeStatistics", "topicsubscriptions"
            => "TopicSubscriptions", "trackingcategories" => "TrackingCategories",
            "transcode_statistics" => "TranscodeStatistics", "two-factor_authentication"
            => "OATHManage", "unblock" => "Unblock", "uncategorizedcategories" =>
            "Uncategorizedcategories", "uncategorizedfiles" => "Uncategorizedimages",
            "uncategorizedimages" => "Uncategorizedimages", "uncategorizedpages" =>
            "Uncategorizedpages", "uncategorizedtemplates" => "Uncategorizedtemplates",
            "unconnectedpages" => "UnconnectedPages", "undelete" => "Undelete",
            "unlinkaccounts" => "UnlinkAccounts", "unlockdb" => "Unlockdb",
            "unusedcategories" => "Unusedcategories", "unusedfiles" => "Unusedimages",
            "unusedimages" => "Unusedimages", "unusedtemplates" => "Unusedtemplates",
            "unwatchedpages" => "Unwatchedpages", "upload" => "Upload", "uploads" =>
            "Uploads", "uploadstash" => "UploadStash", "urlredirector" =>
            "UrlRedirector", "urlshortener" => "UrlShortener", "usergrouprights" =>
            "Listgrouprights", "userlist" => "Listusers", "userlogin" => "Userlogin",
            "userlogout" => "Userlogout", "userrights" => "Userrights", "users" =>
            "Listusers", "validationstatistics" => "ValidationStatistics",
            "verifyoathforuser" => "VerifyOATHForUser", "version" => "Version",
            "versions" => "Version", "wantedcategories" => "Wantedcategories",
            "wantedfiles" => "Wantedfiles", "wantedpages" => "Wantedpages",
            "wantedtemplates" => "Wantedtemplates", "watchlist" => "Watchlist",
            "watchlistlabels" => "WatchlistLabels", "welcomesurvey" => "WelcomeSurvey",
            "whatlinkshere" => "Whatlinkshere", "wikimediadebug" => "WikimediaDebug",
            "wikimediawikis" => "SiteMatrix", "wikisets" => "WikiSets",
            "withoutconnection" => "UnconnectedPages", "withoutinterwiki" =>
            "Withoutinterwiki", "withoutsitelinks" => "UnconnectedPages"
        },
        canonical: phf::phf_map! {
            "Activeusers" => "ActiveUsers", "Allmessages" => "AllMessages", "Allpages" =>
            "AllPages", "Ancientpages" => "AncientPages", "Blankpage" => "BlankPage",
            "Booksources" => "BookSources", "Confirmemail" => "ConfirmEmail",
            "Deadendpages" => "DeadendPages", "Emailuser" => "EmailUser",
            "Fewestrevisions" => "FewestRevisions", "Filepath" => "FilePath",
            "GlobalBlockStatus" => "GlobalBlockWhitelist", "GlobalGroupMembership" =>
            "GlobalUserRights", "Invalidateemail" => "InvalidateEmail", "Listadmins" =>
            "ListAdmins", "Listbots" => "ListBots", "Listfiles" => "ListFiles",
            "Listgrants" => "ListGrants", "Listgrouprights" => "ListGroupRights",
            "Listredirects" => "ListRedirects", "Listusers" => "ListUsers", "Lockdb" =>
            "LockDB", "Lonelypages" => "LonelyPages", "Longpages" => "LongPages",
            "MIMEsearch" => "MIMESearch", "Mostcategories" => "MostCategories",
            "Mostimages" => "MostLinkedFiles", "Mostinterwikis" => "MostInterwikis",
            "Mostlinked" => "MostLinkedPages", "Mostlinkedcategories" =>
            "MostLinkedCategories", "Mostlinkedtemplates" => "MostTranscludedPages",
            "Mostrevisions" => "MostRevisions", "Movepage" => "MovePage",
            "Mycontributions" => "MyContributions", "Mylog" => "MyLog", "Mypage" =>
            "MyPage", "Mytalk" => "MyTalk", "Myuploads" => "MyUploads", "Newimages" =>
            "NewFiles", "Newpages" => "NewPages", "OATHManage" => "AccountSecurity",
            "Prefixindex" => "PrefixIndex", "Protectedpages" => "ProtectedPages",
            "Protectedtitles" => "ProtectedTitles", "Randompage" => "Random",
            "Randomredirect" => "RandomRedirect", "Randomrootpage" => "RandomRootpage",
            "Recentchanges" => "RecentChanges", "Recentchangeslinked" =>
            "RecentChangesLinked", "RemoveGlobalBlock" => "GlobalUnblock", "Renameuser"
            => "RenameUser", "Revisiondelete" => "RevisionDelete", "Shortpages" =>
            "ShortPages", "Specialpages" => "SpecialPages", "TranscodeStatistics" =>
            "Transcode_statistics", "TwoColConflictProvideSubmittedText" =>
            "ProvideSubmittedText", "Uncategorizedcategories" =>
            "UncategorizedCategories", "Uncategorizedimages" => "UncategorizedFiles",
            "Uncategorizedpages" => "UncategorizedPages", "Uncategorizedtemplates" =>
            "UncategorizedTemplates", "Unlockdb" => "UnlockDB", "Unusedcategories" =>
            "UnusedCategories", "Unusedimages" => "UnusedFiles", "Unusedtemplates" =>
            "UnusedTemplates", "Unwatchedpages" => "UnwatchedPages", "Userlogin" =>
            "UserLogin", "Userlogout" => "UserLogout", "Userrights" => "UserRights",
            "Wantedcategories" => "WantedCategories", "Wantedfiles" => "WantedFiles",
            "Wantedpages" => "WantedPages", "Wantedtemplates" => "WantedTemplates",
            "Whatlinkshere" => "WhatLinksHere", "Withoutinterwiki" => "WithoutInterwiki"
        },
    },
    valid_title_bytes: " %!\"$&'()*,\\-.\\/0-9:;=?@A-Z\\\\^_`a-z~\\x80-\\xFF+",
    variables: phf::phf_map! {
        "!" => "!", "=" => "=", "articlepath" => "articlepath", "basepagename" =>
        "basepagename", "basepagenamee" => "basepagenamee", "#bcp47" => "bcp47",
        "cascadingsources" => "cascadingsources", "contentlanguage" => "contentlanguage",
        "contentlang" => "contentlanguage", "#contentmodel" => "contentmodel",
        "currentday" => "currentday", "currentday2" => "currentday2", "currentdayname" =>
        "currentdayname", "currentdow" => "currentdow", "currenthour" => "currenthour",
        "currentmonth" => "currentmonth", "currentmonth2" => "currentmonth",
        "currentmonth1" => "currentmonth1", "currentmonthabbrev" => "currentmonthabbrev",
        "currentmonthname" => "currentmonthname", "currentmonthnamegen" =>
        "currentmonthnamegen", "currenttime" => "currenttime", "currenttimestamp" =>
        "currenttimestamp", "currentversion" => "currentversion", "currentweek" =>
        "currentweek", "currentyear" => "currentyear", "#dir" => "dir", "directionmark"
        => "directionmark", "dirmark" => "directionmark", "fullpagename" =>
        "fullpagename", "fullpagenamee" => "fullpagenamee", "#language" => "language",
        "localday" => "localday", "localday2" => "localday2", "localdayname" =>
        "localdayname", "localdow" => "localdow", "localhour" => "localhour",
        "localmonth" => "localmonth", "localmonth2" => "localmonth", "localmonth1" =>
        "localmonth1", "localmonthabbrev" => "localmonthabbrev", "localmonthname" =>
        "localmonthname", "localmonthnamegen" => "localmonthnamegen", "localtime" =>
        "localtime", "localtimestamp" => "localtimestamp", "localweek" => "localweek",
        "localyear" => "localyear", "namespace" => "namespace", "namespacee" =>
        "namespacee", "namespacenumber" => "namespacenumber", "noexternallanglinks" =>
        "noexternallanglinks", "numberofactiveusers" => "numberofactiveusers",
        "numberofadmins" => "numberofadmins", "numberofarticles" => "numberofarticles",
        "numberofedits" => "numberofedits", "numberoffiles" => "numberoffiles",
        "numberofpages" => "numberofpages", "numberofusers" => "numberofusers",
        "numberofwikis" => "numberofwikis", "pageid" => "pageid", "pagelanguage" =>
        "pagelanguage", "pagename" => "pagename", "pagenamee" => "pagenamee",
        "pendingchangelevel" => "pendingchangelevel", "revisionday" => "revisionday",
        "revisionday2" => "revisionday2", "revisionid" => "revisionid", "revisionmonth"
        => "revisionmonth", "revisionmonth1" => "revisionmonth1", "revisionsize" =>
        "revisionsize", "revisiontimestamp" => "revisiontimestamp", "revisionuser" =>
        "revisionuser", "revisionyear" => "revisionyear", "rootpagename" =>
        "rootpagename", "rootpagenamee" => "rootpagenamee", "scriptpath" => "scriptpath",
        "server" => "server", "servername" => "servername", "sitename" => "sitename",
        "stylepath" => "stylepath", "subjectpagename" => "subjectpagename",
        "articlepagename" => "subjectpagename", "subjectpagenamee" => "subjectpagenamee",
        "articlepagenamee" => "subjectpagenamee", "subjectspace" => "subjectspace",
        "articlespace" => "subjectspace", "subjectspacee" => "subjectspacee",
        "articlespacee" => "subjectspacee", "subpagename" => "subpagename",
        "subpagenamee" => "subpagenamee", "talkpagename" => "talkpagename",
        "talkpagenamee" => "talkpagenamee", "talkspace" => "talkspace", "talkspacee" =>
        "talkspacee", "userlanguage" => "userlanguage", "wbreponame" => "wbreponame"
    },
};

/// The installation configuration, suitable for runtime use.
pub static CONFIG: LazyLock<Configuration> = LazyLock::new(|| Configuration::new(&CONFIG_SOURCE));
