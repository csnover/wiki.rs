//! MediaWiki configuration.
//!
//! Wikitext documents are not self-encapsulated and cannot be parsed without
//! out-of-band configuration data. Most of this configuration data can be
//! acquired by querying the MediaWiki API for a given MediaWiki installation.

use libwikitext_common::{
    config::{
        Configuration, ConfigurationSource, ImageHotlinking, Language, MagicLinks, SpecialPages,
    },
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
    image_hotlinking: ImageHotlinking::Disabled,
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
        "aa" => 0, "aae" => 1, "ab" => 2, "abe" => 3, "abq" => 4, "abq-Latn" => 5, "abr"
        => 6, "abs" => 7, "ace" => 8, "acf" => 9, "ach" => 10, "acm" => 11, "ada" => 12,
        "adg" => 13, "ady" => 14, "ady-Cyrl" => 15, "ady-Latn" => 16, "ae" => 17, "aeb"
        => 18, "aeb-Arab" => 19, "aeb-Latn" => 20, "aec" => 21, "aee" => 22, "aer" => 23,
        "af" => 24, "afa" => 25, "afh" => 26, "agq" => 27, "aha" => 28, "ahr" => 29,
        "aig" => 30, "aii" => 31, "ain" => 32, "ajg" => 33, "ajp" => 34, "ajp-Arab" =>
        35, "ajp-Latn" => 36, "ak" => 37, "akb" => 38, "akk" => 39, "akk-Latn" => 40,
        "akk-Xsux" => 41, "akz" => 42, "alc" => 43, "ale" => 44, "ale-Cyrl" => 45, "alg"
        => 46, "aln" => 47, "alq" => 48, "gsw" => 49, "alt" => 50, "aly" => 51, "am" =>
        52, "ami" => 53, "amx" => 54, "an" => 55, "ane" => 56, "ang" => 57, "ann" => 58,
        "anp" => 59, "apa" => 60, "apc" => 61, "apc-Arab" => 62, "apc-Latn" => 63, "apw"
        => 64, "ar" => 65, "ar-001" => 66, "arc" => 67, "are" => 68, "arn" => 69, "aro"
        => 70, "arp" => 71, "arq" => 72, "ars" => 73, "art" => 74, "arw" => 75, "ary" =>
        76, "ary-Arab" => 77, "ary-Latn" => 78, "arz" => 79, "as" => 80, "asa" => 81,
        "ase" => 82, "ast" => 83, "ath" => 84, "atj" => 85, "atv" => 86, "aus" => 87,
        "av" => 88, "avk" => 89, "awa" => 90, "axe" => 91, "axl" => 92, "ay" => 93, "ayh"
        => 94, "az" => 95, "az-Arab" => 96, "az-Cyrl" => 97, "az-Latn" => 98, "azb" =>
        99, "azj" => 100, "ba" => 101, "bad" => 102, "bag" => 103, "bai" => 104, "bal" =>
        105, "bal-Latn" => 106, "ban" => 107, "ban-Bali" => 108, "bar" => 109, "bas" =>
        110, "bat" => 111, "sgs" => 112, "bax" => 113, "bbc" => 114, "bbc-Batk" => 115,
        "bbc-Latn" => 116, "bbj" => 117, "bcc" => 118, "bci" => 119, "bcl" => 120, "bdr"
        => 121, "be" => 122, "be-tarask" => 123, "bej" => 125, "bem" => 126, "ber" =>
        127, "bew" => 128, "bez" => 129, "bfa" => 130, "bfd" => 131, "bfi" => 132, "bfq"
        => 133, "bft" => 134, "bft-Tibt" => 135, "bfw" => 136, "bfz" => 137, "bfz-Deva"
        => 138, "bfz-Takr" => 139, "bg" => 140, "bgc" => 141, "bgc-Arab" => 142,
        "bgc-Deva" => 143, "bgn" => 144, "bgp" => 145, "bgq" => 146, "bgq-Arab" => 147,
        "bgq-Deva" => 148, "bh" => 149, "bha" => 150, "bhd" => 151, "bhd-Deva" => 152,
        "bhd-Takr" => 153, "bho" => 154, "bi" => 155, "bik" => 156, "bin" => 157, "bjn"
        => 158, "bkc" => 159, "bkh" => 160, "bkm" => 161, "bkn" => 162, "bla" => 163,
        "blc" => 164, "blk" => 165, "blo" => 166, "blt" => 167, "bm" => 168, "bn" => 169,
        "bnb" => 170, "bnn" => 171, "bnt" => 172, "bny" => 173, "bo" => 174, "bol" =>
        175, "bom" => 176, "bpy" => 177, "bqi" => 178, "bqz" => 179, "br" => 180, "bra"
        => 181, "brh" => 182, "brh-Latn" => 183, "brx" => 184, "bs" => 185, "bse" => 186,
        "bsk" => 187, "bss" => 188, "btd" => 189, "bth" => 190, "btk" => 191, "btm" =>
        192, "bto" => 193, "bts" => 194, "btx" => 195, "btz" => 196, "bua" => 197, "bug"
        => 198, "bug-Bugi" => 199, "bum" => 200, "bvb" => 201, "bwr" => 202, "bxr" =>
        203, "byn" => 204, "byv" => 205, "bzj" => 206, "bzs" => 207, "ca" => 208, "cad"
        => 209, "cai" => 210, "cak" => 211, "cal" => 212, "car" => 213, "cau" => 214,
        "cay" => 215, "cbk" => 216, "cch" => 218, "ccp" => 219, "ccp-Beng" => 220, "cdo"
        => 221, "cdo-Hani" => 222, "cdo-Hant" => 223, "cdo-Latn" => 224, "cdz-Beng" =>
        225, "ce" => 226, "ceb" => 227, "cel" => 228, "cgg" => 229, "ch" => 230, "chb" =>
        231, "chg" => 232, "chk" => 233, "chm" => 234, "chn" => 235, "cho" => 236, "chp"
        => 237, "chr" => 238, "chy" => 239, "cic" => 240, "ciw" => 241, "cja" => 242,
        "cja-Arab" => 243, "cja-Cham" => 244, "cja-Latn" => 245, "cjm" => 246, "cjm-Arab"
        => 247, "cjm-Cham" => 248, "cjm-Latn" => 249, "cjy" => 250, "cjy-Hans" => 251,
        "cjy-Hant" => 252, "ckb" => 253, "ckb-Arab" => 254, "ckb-Latn" => 255, "cko" =>
        256, "ckt" => 257, "ckv" => 258, "clc" => 259, "cmc" => 260, "cmg" => 261, "cnh"
        => 262, "cnr" => 263, "cnr-Cyrl" => 264, "cnr-Latn" => 265, "cnx" => 266, "co" =>
        267, "coa" => 268, "cop" => 269, "cpe" => 270, "cpf" => 271, "cpp" => 272, "cps"
        => 273, "cpx" => 274, "cpx-Hans" => 275, "cpx-Hant" => 276, "cpx-Latn" => 277,
        "cr" => 278, "cr-Cans" => 279, "cr-Latn" => 280, "crb" => 281, "crg" => 282,
        "crh" => 283, "crh-Cyrl" => 284, "crh-Latn" => 285, "crh-Latn-RO" => 286, "crj"
        => 287, "crk" => 288, "crl" => 289, "crm" => 290, "crp" => 291, "crr" => 292,
        "crs" => 293, "cs" => 294, "csb" => 295, "csw" => 296, "ctg" => 297, "cu" => 298,
        "cus" => 299, "cv" => 300, "cy" => 301, "da" => 302, "dag" => 303, "dak" => 304,
        "dar" => 305, "dav" => 306, "day" => 307, "dbj" => 308, "ddn" => 309, "de" =>
        310, "de-1901" => 311, "de-AT" => 312, "de-CH" => 313, "de-x-formal" => 314,
        "del" => 315, "den" => 316, "dga" => 317, "dgr" => 318, "din" => 319, "diq" =>
        320, "dje" => 321, "dkr" => 322, "dlg" => 323, "dmg" => 324, "dmv" => 325, "doi"
        => 326, "doi-Arab" => 327, "doi-Deva" => 328, "doi-Dogr" => 329, "dpp" => 330,
        "dra" => 331, "drg" => 332, "dro" => 333, "dru" => 334, "dsb" => 335, "dso" =>
        336, "dtb" => 337, "dtp" => 338, "dtr" => 339, "dty" => 340, "dua" => 341, "duf"
        => 342, "dum" => 343, "dv" => 344, "dyo" => 345, "dyu" => 346, "dz" => 347, "dzg"
        => 348, "ebu" => 349, "ee" => 350, "efi" => 351, "egl" => 352, "egy" => 353,
        "eka" => 354, "ekp" => 355, "el" => 356, "el-CY" => 357, "elm" => 358, "elx" =>
        359, "en" => 361, "en-AU" => 362, "en-CA" => 363, "en-GB" => 364, "en-IN" => 365,
        "en-JM" => 366, "en-NZ" => 367, "en-simple" => 368, "en-UK" => 369, "en-US" =>
        370, "enm" => 371, "eo" => 372, "eo-hsistemo" => 373, "eo-xsistemo" => 374, "es"
        => 375, "es-419" => 376, "es-ES" => 377, "es-x-formal" => 378, "es-MX" => 379,
        "es-NI" => 380, "ess" => 381, "esu" => 382, "et" => 383, "eto" => 384, "ett" =>
        385, "etu" => 386, "eu" => 387, "ewo" => 388, "ext" => 389, "eya" => 390, "fa" =>
        391, "fa-AF" => 392, "fab" => 393, "fan" => 394, "fat" => 395, "fax" => 396,
        "fay" => 397, "ff" => 398, "fi" => 399, "fil" => 400, "fit" => 401, "fiu" => 402,
        "vro" => 403, "fj" => 404, "fkv" => 405, "fmp" => 406, "fo" => 407, "fon" => 408,
        "fos" => 409, "fr" => 410, "fr-BE" => 411, "fr-CA" => 412, "fr-CH" => 413, "frc"
        => 414, "frk" => 415, "frm" => 416, "fro" => 417, "frp" => 418, "frr" => 419,
        "frs" => 420, "fsl" => 421, "fud" => 422, "fuf" => 423, "fur" => 424, "fvr" =>
        425, "fy" => 426, "ga" => 427, "gaa" => 428, "gag" => 429, "gah" => 430, "gan" =>
        431, "gan-Hans" => 432, "gan-Hant" => 433, "gay" => 434, "gba" => 435, "gbb" =>
        436, "gbk" => 437, "gbk-Deva" => 438, "gbk-Takr" => 439, "gbm" => 440, "gbz" =>
        441, "gcf" => 442, "gcr" => 443, "gd" => 444, "gem" => 445, "gez" => 446, "gil"
        => 447, "gju" => 448, "gju-Arab" => 449, "gju-Deva" => 450, "gl" => 451, "gld" =>
        452, "glh" => 453, "glk" => 454, "gmh" => 455, "gml" => 456, "gmy" => 457, "gn"
        => 458, "gnq" => 459, "goh" => 460, "gom" => 461, "gom-Deva" => 462, "gom-Latn"
        => 463, "gon" => 464, "gor" => 465, "got" => 466, "gpe" => 467, "grb" => 468,
        "grc" => 469, "gsg" => 470, "gsw-FR" => 472, "gu" => 473, "guc" => 474, "gum" =>
        475, "gur" => 476, "guw" => 477, "guz" => 478, "gv" => 479, "gwi" => 480, "gya"
        => 481, "ha" => 482, "ha-Arab" => 483, "ha-Latn" => 484, "ha-NE" => 485, "hac" =>
        486, "hai" => 487, "hak" => 488, "hak-Hans" => 489, "hak-Hant" => 490, "hak-Latn"
        => 491, "hav" => 492, "haw" => 493, "hax" => 494, "haz" => 495, "hbo" => 496,
        "he" => 497, "hea" => 498, "hi" => 499, "hi-Kthi" => 500, "hi-Latn" => 501, "hif"
        => 502, "hif-Deva" => 503, "hif-Latn" => 504, "hil" => 505, "him" => 506, "hit"
        => 507, "hit-Latn" => 508, "hit-Xsux" => 509, "hke" => 510, "hmn" => 511, "hne"
        => 512, "hnj" => 513, "hno" => 514, "ho" => 515, "hoc" => 516, "hoc-Latn" => 517,
        "hr" => 518, "hrx" => 519, "hsb" => 520, "hsn" => 521, "hsn-Hans" => 522,
        "hsn-Hant" => 523, "ht" => 524, "hts" => 525, "hu" => 526, "hu-x-formal" => 527,
        "hup" => 528, "hur" => 529, "hy" => 530, "hyw" => 531, "hz" => 532, "ia" => 533,
        "iba" => 534, "ibb" => 535, "id" => 536, "ie" => 537, "ifu" => 538, "ig" => 539,
        "igb" => 540, "igl" => 541, "ii" => 542, "ijo" => 543, "ik" => 544, "ike-Cans" =>
        545, "ike-Latn" => 546, "ikt" => 547, "ilo" => 548, "inc" => 549, "ine" => 550,
        "inh" => 551, "io" => 552, "ira" => 553, "iro" => 554, "is" => 555, "ish" => 556,
        "isk-Arab" => 557, "isk-Cyrl" => 558, "isk-Latn" => 559, "ist" => 560, "isu" =>
        561, "isv" => 562, "isv-Cyrl" => 563, "isv-Latn" => 564, "it" => 565, "iu" =>
        566, "ivb" => 567, "izh" => 568, "ja" => 569, "ja-Hani" => 570, "ja-Hira" => 571,
        "ja-Hrkt" => 572, "ja-Kana" => 573, "jac" => 574, "jak" => 575, "jam" => 576,
        "jax" => 577, "jbo" => 578, "jdt" => 579, "jdt-Cyrl" => 580, "jgo" => 581, "jje"
        => 582, "jmc" => 583, "jpr" => 584, "jrb" => 585, "juk" => 586, "jut" => 587,
        "jv" => 588, "jv-Java" => 589, "ka" => 590, "kaa" => 591, "kab" => 592, "kac" =>
        593, "kag" => 594, "kai" => 595, "kaj" => 596, "kam" => 597, "kar" => 598, "kaw"
        => 599, "kbd" => 600, "kbd-Cyrl" => 601, "kbd-Latn" => 602, "kbl" => 603, "kbp"
        => 604, "kcg" => 605, "kck" => 606, "kde" => 607, "kea" => 608, "kek" => 609,
        "ken" => 610, "ker" => 611, "kfo" => 612, "kfr" => 613, "kg" => 614, "kge" =>
        615, "kge-Arab" => 616, "kgg" => 617, "kgp" => 618, "kha" => 619, "khi" => 620,
        "kho" => 621, "khq" => 622, "khw" => 623, "ki" => 624, "kip" => 625, "kiu" =>
        626, "kj" => 627, "kjh" => 628, "kjp" => 629, "kk" => 630, "kk-Arab" => 631,
        "kk-Arab-CN" => 632, "kk-Cyrl" => 633, "kk-KZ" => 634, "kk-Latn" => 635,
        "kk-Latn-TR" => 636, "kkj" => 637, "kl" => 638, "kld" => 639, "kln" => 640, "kls"
        => 641, "kls-Arab" => 642, "kls-Latn" => 643, "km" => 644, "kmb" => 645, "kmr" =>
        646, "kmr-Arab" => 647, "kmr-Latn" => 648, "kmz" => 649, "kn" => 650, "knc" =>
        651, "kne" => 652, "knn" => 653, "knq" => 654, "ko" => 655, "ko-CN" => 656,
        "ko-Hani" => 657, "ko-Kore" => 658, "ko-KP" => 659, "koi" => 660, "kok" => 661,
        "kos" => 662, "koy" => 663, "kpe" => 664, "kqr" => 665, "kqt" => 666, "kqv" =>
        667, "kr" => 668, "krc" => 669, "kri" => 670, "krj" => 671, "krl" => 672, "kro"
        => 673, "kru" => 674, "ks" => 675, "ks-Arab" => 676, "ks-Deva" => 677, "ksb" =>
        678, "ksf" => 679, "ksh" => 680, "ksw" => 681, "ksy-Beng" => 682, "ku" => 683,
        "ku-Arab" => 684, "ku-Latn" => 685, "kum" => 686, "kus" => 687, "kut" => 688,
        "kv" => 689, "kve" => 690, "kw" => 691, "kwk" => 692, "kxd" => 693, "kxi" => 694,
        "kxn" => 695, "kxv" => 696, "ky" => 697, "kyw-Beng" => 698, "kyw-Deva" => 699,
        "la" => 700, "lad" => 701, "lad-Hebr" => 702, "lad-Latn" => 703, "lag" => 704,
        "lah" => 705, "laj" => 706, "lam" => 707, "lb" => 708, "lbe" => 709, "lcm" =>
        710, "ldn" => 711, "lem" => 712, "lez" => 713, "lfn" => 714, "lg" => 715, "li" =>
        716, "li-BE" => 717, "li-NL" => 718, "lij" => 719, "lij-MC" => 720, "lil" => 721,
        "liv" => 722, "ljp" => 723, "lki" => 724, "lkt" => 725, "lld" => 726, "lmn" =>
        727, "lmo" => 728, "ln" => 729, "lns" => 730, "lo" => 731, "lol" => 732, "lou" =>
        733, "loz" => 734, "lrc" => 735, "lsm" => 736, "lt" => 737, "ltg" => 738, "lu" =>
        739, "lua" => 740, "lud" => 741, "lui" => 742, "lun" => 743, "luo" => 744, "lus"
        => 745, "lut" => 746, "luy" => 747, "luz" => 748, "lv" => 749, "lzh" => 750,
        "lzz" => 751, "mad" => 752, "maf" => 753, "mag" => 754, "mai" => 755, "mak" =>
        756, "mak-Bugi" => 757, "man" => 758, "map" => 759, "jv-x-bms" => 760, "mas" =>
        761, "maw" => 762, "mcn" => 763, "mcp" => 764, "mde" => 765, "mdf" => 766, "mdh"
        => 767, "mdr" => 768, "men" => 769, "mer" => 770, "mey" => 771, "mfa" => 772,
        "mfe" => 773, "mg" => 774, "mga" => 775, "mgh" => 776, "mgo" => 777, "mh" => 778,
        "mhk" => 779, "mhn" => 780, "mhr" => 781, "mi" => 782, "mic" => 783, "mid" =>
        784, "min" => 785, "miq" => 786, "mis" => 787, "mix" => 788, "mjx-Beng" => 789,
        "mk" => 790, "mkh" => 791, "ml" => 792, "mn" => 793, "mn-Cyrl" => 794, "mn-Mong"
        => 795, "mnc" => 796, "mnc-Latn" => 797, "mnc-Mong" => 798, "mni" => 799,
        "mni-Beng" => 800, "mnj" => 801, "mno" => 802, "mnq" => 803, "mns" => 804, "mnw"
        => 805, "ro-Cyrl-MD" => 806, "moe" => 807, "moh" => 808, "mos" => 809, "mr" =>
        810, "mr-Modi" => 811, "mrh" => 812, "mrj" => 813, "mrt" => 814, "mrv" => 815,
        "ms" => 816, "ms-Arab" => 817, "msi" => 818, "mt" => 819, "mua" => 820, "mui" =>
        821, "mul" => 822, "mun" => 823, "mus" => 824, "mvf" => 825, "mvi" => 826,
        "mvi-Hira" => 827, "mvv" => 828, "mwl" => 829, "mwr" => 830, "mwv" => 831, "mww"
        => 832, "mww-Latn" => 833, "my" => 834, "mye" => 835, "myn" => 836, "myv" => 837,
        "mzn" => 838, "na" => 839, "nah" => 840, "nai" => 841, "nan" => 842, "nan-Hani"
        => 843, "nan-Hans" => 844, "nan-Hant" => 845, "nan-Latn-pehoeji" => 846,
        "nan-Latn-tailo" => 847, "nap" => 848, "naq" => 849, "nb" => 850, "nd" => 851,
        "nds" => 852, "nds-NL" => 853, "ne" => 854, "new" => 855, "ng" => 856, "nge" =>
        857, "nia" => 858, "nic" => 859, "nit" => 860, "niu" => 861, "njo" => 862, "nl"
        => 863, "nl-BE" => 864, "nl-x-informal" => 865, "nla" => 866, "nmg" => 867, "nmz"
        => 868, "nn" => 869, "nn-hognorsk" => 870, "nnh" => 871, "nnz" => 872, "no" =>
        873, "nod" => 874, "nod-Thai" => 875, "nog" => 876, "non" => 877, "non-Runr" =>
        878, "nov" => 879, "nqo" => 880, "nr" => 881, "nrf-GG" => 882, "nrf-JE" => 883,
        "nrf" => 884, "nsk" => 885, "nsl" => 886, "nso" => 887, "ntd" => 888, "nub" =>
        889, "nup" => 890, "nus" => 891, "nv" => 892, "nwc" => 893, "nxm" => 894, "ny" =>
        895, "nym" => 896, "nyn" => 897, "nyo" => 898, "nys" => 899, "nzi" => 900, "obt"
        => 901, "oc" => 902, "oco" => 903, "odt" => 904, "ofs" => 905, "oj" => 906, "ojb"
        => 907, "ojc" => 908, "ojp" => 909, "ojp-Hani" => 910, "ojp-Hira" => 911, "ojs"
        => 912, "ojw" => 913, "oka" => 914, "olo" => 915, "om" => 916, "oma" => 917,
        "ood" => 918, "or" => 919, "os" => 920, "osa" => 921, "osa-Latn" => 922, "osi" =>
        923, "osx" => 924, "ota" => 925, "otk" => 926, "oto" => 927, "ovd" => 928, "owl"
        => 929, "pa" => 930, "pa-Guru" => 931, "paa" => 932, "pag" => 933, "pal" => 934,
        "pal-Phli" => 935, "pal-Phlp" => 936, "pal-Phlv" => 937, "pam" => 938, "pao" =>
        939, "pap" => 940, "pap-AW" => 941, "paq" => 942, "pau" => 943, "pbb" => 944,
        "pcd" => 945, "pcm" => 946, "pdc" => 947, "pdt" => 948, "peo" => 949, "pfl" =>
        950, "pgd" => 951, "pgd-Arab" => 952, "pgd-Deva" => 953, "pgd-Khar" => 954, "pgl"
        => 955, "phi" => 956, "phl" => 957, "phn" => 958, "phn-Latn" => 959, "phn-Phnx"
        => 960, "phr" => 961, "pi" => 962, "pi-Sidd" => 963, "pih" => 964, "pis" => 965,
        "pjt" => 966, "pkc" => 967, "pko" => 968, "pks" => 969, "pl" => 970, "plv" =>
        971, "plw" => 972, "pms" => 973, "pnb" => 974, "pnt" => 975, "pon" => 976, "pov"
        => 977, "ppl" => 978, "ppu" => 979, "pqm" => 980, "pra" => 981, "prc" => 982,
        "prg" => 983, "pro" => 984, "prs" => 985, "ps" => 986, "ps-AF" => 987, "ps-PK" =>
        988, "psh" => 989, "psi" => 990, "psu" => 991, "psu-Arab" => 992, "psu-Brah" =>
        993, "psu-Deva" => 994, "psu-Guru" => 995, "pt" => 996, "pt-ao1990" => 997,
        "pt-BR" => 998, "pt-colb1945" => 999, "pt-PT" => 1000, "pwn" => 1001, "pwo" =>
        1002, "pyu" => 1003, "qu" => 1004, "quc" => 1005, "qug" => 1006, "qwh" => 1007,
        "qxp" => 1008, "qxq" => 1009, "qya" => 1010, "rag" => 1011, "rah" => 1012, "raj"
        => 1013, "rap" => 1014, "rar" => 1015, "rcf" => 1016, "rej" => 1017, "rgn" =>
        1018, "rhg" => 1019, "rhg-Arab" => 1020, "rhg-Rohg" => 1021, "rif" => 1022, "rki"
        => 1023, "rkt" => 1024, "rm" => 1025, "rm-puter" => 1026, "rm-rumgr" => 1027,
        "rm-surmiran" => 1028, "rm-sursilv" => 1029, "rm-sutsilv" => 1030, "rm-vallader"
        => 1031, "rmc" => 1032, "rmf" => 1033, "rmg" => 1034, "rml" => 1035, "rml-Cyrl"
        => 1036, "rmn" => 1037, "rmo" => 1038, "rmw" => 1039, "rmy" => 1040, "rn" =>
        1041, "ro" => 1042, "ro-MD" => 1043, "roa" => 1044, "rup" => 1045, "nap-x-tara"
        => 1046, "rof" => 1047, "rom" => 1048, "rsk" => 1049, "rtm" => 1050, "ru" =>
        1051, "ru-petr1708" => 1052, "rue" => 1053, "rug" => 1054, "ruo" => 1055, "ruq"
        => 1057, "ruq-Cyrl" => 1058, "ruq-Latn" => 1059, "rut" => 1060, "rw" => 1061,
        "rwk" => 1062, "rwr" => 1063, "rys" => 1064, "rys-Hira" => 1065, "ryu" => 1066,
        "ryu-Hira" => 1067, "sa" => 1068, "sa-Sidd" => 1069, "sad" => 1070, "sah" =>
        1071, "sai" => 1072, "sal" => 1073, "sam" => 1074, "saq" => 1075, "sas" => 1076,
        "sat" => 1077, "sat-Beng" => 1078, "sat-Latn" => 1079, "sat-Orya" => 1080, "saz"
        => 1081, "sba" => 1082, "sbp" => 1083, "sc" => 1084, "scl" => 1085, "scn" =>
        1086, "sco" => 1087, "sd" => 1088, "sd-Deva" => 1089, "sd-Gujr" => 1090,
        "sd-Khoj" => 1091, "sd-Sind" => 1092, "sdc" => 1093, "sdh" => 1094, "sdh-Arab" =>
        1095, "sdh-Latn" => 1096, "sdo" => 1097, "se" => 1098, "se-FI" => 1099, "se-NO"
        => 1100, "se-SE" => 1101, "sea" => 1102, "see" => 1103, "seh" => 1104, "sei" =>
        1105, "sel" => 1106, "sem" => 1107, "ser" => 1108, "ses" => 1109, "sg" => 1110,
        "sga" => 1111, "sgh" => 1112, "sgh-Arab" => 1113, "sgh-Cyrl" => 1114, "sgh-Latn"
        => 1115, "sgn" => 1116, "sgy-Arab" => 1118, "sgy-Latn" => 1119, "sh" => 1120,
        "sh-Cyrl" => 1121, "sh-Latn" => 1122, "shd" => 1123, "shi" => 1124, "shi-Latn" =>
        1125, "shi-Tfng" => 1126, "shn" => 1127, "shu" => 1128, "shy" => 1129, "shy-Arab"
        => 1130, "shy-Latn" => 1131, "shy-Tfng" => 1132, "si" => 1133, "sia" => 1134,
        "sid" => 1135, "sio" => 1137, "sit" => 1138, "sjd" => 1139, "sje" => 1140, "sjk"
        => 1141, "sjn" => 1142, "sjo" => 1143, "sjt" => 1144, "sju" => 1145, "sk" =>
        1146, "skr" => 1147, "skr-Arab" => 1148, "sl" => 1149, "sla" => 1150, "slh" =>
        1151, "sli" => 1152, "slr" => 1153, "sly" => 1154, "sm" => 1155, "sma" => 1156,
        "smi" => 1157, "smj" => 1158, "smn" => 1159, "sms" => 1160, "sn" => 1161, "sne"
        => 1162, "snk" => 1163, "so" => 1164, "sog" => 1165, "son" => 1166, "spv" =>
        1167, "sq" => 1168, "sr" => 1169, "sr-Cyrl" => 1170, "sr-Latn" => 1172, "sr-ME"
        => 1174, "srh-Arab" => 1175, "srh-Cyrl" => 1176, "srh-Latn" => 1177, "srk" =>
        1178, "srn" => 1179, "sro" => 1180, "srq" => 1181, "srr" => 1182, "ss" => 1183,
        "ssa" => 1184, "ssb" => 1185, "ssf" => 1186, "ssy" => 1187, "st" => 1188, "sth"
        => 1189, "stq" => 1190, "str" => 1191, "sty" => 1192, "su" => 1193, "suk" =>
        1194, "sus" => 1195, "sux" => 1196, "sux-Latn" => 1197, "sux-Xsux" => 1198, "suz"
        => 1199, "sv" => 1200, "sva" => 1201, "sw" => 1202, "sw-CD" => 1203, "swb" =>
        1204, "sxr" => 1205, "sxu" => 1206, "syc" => 1207, "syl" => 1208, "syl-Beng" =>
        1209, "syl-Sylo" => 1210, "syr" => 1211, "szl" => 1212, "szy" => 1213, "ta" =>
        1214, "tai" => 1215, "tao" => 1216, "tay" => 1217, "tbl" => 1218, "tce" => 1219,
        "tcy" => 1220, "tdd" => 1221, "te" => 1222, "tem" => 1223, "teo" => 1224, "ter"
        => 1225, "tet" => 1226, "tg" => 1227, "tg-Cyrl" => 1228, "tg-Latn" => 1229, "tgx"
        => 1230, "th" => 1231, "thq" => 1232, "thr" => 1233, "tht" => 1234, "ti" => 1235,
        "tig" => 1236, "tih" => 1237, "tiv" => 1238, "tji" => 1239, "tk" => 1240, "tkl"
        => 1241, "tkr" => 1242, "tl" => 1243, "tlb" => 1244, "tlh" => 1245, "tlh-Latn" =>
        1246, "tlh-Piqd" => 1247, "tli" => 1248, "tly" => 1249, "tly-Cyrl" => 1250, "tmh"
        => 1251, "tmr" => 1252, "tn" => 1253, "tnq" => 1254, "to" => 1255, "tog" => 1256,
        "toi" => 1257, "tok" => 1258, "tpi" => 1259, "tr" => 1260, "trp" => 1261, "tru"
        => 1262, "trv" => 1263, "trw" => 1264, "ts" => 1265, "tsd" => 1266, "tsg" =>
        1267, "tsi" => 1268, "tsu" => 1269, "tt" => 1270, "tt-Cyrl" => 1271, "tt-Latn" =>
        1272, "ttj" => 1273, "ttm" => 1274, "ttt" => 1275, "tui" => 1276, "tum" => 1277,
        "tup" => 1278, "tut" => 1279, "tvl" => 1280, "tvu" => 1281, "tw" => 1282, "twd"
        => 1283, "twq" => 1284, "txa" => 1285, "txg" => 1286, "txo-Beng" => 1287,
        "txo-Toto" => 1288, "txx" => 1289, "ty" => 1290, "tyv" => 1291, "tzl" => 1292,
        "tzm" => 1293, "udm" => 1294, "ug" => 1295, "ug-Arab" => 1296, "ug-Cyrl" => 1297,
        "ug-Latn" => 1298, "uga" => 1299, "uk" => 1300, "ulc" => 1301, "uln" => 1302,
        "umb" => 1303, "umu" => 1304, "und" => 1305, "unr" => 1306, "unr-Deva" => 1307,
        "unr-Nagm" => 1308, "ur" => 1309, "urk" => 1310, "ush" => 1311, "uun" => 1312,
        "uz" => 1313, "uz-Cyrl" => 1314, "uz-Latn" => 1315, "vai" => 1316, "ve" => 1317,
        "vec" => 1318, "vep" => 1319, "vi" => 1320, "vi-Hani" => 1321, "vls" => 1322,
        "vmf" => 1323, "vmw" => 1324, "vo" => 1325, "vot" => 1326, "vun" => 1328, "vut"
        => 1329, "wa" => 1330, "wae" => 1331, "wak" => 1332, "wal" => 1333, "war" =>
        1334, "was" => 1335, "wbl-Arab" => 1336, "wbl-Arab-AF" => 1337, "wbl-Arab-CN" =>
        1338, "wbl-Arab-PK" => 1339, "wbl-Cyrl" => 1340, "wbl-Latn" => 1341, "wbp" =>
        1342, "wen" => 1343, "wes" => 1344, "wlm" => 1345, "wls" => 1346, "wlx" => 1347,
        "wo" => 1348, "wsg" => 1349, "wsv" => 1350, "wuu" => 1351, "wuu-Hans" => 1352,
        "wuu-Hant" => 1353, "wya" => 1354, "wyi" => 1355, "xal" => 1356, "xbm" => 1357,
        "xh" => 1358, "xmf" => 1359, "xmm" => 1360, "xnb" => 1361, "xno" => 1362, "xnr"
        => 1363, "xnr-Deva" => 1364, "xnr-Takr" => 1365, "xog" => 1366, "xon" => 1367,
        "xpu" => 1368, "xsu" => 1369, "xsy" => 1370, "yah-Cyrl" => 1371, "yah-Latn" =>
        1372, "yai-Cyrl" => 1373, "yai-Latn" => 1374, "yao" => 1375, "yap" => 1376, "yas"
        => 1377, "yat" => 1378, "yav" => 1379, "ybb" => 1380, "ydd" => 1381, "ydg" =>
        1382, "yec" => 1383, "yi" => 1384, "ykg" => 1385, "yo" => 1386, "yoi" => 1387,
        "yoi-Hira" => 1388, "yox" => 1389, "yox-Hira" => 1390, "ypk" => 1391, "yrk" =>
        1392, "yrl" => 1393, "yua" => 1394, "yue" => 1395, "yue-Hans" => 1396, "yue-Hant"
        => 1397, "za" => 1398, "zai" => 1399, "zap" => 1400, "zbl" => 1401, "zea" =>
        1402, "zen" => 1403, "zgh" => 1404, "zgh-Latn" => 1405, "zh" => 1406,
        "zh-Hans-CN" => 1408, "zh-Hans" => 1409, "zh-Hant" => 1410, "zh-Hant-HK" => 1411,
        "zh-Hant-MO" => 1413, "zh-Hans-MY" => 1414, "zh-Hans-SG" => 1415, "zh-Hant-TW" =>
        1416, "zmi" => 1418, "znd" => 1419, "zpu" => 1420, "zu" => 1421, "zun" => 1422,
        "zxx" => 1423, "zza" => 1424
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
    languages: phf::phf_ordered_map! {
        "aa" => Language { autonym : "Qafár af", is_enabled : true, is_rtl : false, name
        : "Afar", }, "aae" => Language { autonym : "Arbërisht", is_enabled : true,
        is_rtl : false, name : "Arbëresh", }, "ab" => Language { autonym :
        "аԥсшәа", is_enabled : true, is_rtl : false, name : "Abkhazian", }, "abe"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Western Abenaki", }, "abq" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Abaza", }, "abq-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Abaza", }, "abr" => Language {
        autonym : "Abron", is_enabled : true, is_rtl : false, name : "Abron", }, "abs" =>
        Language { autonym : "bahasa ambon", is_enabled : true, is_rtl : false, name :
        "Ambonese Malay", }, "ace" => Language { autonym : "Acèh", is_enabled : true,
        is_rtl : false, name : "Acehnese", }, "acf" => Language { autonym :
        "Kwéyòl Sent Lisi", is_enabled : true, is_rtl : false, name :
        "Saint Lucian Creole", }, "ach" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Acoli", }, "acm" => Language { autonym : "عراقي",
        is_enabled : true, is_rtl : true, name : "Iraqi Arabic", }, "ada" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Adangme", }, "adg" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Andegerebinha", }, "ady" => Language { autonym : "адыгабзэ", is_enabled
        : true, is_rtl : false, name : "Adyghe", }, "ady-cyrl" => Language { autonym :
        "адыгабзэ", is_enabled : true, is_rtl : false, name :
        "Adyghe (Cyrillic script)", }, "ady-latn" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Adyghe (Latin script)", }, "ae" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Avestan", }, "aeb" =>
        Language { autonym : "تونسي / Tûnsî", is_enabled : true, is_rtl : true,
        name : "Tunisian Arabic", }, "aeb-arab" => Language { autonym : "تونسي",
        is_enabled : true, is_rtl : true, name : "Tunisian Arabic (Arabic script)", },
        "aeb-latn" => Language { autonym : "Tûnsî", is_enabled : true, is_rtl : false,
        name : "Tunisian Arabic (Latin script)", }, "aec" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Saʽidi Arabic", }, "aee" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Northeast Pashai", },
        "aer" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Eastern Arrernte", }, "af" => Language { autonym : "Afrikaans", is_enabled :
        true, is_rtl : false, name : "Afrikaans", }, "afa" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Afroasiatic languages", }, "afh" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Afrihili",
        }, "agq" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Aghem", }, "aha" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Ahanta", }, "ahr" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Ahirani", }, "aig" => Language { autonym :
        "Aanteegan an' Baabyuudan", is_enabled : true, is_rtl : false, name :
        "Antiguan and Barbudan Creole English", }, "aii" => Language { autonym : "",
        is_enabled : false, is_rtl : true, name : "Assyrian Neo-Aramaic", }, "ain" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Ainu", },
        "ajg" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Ajagbe", }, "ajp" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "South Levantine Arabic", }, "ajp-arab" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name :
        "South Levantine Arabic (Arabic script)", }, "ajp-latn" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name :
        "South Levantine Arabic (Latin script)", }, "ak" => Language { autonym : "Akan",
        is_enabled : true, is_rtl : false, name : "Akan", }, "akb" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Batak Angkola", }, "akk" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Akkadian",
        }, "akk-latn" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Akkadian (Latin script)", }, "akk-xsux" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Akkadian (Cuneiform script)", },
        "akz" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Alabama", }, "alc" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kawésqar", }, "ale" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Aleut", }, "ale-cyrl" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Aleut (Cyrillic script)", }, "alg" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Algonquian languages", }, "aln" => Language { autonym : "Gegë", is_enabled :
        true, is_rtl : false, name : "Gheg Albanian", }, "alq" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Algonquin", }, "als" => Language
        { autonym : "Alemannisch", is_enabled : true, is_rtl : false, name : "Alemannic",
        }, "alt" => Language { autonym : "алтай тил", is_enabled : true, is_rtl :
        false, name : "Southern Altai", }, "aly" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Alyawarr", }, "am" => Language { autonym :
        "አማርኛ", is_enabled : true, is_rtl : false, name : "Amharic", }, "ami" =>
        Language { autonym : "Pangcah", is_enabled : true, is_rtl : false, name : "Amis",
        }, "amx" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Anmatyerre", }, "an" => Language { autonym : "aragonés", is_enabled : true,
        is_rtl : false, name : "Aragonese", }, "ane" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Xârâcùù", }, "ang" => Language {
        autonym : "Ænglisc", is_enabled : true, is_rtl : false, name : "Old English", },
        "ann" => Language { autonym : "Obolo", is_enabled : true, is_rtl : false, name :
        "Obolo", }, "anp" => Language { autonym : "अ\u{902}गिका", is_enabled :
        true, is_rtl : false, name : "Angika", }, "apa" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Southern Athabaskan", }, "apc" =>
        Language { autonym : "شامي", is_enabled : true, is_rtl : true, name :
        "Levantine Arabic", }, "apc-arab" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Levantine Arabic (Arabic script)", }, "apc-latn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Levantine Arabic (Latin script)", }, "apw" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Western Apache", }, "ar" => Language
        { autonym : "العربية", is_enabled : true, is_rtl : true, name : "Arabic",
        }, "ar-001" => Language { autonym : "", is_enabled : false, is_rtl : false, name
        : "Modern Standard Arabic", }, "arc" => Language { autonym : "ܐܪܡܝܐ",
        is_enabled : true, is_rtl : true, name : "Aramaic", }, "are" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Western Arrarnta", },
        "arn" => Language { autonym : "mapudungun", is_enabled : true, is_rtl : false,
        name : "Mapuche", }, "aro" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Araona", }, "arp" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Arapaho", }, "arq" => Language { autonym :
        "جازايرية", is_enabled : true, is_rtl : true, name : "Algerian Arabic",
        }, "ars" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Najdi Arabic", }, "art" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "constructed languages", }, "arw" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Arawak", }, "ary" => Language {
        autonym : "الدارجة", is_enabled : true, is_rtl : true, name :
        "Moroccan Arabic", }, "ary-arab" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Moroccan Arabic (Arabic script)", }, "ary-latn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Moroccan Arabic (Latin script)", }, "arz" => Language { autonym : "مصرى",
        is_enabled : true, is_rtl : true, name : "Egyptian Arabic", }, "as" => Language {
        autonym : "অসমীয\u{9bc}\u{9be}", is_enabled : true, is_rtl : false,
        name : "Assamese", }, "asa" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Asu", }, "ase" => Language { autonym :
        "American sign language", is_enabled : true, is_rtl : false, name :
        "American Sign Language", }, "ast" => Language { autonym : "asturianu",
        is_enabled : true, is_rtl : false, name : "Asturian", }, "ath" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Athabaskan languages",
        }, "atj" => Language { autonym : "Atikamekw", is_enabled : true, is_rtl : false,
        name : "Atikamekw", }, "atv" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Northern Altai", }, "aus" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Australian Aboriginal languages", },
        "av" => Language { autonym : "авар", is_enabled : true, is_rtl : false, name
        : "Avaric", }, "avk" => Language { autonym : "Kotava", is_enabled : true, is_rtl
        : false, name : "Kotava", }, "awa" => Language { autonym : "अवधी",
        is_enabled : true, is_rtl : false, name : "Awadhi", }, "axe" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Ayerrerenge", }, "axl"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Lower Southern Aranda", }, "ay" => Language { autonym : "Aymar aru", is_enabled
        : true, is_rtl : false, name : "Aymara", }, "ayh" => Language { autonym : "",
        is_enabled : false, is_rtl : true, name : "Hadhrami Arabic", }, "az" => Language
        { autonym : "azərbaycanca", is_enabled : true, is_rtl : false, name :
        "Azerbaijani", }, "az-arab" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Azerbaijani (Arabic script)", }, "az-cyrl" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Azerbaijani (Cyrillic script)", }, "az-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Azerbaijani (Latin script)", }, "azb"
        => Language { autonym : "تۆرکجه", is_enabled : true, is_rtl : true, name :
        "South Azerbaijani", }, "azj" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "North Azerbaijani", }, "ba" => Language { autonym :
        "башҡортса", is_enabled : true, is_rtl : false, name : "Bashkir", },
        "bad" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Banda languages", }, "bag" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Tuki", }, "bai" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Bamileke languages", }, "bal" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Baluchi", }, "bal-latn"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Baluchi (Latin script)", }, "ban" => Language { autonym : "Basa Bali",
        is_enabled : true, is_rtl : false, name : "Balinese", }, "ban-bali" => Language {
        autonym : "ᬩᬲᬩᬮ\u{1b36}", is_enabled : true, is_rtl : false, name :
        "Balinese (Balinese script)", }, "bar" => Language { autonym : "Boarisch",
        is_enabled : true, is_rtl : false, name : "Bavarian", }, "bas" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Basaa", }, "bat" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Baltic languages", }, "bat-smg" => Language { autonym : "žemaitėška",
        is_enabled : true, is_rtl : false, name : "Samogitian", }, "bax" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Bamun", }, "bbc" =>
        Language { autonym : "Batak Toba", is_enabled : true, is_rtl : false, name :
        "Batak Toba", }, "bbc-batk" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Batak Toba (Batak script)", }, "bbc-latn" => Language {
        autonym : "Batak Toba", is_enabled : true, is_rtl : false, name :
        "Batak Toba (Latin script)", }, "bbj" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Ghomala", }, "bcc" => Language { autonym :
        "جهلسری بلوچی", is_enabled : true, is_rtl : true, name :
        "Southern Balochi", }, "bci" => Language { autonym : "wawle", is_enabled : true,
        is_rtl : false, name : "Baoulé", }, "bcl" => Language { autonym :
        "Bikol Central", is_enabled : true, is_rtl : false, name : "Central Bikol", },
        "bdr" => Language { autonym : "Bajau Sama", is_enabled : true, is_rtl : false,
        name : "West Coast Bajau", }, "be" => Language { autonym :
        "беларуская", is_enabled : true, is_rtl : false, name : "Belarusian",
        }, "be-tarask" => Language { autonym :
        "беларуская (тарашкевіца)", is_enabled : true, is_rtl :
        false, name : "Belarusian (Taraškievica orthography)", }, "be-x-old" => Language
        { autonym : "беларуская (тарашкевіца)", is_enabled : true,
        is_rtl : false, name : "Belarusian (Taraškievica orthography)", }, "bej" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Beja", },
        "bem" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Bemba", }, "ber" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Berber languages", }, "bew" => Language { autonym : "Betawi", is_enabled
        : true, is_rtl : false, name : "Betawi", }, "bez" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Bena", }, "bfa" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Bari", }, "bfd" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Bafut", }, "bfi" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "British Sign Language", }, "bfq" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Badaga", }, "bft" => Language { autonym : "", is_enabled
        : false, is_rtl : true, name : "Balti", }, "bft-tibt" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Balti (Tibetan script)", }, "bfw" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Bonda", },
        "bfz" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mahasu Pahari", }, "bfz-deva" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Mahasu Pahari (Devanagari script)", }, "bfz-takr" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mahasu Pahari (Takri script)", }, "bg" => Language { autonym :
        "български", is_enabled : true, is_rtl : false, name : "Bulgarian", },
        "bgc" => Language { autonym : "हरियाणवी", is_enabled : true,
        is_rtl : false, name : "Haryanvi", }, "bgc-arab" => Language { autonym : "",
        is_enabled : false, is_rtl : true, name : "Haryanvi (Arabic script)", },
        "bgc-deva" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Haryanvi (Devanagari script)", }, "bgn" => Language { autonym :
        "روچ کپتین بلوچی", is_enabled : true, is_rtl : true, name :
        "Western Balochi", }, "bgp" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Eastern Balochi", }, "bgq" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Bagri", }, "bgq-arab" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Bagri (Arabic script)",
        }, "bgq-deva" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Bagri (Devanagari script)", }, "bh" => Language { autonym :
        "भोजप\u{941}री", is_enabled : true, is_rtl : false, name :
        "Bhojpuri", }, "bha" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Bharia", }, "bhd" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Bhadrawahi", }, "bhd-deva" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Bhadrawahi (Devanagari script)", },
        "bhd-takr" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Bhadrawahi (Takri script)", }, "bho" => Language { autonym :
        "भोजप\u{941}री", is_enabled : true, is_rtl : false, name :
        "Bhojpuri", }, "bi" => Language { autonym : "Bislama", is_enabled : true, is_rtl
        : false, name : "Bislama", }, "bik" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Bikol", }, "bin" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Bini", }, "bjn" => Language { autonym
        : "Banjar", is_enabled : true, is_rtl : false, name : "Banjar", }, "bkc" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Baka", },
        "bkh" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Bakoko", }, "bkm" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kom", }, "bkn" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Bukitan", }, "bla" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Siksiká", }, "blc" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Nuxalk", }, "blk" => Language {
        autonym : "ပအ\u{102d}\u{102f}ဝ\u{103a}ႏဘာႏသာႏ", is_enabled :
        true, is_rtl : false, name : "Pa'O", }, "blo" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Anii", }, "blt" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Tai Dam", }, "bm" => Language {
        autonym : "bamanankan", is_enabled : true, is_rtl : false, name : "Bambara", },
        "bn" => Language { autonym : "ব\u{9be}ংল\u{9be}", is_enabled : true, is_rtl
        : false, name : "Bangla", }, "bnb" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Bookan", }, "bnn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Bunun", }, "bnt" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Bantu languages", },
        "bny" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Bintulu", }, "bo" => Language { autonym : "བ\u{f7c}ད་ཡ\u{f72}ག",
        is_enabled : true, is_rtl : false, name : "Tibetan", }, "bol" => Language {
        autonym : "bòo pìkkà", is_enabled : true, is_rtl : false, name : "Bole", },
        "bom" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Berom", }, "bpy" => Language { autonym :
        "বিষ\u{9cd}ণ\u{9c1}প\u{9cd}রিয\u{9bc}\u{9be} মণিপ\u{9c1}রী",
        is_enabled : true, is_rtl : false, name : "Bishnupriya", }, "bqi" => Language {
        autonym : "بختیاری", is_enabled : true, is_rtl : true, name : "Bakhtiari",
        }, "bqz" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mka'a", }, "br" => Language { autonym : "brezhoneg", is_enabled : true, is_rtl :
        false, name : "Breton", }, "bra" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Braj", }, "brh" => Language { autonym : "Bráhuí",
        is_enabled : true, is_rtl : false, name : "Brahui", }, "brh-latn" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Brahui (Latin script)",
        }, "brx" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Bodo", }, "bs" => Language { autonym : "bosanski", is_enabled : true, is_rtl :
        false, name : "Bosnian", }, "bse" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Wushi", }, "bsk" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Burushaski", }, "bss" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Akoose", }, "btd" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Batak Dairi", }, "bth"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name : "Biatah",
        }, "btk" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Batak languages", }, "btm" => Language { autonym : "Batak Mandailing",
        is_enabled : true, is_rtl : false, name : "Batak Mandailing", }, "bto" =>
        Language { autonym : "Iriga Bicolano", is_enabled : true, is_rtl : false, name :
        "Rinconada Bikol", }, "bts" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Batak Simalungun", }, "btx" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Batak Karo", }, "btz" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Batak Alas-Kluet", },
        "bua" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Buriat", }, "bug" => Language { autonym : "Basa Ugi", is_enabled : true, is_rtl
        : false, name : "Buginese", }, "bug-bugi" => Language { autonym :
        "ᨅᨔ ᨕ\u{1a18}ᨁ\u{1a17}", is_enabled : true, is_rtl : false, name :
        "Buginese (Buginese script)", }, "bum" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Bulu", }, "bvb" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Bube", }, "bwr" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Bura-Pabir", }, "bxr" =>
        Language { autonym : "буряад", is_enabled : true, is_rtl : false, name :
        "Russia Buriat", }, "byn" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Blin", }, "byv" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Medumba", }, "bzj" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Belize Kriol", }, "bzs" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Brazilian Sign Language", }, "ca"
        => Language { autonym : "català", is_enabled : true, is_rtl : false, name :
        "Catalan", }, "cad" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Caddo", }, "cai" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Mesoamerican languages", }, "cak" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Kaqchikel", }, "cal" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Carolinian", }, "car"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name : "Carib",
        }, "cau" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Caucasian languages", }, "cay" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Cayuga", }, "cbk" => Language { autonym :
        "Chavacano de Zamboanga", is_enabled : false, is_rtl : false, name : "Chavacano",
        }, "cbk-zam" => Language { autonym : "Chavacano de Zamboanga", is_enabled : true,
        is_rtl : false, name : "Chavacano", }, "cch" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Atsam", }, "ccp" => Language {
        autonym : "𑄌𑄋\u{11134}𑄟\u{11133}𑄦", is_enabled : true, is_rtl :
        false, name : "Chakma", }, "ccp-beng" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Chakma (Bengali script)", }, "cdo" => Language {
        autonym : "閩東語 / Mìng-dĕ\u{324}ng-ngṳ\u{304}", is_enabled : true,
        is_rtl : false, name : "Mindong", }, "cdo-hani" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Mindong (Han script)", }, "cdo-hant"
        => Language { autonym : "閩東語（傳統漢字）", is_enabled : true, is_rtl
        : false, name : "Mindong (Traditional Han script)", }, "cdo-latn" => Language {
        autonym : "Mìng-dĕ\u{324}ng-ngṳ\u{304} (Bàng-uâ-cê)", is_enabled : true,
        is_rtl : false, name : "Mindong (Latin script)", }, "cdz-beng" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Koda (Bengali script)",
        }, "ce" => Language { autonym : "нохчийн", is_enabled : true, is_rtl :
        false, name : "Chechen", }, "ceb" => Language { autonym : "Cebuano", is_enabled :
        true, is_rtl : false, name : "Cebuano", }, "cel" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Celtic languages", }, "cgg" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Chiga", },
        "ch" => Language { autonym : "Chamoru", is_enabled : true, is_rtl : false, name :
        "Chamorro", }, "chb" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Chibcha", }, "chg" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Chagatai", }, "chk" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Chuukese", }, "chm" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Mari", }, "chn" =>
        Language { autonym : "chinuk wawa", is_enabled : true, is_rtl : false, name :
        "Chinook Jargon", }, "cho" => Language { autonym : "Chahta anumpa", is_enabled :
        true, is_rtl : false, name : "Choctaw", }, "chp" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Chipewyan", }, "chr" => Language {
        autonym : "ᏣᎳᎩ", is_enabled : true, is_rtl : false, name : "Cherokee", },
        "chy" => Language { autonym : "Tsetsêhestâhese", is_enabled : true, is_rtl :
        false, name : "Cheyenne", }, "cic" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Chickasaw", }, "ciw" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Chippewa", }, "cja" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Western Cham", },
        "cja-arab" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Western Cham (Arabic script)", }, "cja-cham" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Western Cham (Cham script)", },
        "cja-latn" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Western Cham (Latin script)", }, "cjm" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Eastern Cham", }, "cjm-arab" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Eastern Cham (Arabic script)",
        }, "cjm-cham" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Eastern Cham (Cham script)", }, "cjm-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Eastern Cham (Latin script)", },
        "cjy" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Jin", }, "cjy-hans" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Jin (Simplified Han script)", }, "cjy-hant" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Jin (Traditional Han script)",
        }, "ckb" => Language { autonym : "کوردی", is_enabled : true, is_rtl : true,
        name : "Central Kurdish", }, "ckb-arab" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Central Kurdish (Arabic script)", }, "ckb-latn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Central Kurdish (Latin script)", }, "cko" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Anufo", }, "ckt" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Chukchi", }, "ckv" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Kavalan", }, "clc" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Chilcotin",
        }, "cmc" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Chamic languages", }, "cmg" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Classical Mongolian", }, "cnh" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Hakha-Chin", }, "cnr" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Montenegrin", },
        "cnr-cyrl" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Montenegrin (Cyrillic script)", }, "cnr-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Montenegrin (Latin script)", }, "cnx"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Middle Cornish", }, "co" => Language { autonym : "corsu", is_enabled : true,
        is_rtl : false, name : "Corsican", }, "coa" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Cocos Malay", }, "cop" => Language {
        autonym : "ϯⲙⲉⲧⲣⲉⲙⲛ\u{300}ⲭⲏⲙⲓ", is_enabled : true, is_rtl
        : false, name : "Coptic", }, "cpe" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "English-based creole languages", }, "cpf" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "French-based creole languages", }, "cpp" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Portuguese-based creole languages", }, "cps" =>
        Language { autonym : "Capiceño", is_enabled : true, is_rtl : false, name :
        "Capiznon", }, "cpx" => Language { autonym : "莆仙語 / Pó-sing-gṳ\u{302}",
        is_enabled : true, is_rtl : false, name : "Puxian", }, "cpx-hans" => Language {
        autonym : "莆仙语（简体）", is_enabled : true, is_rtl : false, name :
        "Puxian (Simplified Han script)", }, "cpx-hant" => Language { autonym :
        "莆仙語（繁體）", is_enabled : true, is_rtl : false, name :
        "Puxian (Traditional Han script)", }, "cpx-latn" => Language { autonym :
        "Pó-sing-gṳ\u{302} (Báⁿ-uā-ci\u{30d})", is_enabled : true, is_rtl : false,
        name : "Puxian (Latin script)", }, "cr" => Language { autonym :
        "Nēhiyawēwin / ᓀᐦᐃᔭᐍᐏᐣ", is_enabled : true, is_rtl : false, name
        : "Cree", }, "cr-cans" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Cree (Canadian Aboriginal syllabics)", }, "cr-latn" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Cree (Latin script)",
        }, "crb" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Island Carib", }, "crg" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Michif", }, "crh" => Language { autonym : "qırımtatarca",
        is_enabled : true, is_rtl : false, name : "Crimean Tatar", }, "crh-cyrl" =>
        Language { autonym : "къырымтатарджа (Кирилл)", is_enabled :
        true, is_rtl : false, name : "Crimean Tatar (Cyrillic script)", }, "crh-latn" =>
        Language { autonym : "qırımtatarca (Latin)", is_enabled : true, is_rtl : false,
        name : "Crimean Tatar (Latin script)", }, "crh-ro" => Language { autonym :
        "tatarşa", is_enabled : true, is_rtl : false, name : "Dobrujan Tatar", }, "crj"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Southern East Cree", }, "crk" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Plains Cree", }, "crl" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Northern East Cree", }, "crm" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Moose Cree",
        }, "crp" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "creoles and pidgins", }, "crr" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Carolina Algonquian", }, "crs" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Seselwa Creole French", }, "cs"
        => Language { autonym : "čeština", is_enabled : true, is_rtl : false, name :
        "Czech", }, "csb" => Language { autonym : "kaszëbsczi", is_enabled : true,
        is_rtl : false, name : "Kashubian", }, "csw" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Swampy Cree", }, "ctg" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Chittagonian", }, "cu"
        => Language { autonym : "словѣньскъ / ⰔⰎⰑⰂⰡⰐⰠⰔⰍⰟ",
        is_enabled : true, is_rtl : false, name : "Church Slavic", }, "cus" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Cushitic languages", },
        "cv" => Language { autonym : "чӑвашла", is_enabled : true, is_rtl : false,
        name : "Chuvash", }, "cy" => Language { autonym : "Cymraeg", is_enabled : true,
        is_rtl : false, name : "Welsh", }, "da" => Language { autonym : "dansk",
        is_enabled : true, is_rtl : false, name : "Danish", }, "dag" => Language {
        autonym : "dagbanli", is_enabled : true, is_rtl : false, name : "Dagbani", },
        "dak" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Dakota", }, "dar" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Dargwa", }, "dav" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Taita", }, "day" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Land Dayak languages", }, "dbj" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Idaʼan", }, "ddn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Dendi", },
        "de" => Language { autonym : "Deutsch", is_enabled : true, is_rtl : false, name :
        "German", }, "de-1901" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "German (traditional orthography)", }, "de-at" => Language {
        autonym : "Österreichisches Deutsch", is_enabled : true, is_rtl : false, name :
        "Austrian German", }, "de-ch" => Language { autonym : "Schweizer Hochdeutsch",
        is_enabled : true, is_rtl : false, name : "Swiss High German", }, "de-formal" =>
        Language { autonym : "Deutsch (Sie-Form)", is_enabled : true, is_rtl : false,
        name : "German (formal address)", }, "del" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Delaware", }, "den" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Slave", }, "dga" => Language {
        autonym : "Dagaare", is_enabled : true, is_rtl : false, name :
        "Southern Dagaare", }, "dgr" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Dogrib", }, "din" => Language { autonym : "Thuɔŋjäŋ",
        is_enabled : true, is_rtl : false, name : "Dinka", }, "diq" => Language { autonym
        : "Zazaki", is_enabled : true, is_rtl : false, name : "Dimli", }, "dje" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Zarma", },
        "dkr" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kuijau", }, "dlg" => Language { autonym : "долган тыла", is_enabled :
        true, is_rtl : false, name : "Dolgan", }, "dmg" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Upper Kinabatangan", }, "dmv" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Dumpas", },
        "doi" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Dogri", }, "doi-arab" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Dogri (Arabic script)", }, "doi-deva" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Dogri (Devanagari script)", },
        "doi-dogr" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Dogri (Dogra script)", }, "dpp" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Papar", }, "dra" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Dravidian languages", }, "drg" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Rungus", }, "dro" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Daro-Matu",
        }, "dru" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Rukai", }, "dsb" => Language { autonym : "dolnoserbski", is_enabled : true,
        is_rtl : false, name : "Lower Sorbian", }, "dso" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Desiya", }, "dtb" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Eastern Kadazan", },
        "dtp" => Language { autonym : "Kadazandusun", is_enabled : true, is_rtl : false,
        name : "Central Dusun", }, "dtr" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Lotud", }, "dty" => Language { autonym :
        "डोट\u{947}ली", is_enabled : true, is_rtl : false, name : "Doteli", },
        "dua" => Language { autonym : "Duálá", is_enabled : true, is_rtl : false, name
        : "Duala", }, "duf" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Dumbea", }, "dum" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Middle Dutch", }, "dv" => Language { autonym :
        "ދ\u{7a8}ވ\u{7ac}ހ\u{7a8}ބ\u{7a6}ސ\u{7b0}", is_enabled : true, is_rtl :
        true, name : "Divehi", }, "dyo" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Jola-Fonyi", }, "dyu" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Dyula", }, "dz" => Language { autonym
        : "ཇ\u{f7c}ང་ཁ", is_enabled : true, is_rtl : false, name : "Dzongkha", },
        "dzg" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Dazaga", }, "ebu" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Embu", }, "ee" => Language { autonym : "eʋegbe", is_enabled :
        true, is_rtl : false, name : "Ewe", }, "efi" => Language { autonym : "Efịk",
        is_enabled : true, is_rtl : false, name : "Efik", }, "egl" => Language { autonym
        : "emiliàn e rumagnòl", is_enabled : true, is_rtl : false, name :
        "Emiliano-Romagnolo", }, "egy" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Ancient Egyptian", }, "eka" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Ekajuk", }, "ekp" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Ekpeye", }, "el" =>
        Language { autonym : "Ελληνικά", is_enabled : true, is_rtl : false, name
        : "Greek", }, "el-cy" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Cypriot Greek", }, "elm" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Eleme", }, "elx" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Elamite", }, "eml" => Language {
        autonym : "emiliàn e rumagnòl", is_enabled : true, is_rtl : false, name :
        "Emiliano-Romagnolo", }, "en" => Language { autonym : "English", is_enabled :
        true, is_rtl : false, name : "English", }, "en-au" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Australian English", }, "en-ca" =>
        Language { autonym : "Canadian English", is_enabled : true, is_rtl : false, name
        : "Canadian English", }, "en-gb" => Language { autonym : "British English",
        is_enabled : true, is_rtl : false, name : "British English", }, "en-in" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Indian English", }, "en-jm" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Jamaican English", }, "en-nz" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "New Zealand English", }, "en-simple"
        => Language { autonym : "Simple English", is_enabled : false, is_rtl : false,
        name : "Simple English", }, "en-uk" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "British English", }, "en-us" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "American English", }, "enm" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Middle English", }, "eo" => Language { autonym : "Esperanto", is_enabled : true,
        is_rtl : false, name : "Esperanto", }, "eo-hsistemo" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Esperanto (h-system orthography)", },
        "eo-xsistemo" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Esperanto (x-system orthography)", }, "es" => Language { autonym :
        "español", is_enabled : true, is_rtl : false, name : "Spanish", }, "es-419" =>
        Language { autonym : "español de América Latina", is_enabled : true, is_rtl :
        false, name : "Latin American Spanish", }, "es-es" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "European Spanish", }, "es-formal" =>
        Language { autonym : "español (formal)", is_enabled : true, is_rtl : false, name
        : "Spanish (formal address)", }, "es-mx" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Mexican Spanish", }, "es-ni" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Spanish (Nicaragua)", }, "ess"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Central Siberian Yupik", }, "esu" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Central Yupik", }, "et" => Language { autonym :
        "eesti", is_enabled : true, is_rtl : false, name : "Estonian", }, "eto" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Eton", },
        "ett" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Etruscan", }, "etu" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Ejagham", }, "eu" => Language { autonym : "euskara", is_enabled :
        true, is_rtl : false, name : "Basque", }, "ewo" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Ewondo", }, "ext" => Language {
        autonym : "estremeñu", is_enabled : true, is_rtl : false, name : "Extremaduran",
        }, "eya" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Eyak", }, "fa" => Language { autonym : "فارسی", is_enabled : true, is_rtl :
        true, name : "Persian", }, "fa-af" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Dari", }, "fab" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Annobonese Creole", }, "fan" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Fang", },
        "fat" => Language { autonym : "mfantse", is_enabled : true, is_rtl : false, name
        : "Fanti", }, "fax" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Fala", }, "fay" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Kuhmareyi", }, "ff" => Language { autonym : "Fulfulde",
        is_enabled : true, is_rtl : false, name : "Fula", }, "fi" => Language { autonym :
        "suomi", is_enabled : true, is_rtl : false, name : "Finnish", }, "fil" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Filipino",
        }, "fit" => Language { autonym : "meänkieli", is_enabled : true, is_rtl : false,
        name : "Tornedalen Finnish", }, "fiu" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Finno-Ugric languages", }, "fiu-vro" => Language {
        autonym : "võro", is_enabled : true, is_rtl : false, name : "Võro", }, "fj" =>
        Language { autonym : "Na Vosa Vakaviti", is_enabled : true, is_rtl : false, name
        : "Fijian", }, "fkv" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kvensk", }, "fmp" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Fe'Fe'", }, "fo" => Language { autonym : "føroyskt",
        is_enabled : true, is_rtl : false, name : "Faroese", }, "fon" => Language {
        autonym : "fɔ\u{300}ngbè", is_enabled : true, is_rtl : false, name : "Fon", },
        "fos" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Siraya", }, "fr" => Language { autonym : "français", is_enabled : true, is_rtl
        : false, name : "French", }, "fr-be" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Belgian French", }, "fr-ca" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Canadian French", }, "fr-ch" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Swiss French", }, "frc" => Language { autonym : "français cadien", is_enabled :
        true, is_rtl : false, name : "Cajun French", }, "frk" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Frankish", }, "frm" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Middle French", },
        "fro" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Old French", }, "frp" => Language { autonym : "arpetan", is_enabled : true,
        is_rtl : false, name : "Arpitan", }, "frr" => Language { autonym : "Nordfriisk",
        is_enabled : true, is_rtl : false, name : "Northern Frisian", }, "frs" =>
        Language { autonym : "Oostfräisk", is_enabled : true, is_rtl : false, name :
        "Eastern Frisian", }, "fsl" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "French Sign Language", }, "fud" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Futunan", }, "fuf" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Pular", }, "fur" =>
        Language { autonym : "furlan", is_enabled : true, is_rtl : false, name :
        "Friulian", }, "fvr" => Language { autonym : "poor’íŋ belé’ŋ", is_enabled
        : true, is_rtl : false, name : "Fur", }, "fy" => Language { autonym : "Frysk",
        is_enabled : true, is_rtl : false, name : "Western Frisian", }, "ga" => Language
        { autonym : "Gaeilge", is_enabled : true, is_rtl : false, name : "Irish", },
        "gaa" => Language { autonym : "Ga", is_enabled : true, is_rtl : false, name :
        "Ga", }, "gag" => Language { autonym : "Gagauz", is_enabled : true, is_rtl :
        false, name : "Gagauz", }, "gah" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Alekano", }, "gan" => Language { autonym : "贛語",
        is_enabled : true, is_rtl : false, name : "Gan", }, "gan-hans" => Language {
        autonym : "赣语（简体）", is_enabled : true, is_rtl : false, name :
        "Gan (Simplified Han script)", }, "gan-hant" => Language { autonym :
        "贛語（繁體）", is_enabled : true, is_rtl : false, name :
        "Gan (Traditional Han script)", }, "gay" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Gayo", }, "gba" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Gbaya", }, "gbb" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Kaytetye", }, "gbk" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Gaddi", },
        "gbk-deva" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gaddi (Devanagari script)", }, "gbk-takr" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Gaddi (Takri script)", }, "gbm" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Garhwali", }, "gbz" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Zoroastrian Dari", }, "gcf" => Language { autonym : "kréyòl Gwadloup",
        is_enabled : true, is_rtl : false, name : "Guadeloupean Creole", }, "gcr" =>
        Language { autonym : "kriyòl gwiyannen", is_enabled : true, is_rtl : false, name
        : "Guianan Creole", }, "gd" => Language { autonym : "Gàidhlig", is_enabled :
        true, is_rtl : false, name : "Scottish Gaelic", }, "gem" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Germanic languages", }, "gez" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Geez", },
        "gil" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gilbertese", }, "gju" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Gujari", }, "gju-arab" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Gujari (Arabic script)", }, "gju-deva" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gujari (Devanagari script)", }, "gl" => Language { autonym : "galego",
        is_enabled : true, is_rtl : false, name : "Galician", }, "gld" => Language {
        autonym : "на\u{304}ни", is_enabled : true, is_rtl : false, name : "Nanai",
        }, "glh" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Northwest Pashai", }, "glk" => Language { autonym : "گیلکی", is_enabled :
        true, is_rtl : true, name : "Gilaki", }, "gmh" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Middle High German", }, "gml" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Middle Low German", }, "gmy" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Mycenaean Greek", }, "gn" => Language { autonym :
        "Avañe'ẽ", is_enabled : true, is_rtl : false, name : "Guarani", }, "gnq" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Ganaʼ", },
        "goh" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Old High German", }, "gom" => Language { autonym :
        "गो\u{902}यची को\u{902}कणी / Gõychi Konknni", is_enabled :
        true, is_rtl : false, name : "Goan Konkani", }, "gom-deva" => Language { autonym
        : "गो\u{902}यची को\u{902}कणी", is_enabled : true, is_rtl :
        false, name : "Goan Konkani (Devanagari script)", }, "gom-latn" => Language {
        autonym : "Gõychi Konknni", is_enabled : true, is_rtl : false, name :
        "Goan Konkani (Latin script)", }, "gon" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Gondi", }, "gor" => Language { autonym :
        "Bahasa Hulontalo", is_enabled : true, is_rtl : false, name : "Gorontalo", },
        "got" => Language { autonym : "𐌲𐌿𐍄𐌹𐍃𐌺", is_enabled : true,
        is_rtl : false, name : "Gothic", }, "gpe" => Language { autonym :
        "Ghanaian Pidgin", is_enabled : true, is_rtl : false, name : "Ghanaian Pidgin",
        }, "grb" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Grebo", }, "grc" => Language { autonym : "Ἀρχαία ἑλληνικὴ",
        is_enabled : true, is_rtl : false, name : "Ancient Greek", }, "gsg" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "German Sign Language",
        }, "gsw" => Language { autonym : "Alemannisch", is_enabled : true, is_rtl :
        false, name : "Alemannic", }, "gsw-fr" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Alsatian", }, "gu" => Language { autonym :
        "ગ\u{ac1}જરાતી", is_enabled : true, is_rtl : false, name :
        "Gujarati", }, "guc" => Language { autonym : "wayuunaiki", is_enabled : true,
        is_rtl : false, name : "Wayuu", }, "gum" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Guambiano", }, "gur" => Language { autonym :
        "farefare", is_enabled : true, is_rtl : false, name : "Frafra", }, "guw" =>
        Language { autonym : "gungbe", is_enabled : true, is_rtl : false, name : "Gun",
        }, "guz" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gusii", }, "gv" => Language { autonym : "Gaelg", is_enabled : true, is_rtl :
        false, name : "Manx", }, "gwi" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Gwichʼin", }, "gya" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Gbaya", }, "ha" => Language { autonym
        : "Hausa", is_enabled : true, is_rtl : false, name : "Hausa", }, "ha-arab" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Hausa (Arabic script)", }, "ha-latn" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Hausa (Latin script)", }, "ha-ne" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Hausa (Niger)", },
        "hac" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gurani", }, "hai" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Haida", }, "hak" => Language { autonym :
        "客家語 / Hak-kâ-ngî", is_enabled : true, is_rtl : false, name :
        "Hakka Chinese", }, "hak-hans" => Language { autonym : "客家语（简体）",
        is_enabled : true, is_rtl : false, name : "Hakka (Simplified Han script)", },
        "hak-hant" => Language { autonym : "客家語（繁體）", is_enabled : true,
        is_rtl : false, name : "Hakka (Traditional Han script)", }, "hak-latn" =>
        Language { autonym : "Hak-kâ-ngî (Pha\u{30d}k-fa-sṳ)", is_enabled : true,
        is_rtl : false, name : "Hakka (Latin script)", }, "hav" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Havu", }, "haw" => Language {
        autonym : "Hawaiʻi", is_enabled : true, is_rtl : false, name : "Hawaiian", },
        "hax" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Southern Haida", }, "haz" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Hazaragi", }, "hbo" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Biblical Hebrew", }, "he" => Language { autonym :
        "עברית", is_enabled : true, is_rtl : true, name : "Hebrew", }, "hea" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Northern Qiandong Miao", }, "hi" => Language { autonym :
        "हिन\u{94d}दी", is_enabled : true, is_rtl : false, name : "Hindi", },
        "hi-kthi" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Hindi (Kaithi script)", }, "hi-latn" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Hindi (Latin)", }, "hif" => Language { autonym :
        "Fiji Hindi", is_enabled : true, is_rtl : false, name : "Fiji Hindi", },
        "hif-deva" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Fiji Hindi (Devanagari script)", }, "hif-latn" => Language { autonym :
        "Fiji Hindi", is_enabled : true, is_rtl : false, name :
        "Fiji Hindi (Latin script)", }, "hil" => Language { autonym : "Ilonggo",
        is_enabled : true, is_rtl : false, name : "Hiligaynon", }, "him" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Western Pahari", },
        "hit" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Hittite", }, "hit-latn" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Hittite (Latin script)", }, "hit-xsux" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Hittite (Cuneiform script)", }, "hke"
        => Language { autonym : "kihunde", is_enabled : true, is_rtl : false, name :
        "Hunde", }, "hmn" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Hmong", }, "hne" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Chhattisgarhi", }, "hnj" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Hmong Njua", }, "hno" => Language { autonym :
        "ہندکو", is_enabled : true, is_rtl : true, name : "Northern Hindko", }, "ho"
        => Language { autonym : "Hiri Motu", is_enabled : true, is_rtl : false, name :
        "Hiri Motu", }, "hoc" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Ho", }, "hoc-latn" => Language { autonym : "Ho", is_enabled :
        true, is_rtl : false, name : "Ho (Latin script)", }, "hr" => Language { autonym :
        "hrvatski", is_enabled : true, is_rtl : false, name : "Croatian", }, "hrx" =>
        Language { autonym : "Hunsrik", is_enabled : true, is_rtl : false, name :
        "Hunsrik", }, "hsb" => Language { autonym : "hornjoserbsce", is_enabled : true,
        is_rtl : false, name : "Upper Sorbian", }, "hsn" => Language { autonym :
        "湘語", is_enabled : true, is_rtl : false, name : "Xiang", }, "hsn-hans" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Xiang (Simplified Han script)", }, "hsn-hant" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Xiang (Traditional Han script)", },
        "ht" => Language { autonym : "Kreyòl ayisyen", is_enabled : true, is_rtl :
        false, name : "Haitian Creole", }, "hts" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Hadza", }, "hu" => Language { autonym : "magyar",
        is_enabled : true, is_rtl : false, name : "Hungarian", }, "hu-formal" => Language
        { autonym : "magyar (formal)", is_enabled : true, is_rtl : false, name :
        "Hungarian (formal address)", }, "hup" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Hupa", }, "hur" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Halkomelem", }, "hy" => Language {
        autonym : "հայերեն", is_enabled : true, is_rtl : false, name : "Armenian",
        }, "hyw" => Language { autonym : "Արեւմտահայերէն", is_enabled :
        true, is_rtl : false, name : "Western Armenian", }, "hz" => Language { autonym :
        "Otsiherero", is_enabled : true, is_rtl : false, name : "Herero", }, "ia" =>
        Language { autonym : "interlingua", is_enabled : true, is_rtl : false, name :
        "Interlingua", }, "iba" => Language { autonym : "Jaku Iban", is_enabled : true,
        is_rtl : false, name : "Iban", }, "ibb" => Language { autonym : "ibibio",
        is_enabled : true, is_rtl : false, name : "Ibibio", }, "id" => Language { autonym
        : "Bahasa Indonesia", is_enabled : true, is_rtl : false, name : "Indonesian", },
        "ie" => Language { autonym : "Interlingue", is_enabled : true, is_rtl : false,
        name : "Interlingue", }, "ifu" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Mayoyao Ifugao", }, "ig" => Language { autonym : "Igbo",
        is_enabled : true, is_rtl : false, name : "Igbo", }, "igb" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Ebira", }, "igl" => Language {
        autonym : "Igala", is_enabled : true, is_rtl : false, name : "Igala", }, "ii" =>
        Language { autonym : "ꆇꉙ", is_enabled : true, is_rtl : false, name :
        "Sichuan Yi", }, "ijo" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Ijaw languages", }, "ik" => Language { autonym : "Iñupiatun",
        is_enabled : true, is_rtl : false, name : "Inupiaq", }, "ike-cans" => Language {
        autonym : "ᐃᓄᒃᑎᑐᑦ", is_enabled : true, is_rtl : false, name :
        "Eastern Canadian (Aboriginal syllabics)", }, "ike-latn" => Language { autonym :
        "inuktitut", is_enabled : true, is_rtl : false, name :
        "Eastern Canadian (Latin script)", }, "ikt" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Western Canadian Inuktitut", }, "ilo"
        => Language { autonym : "Ilokano", is_enabled : true, is_rtl : false, name :
        "Iloko", }, "inc" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Indo-Aryan languages", }, "ine" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Indo-European languages", }, "inh" => Language {
        autonym : "гӀалгӀай", is_enabled : true, is_rtl : false, name : "Ingush",
        }, "io" => Language { autonym : "Ido", is_enabled : true, is_rtl : false, name :
        "Ido", }, "ira" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Iranian languages", }, "iro" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Iroquoian languages", }, "is" => Language {
        autonym : "íslenska", is_enabled : true, is_rtl : false, name : "Icelandic", },
        "ish" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Esan", }, "isk-arab" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Ishkashimi (Arabic script)", }, "isk-cyrl" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Ishkashimi (Cyrillic script)", },
        "isk-latn" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Ishkashimi (Latin script)", }, "ist" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Istriot", }, "isu" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Isu", }, "isv" => Language { autonym
        : "medžuslovjansky", is_enabled : true, is_rtl : false, name :
        "medžuslovjansky", }, "isv-cyrl" => Language { autonym :
        "меджусловјанскы", is_enabled : true, is_rtl : false, name :
        "Interslavic (Cyrillic script)", }, "isv-latn" => Language { autonym :
        "medžuslovjansky", is_enabled : true, is_rtl : false, name :
        "Interslavic (Latin script)", }, "it" => Language { autonym : "italiano",
        is_enabled : true, is_rtl : false, name : "Italian", }, "iu" => Language {
        autonym : "ᐃᓄᒃᑎᑐᑦ / inuktitut", is_enabled : true, is_rtl : false,
        name : "Inuktitut", }, "ivb" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Ibatan", }, "izh" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Ingrian", }, "ja" => Language { autonym :
        "日本語", is_enabled : true, is_rtl : false, name : "Japanese", }, "ja-hani"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Japanese (Kanji script)", }, "ja-hira" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Japanese (Hiragana script)", }, "ja-hrkt" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Japanese (Kana script)", }, "ja-kana" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Japanese (Katakana script)", }, "jac" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Popti'", }, "jak" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Jakun", },
        "jam" => Language { autonym : "Patois", is_enabled : true, is_rtl : false, name :
        "Jamaican Creole English", }, "jax" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Jambi Malay", }, "jbo" => Language { autonym :
        "la .lojban.", is_enabled : true, is_rtl : false, name : "Lojban", }, "jdt" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Judeo-Tat",
        }, "jdt-cyrl" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Judeo-Tat (Cyrillic script)", }, "jgo" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Ngomba", }, "jje" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Jeju", }, "jmc" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Machame", },
        "jpr" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Judeo-Persian", }, "jrb" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Judeo-Arabic", }, "juk" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Wapan", }, "jut" => Language { autonym : "jysk",
        is_enabled : true, is_rtl : false, name : "Jutish", }, "jv" => Language { autonym
        : "Jawa", is_enabled : true, is_rtl : false, name : "Javanese", }, "jv-java" =>
        Language { autonym : "ꦗꦮ", is_enabled : true, is_rtl : false, name :
        "Javanese (Javanese script)", }, "ka" => Language { autonym :
        "ქართული", is_enabled : true, is_rtl : false, name : "Georgian", },
        "kaa" => Language { autonym : "Qaraqalpaqsha", is_enabled : true, is_rtl : false,
        name : "Kara-Kalpak", }, "kab" => Language { autonym : "Taqbaylit", is_enabled :
        true, is_rtl : false, name : "Kabyle", }, "kac" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Kachin", }, "kag" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Kajaman", }, "kai" =>
        Language { autonym : "Karai-karai", is_enabled : true, is_rtl : false, name :
        "Karekare", }, "kaj" => Language { autonym : "Jju", is_enabled : true, is_rtl :
        false, name : "Jju", }, "kam" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Kamba", }, "kar" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Karenic languages", }, "kaw" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Kawi", }, "kbd" => Language {
        autonym : "адыгэбзэ", is_enabled : true, is_rtl : false, name :
        "Kabardian", }, "kbd-cyrl" => Language { autonym : "адыгэбзэ", is_enabled
        : true, is_rtl : false, name : "Kabardian (Cyrillic script)", }, "kbd-latn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kabardian (Latin script)", }, "kbl" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Kanembu", }, "kbp" => Language { autonym :
        "Kabɩyɛ", is_enabled : true, is_rtl : false, name : "Kabiye", }, "kcg" =>
        Language { autonym : "Tyap", is_enabled : true, is_rtl : false, name : "Tyap", },
        "kck" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kalanga", }, "kde" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Makonde", }, "kea" => Language { autonym : "kabuverdianu",
        is_enabled : true, is_rtl : false, name : "Cape Verdean Creole", }, "kek" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Qʼeqchiʼ",
        }, "ken" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kenyang", }, "ker" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kera", }, "kfo" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Koro", }, "kfr" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Kutchi", }, "kg" => Language { autonym : "Kongo",
        is_enabled : true, is_rtl : false, name : "Kongo", }, "kge" => Language { autonym
        : "Kumoring", is_enabled : true, is_rtl : false, name : "Komering", }, "kge-arab"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Komering (Arabic script)", }, "kgg" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Kusunda", }, "kgp" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Kaingang", }, "kha" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Khasi", }, "khi" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Khoisan languages", }, "kho" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Khotanese", }, "khq" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Koyra Chiini", }, "khw" => Language {
        autonym : "کھوار", is_enabled : true, is_rtl : true, name : "Khowar", },
        "ki" => Language { autonym : "Gĩkũyũ", is_enabled : true, is_rtl : false, name
        : "Kikuyu", }, "kip" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Sheshi Kham", }, "kiu" => Language { autonym : "Kırmancki",
        is_enabled : true, is_rtl : false, name : "Kirmanjki", }, "kj" => Language {
        autonym : "Kwanyama", is_enabled : true, is_rtl : false, name : "Kuanyama", },
        "kjh" => Language { autonym : "хакас", is_enabled : true, is_rtl : false,
        name : "Khakas", }, "kjp" => Language { autonym :
        "ဖ\u{1060}\u{102f}\u{1036}လ\u{102d}က\u{103a}", is_enabled : true, is_rtl :
        false, name : "Eastern Pwo", }, "kk" => Language { autonym : "қазақша",
        is_enabled : true, is_rtl : false, name : "Kazakh", }, "kk-arab" => Language {
        autonym : "قازاقشا (تٴوتە)", is_enabled : true, is_rtl : true, name :
        "Kazakh (Arabic script)", }, "kk-cn" => Language { autonym :
        "قازاقشا (جۇنگو)", is_enabled : true, is_rtl : true, name :
        "Kazakh (China)", }, "kk-cyrl" => Language { autonym :
        "қазақша (кирил)", is_enabled : true, is_rtl : false, name :
        "Kazakh (Cyrillic script)", }, "kk-kz" => Language { autonym :
        "қазақша (Қазақстан)", is_enabled : true, is_rtl : false, name :
        "Kazakh (Kazakhstan)", }, "kk-latn" => Language { autonym : "qazaqşa (latın)",
        is_enabled : true, is_rtl : false, name : "Kazakh (Latin script)", }, "kk-tr" =>
        Language { autonym : "qazaqşa (Türkïya)", is_enabled : true, is_rtl : false,
        name : "Kazakh (Turkey)", }, "kkj" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Kako", }, "kl" => Language { autonym :
        "kalaallisut", is_enabled : true, is_rtl : false, name : "Kalaallisut", }, "kld"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gamilaraay", }, "kln" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kalenjin", }, "kls" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Kalasha", }, "kls-arab" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Kalasha (Arabic script)", },
        "kls-latn" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kalasha (Latin script)", }, "km" => Language { autonym :
        "ភាសាខ\u{17d2}មែរ", is_enabled : true, is_rtl : false, name :
        "Khmer", }, "kmb" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Kimbundu", }, "kmr" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Northern Kurdish", }, "kmr-arab" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name :
        "Northern Kurdish (Arabic script)", }, "kmr-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Northern Kurdish (Latin script)", },
        "kmz" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Khorasani Turkic", }, "kn" => Language { autonym : "ಕನ\u{ccd}ನಡ",
        is_enabled : true, is_rtl : false, name : "Kannada", }, "knc" => Language {
        autonym : "Yerwa Kanuri", is_enabled : true, is_rtl : false, name :
        "Central Kanuri", }, "kne" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Kankanaey", }, "knn" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Maharashtrian Konkani", }, "knq" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Kintaq", }, "ko" =>
        Language { autonym : "한국어", is_enabled : true, is_rtl : false, name :
        "Korean", }, "ko-cn" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Korean (China)", }, "ko-hani" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Korean (Hanja script)", }, "ko-kore"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Korean (mixed script)", }, "ko-kp" => Language { autonym : "조선말",
        is_enabled : true, is_rtl : false, name : "Korean (North Korea)", }, "koi" =>
        Language { autonym : "перем коми", is_enabled : true, is_rtl : false,
        name : "Komi-Permyak", }, "kok" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Konkani", }, "kos" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Kosraean", }, "koy" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Koyukon", }, "kpe" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Kpelle", }, "kqr" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Kimaragang",
        }, "kqt" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Klias River Kadazan", }, "kqv" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Okolod", }, "kr" => Language { autonym : "kanuri",
        is_enabled : true, is_rtl : false, name : "Kanuri", }, "krc" => Language {
        autonym : "къарачай-малкъар", is_enabled : true, is_rtl : false,
        name : "Karachay-Balkar", }, "kri" => Language { autonym : "Krio", is_enabled :
        true, is_rtl : false, name : "Krio", }, "krj" => Language { autonym :
        "Kinaray-a", is_enabled : true, is_rtl : false, name : "Kinaray-a", }, "krl" =>
        Language { autonym : "karjal", is_enabled : true, is_rtl : false, name :
        "Karelian", }, "kro" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kru languages", }, "kru" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Kurukh", }, "ks" => Language { autonym :
        "کٲش\u{64f}ر", is_enabled : true, is_rtl : true, name : "Kashmiri", },
        "ks-arab" => Language { autonym : "کٲش\u{64f}ر", is_enabled : true, is_rtl :
        true, name : "Kashmiri (Arabic script)", }, "ks-deva" => Language { autonym :
        "कॉश\u{941}र", is_enabled : true, is_rtl : false, name :
        "Kashmiri (Devanagari script)", }, "ksb" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Shambala", }, "ksf" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Bafia", }, "ksh" => Language {
        autonym : "Ripoarisch", is_enabled : true, is_rtl : false, name : "Colognian", },
        "ksw" => Language { autonym : "စ\u{103e}\u{102e}ၤ", is_enabled : true, is_rtl
        : false, name : "S'gaw Karen", }, "ksy-beng" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Kharia Thar (Bengali script)", },
        "ku" => Language { autonym : "kurdî", is_enabled : true, is_rtl : false, name :
        "Kurdish", }, "ku-arab" => Language { autonym : "کوردی (عەرەبی)",
        is_enabled : true, is_rtl : true, name : "Kurdish (Arabic script)", }, "ku-latn"
        => Language { autonym : "kurdî (latînî)", is_enabled : true, is_rtl : false,
        name : "Kurdish (Latin script)", }, "kum" => Language { autonym :
        "къумукъ", is_enabled : true, is_rtl : false, name : "Kumyk", }, "kus" =>
        Language { autonym : "Kʋsaal", is_enabled : true, is_rtl : false, name :
        "Kusaal", }, "kut" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kutenai", }, "kv" => Language { autonym : "коми", is_enabled :
        true, is_rtl : false, name : "Komi", }, "kve" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Kalabakan", }, "kw" => Language {
        autonym : "kernowek", is_enabled : true, is_rtl : false, name : "Cornish", },
        "kwk" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kwakʼwala", }, "kxd" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Brunei Malay", }, "kxi" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Keningau Murut", }, "kxn" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Kanowit", }, "kxv" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Kuvi", }, "ky" =>
        Language { autonym : "кыргызча", is_enabled : true, is_rtl : false, name
        : "Kyrgyz", }, "kyw-beng" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Kurmali (Bengali script)", }, "kyw-deva" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Kurmali (Devanagari script)", },
        "la" => Language { autonym : "Latina", is_enabled : true, is_rtl : false, name :
        "Latin", }, "lad" => Language { autonym : "Ladino", is_enabled : true, is_rtl :
        false, name : "Ladino", }, "lad-hebr" => Language { autonym : "", is_enabled :
        false, is_rtl : true, name : "Ladino (Hebrew script)", }, "lad-latn" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name :
        "Ladino (Latin script)", }, "lag" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Langi", }, "lah" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Western Panjabi", }, "laj" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Lango", }, "lam" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Lamba", }, "lb" =>
        Language { autonym : "Lëtzebuergesch", is_enabled : true, is_rtl : false, name :
        "Luxembourgish", }, "lbe" => Language { autonym : "лакку", is_enabled :
        true, is_rtl : false, name : "Lak", }, "lcm" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Tungag", }, "ldn" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Láadan", }, "lem" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Nomaande",
        }, "lez" => Language { autonym : "лезги", is_enabled : true, is_rtl : false,
        name : "Lezghian", }, "lfn" => Language { autonym : "Lingua Franca Nova",
        is_enabled : true, is_rtl : false, name : "Lingua Franca Nova", }, "lg" =>
        Language { autonym : "Luganda", is_enabled : true, is_rtl : false, name :
        "Ganda", }, "li" => Language { autonym : "Limburgs", is_enabled : true, is_rtl :
        false, name : "Limburgish", }, "li-be" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Belgian Limburgish", }, "li-nl" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Dutch Limburgish", },
        "lij" => Language { autonym : "Ligure", is_enabled : true, is_rtl : false, name :
        "Ligurian", }, "lij-mc" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Monégasque", }, "lil" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Lillooet", }, "liv" => Language { autonym :
        "Līvõ kēļ", is_enabled : true, is_rtl : false, name : "Livonian", }, "ljp" =>
        Language { autonym : "Lampung Api", is_enabled : true, is_rtl : false, name :
        "Lampung Api", }, "lki" => Language { autonym : "لەکی", is_enabled : true,
        is_rtl : true, name : "Laki", }, "lkt" => Language { autonym : "Lakȟótiyapi",
        is_enabled : true, is_rtl : false, name : "Lakota", }, "lld" => Language {
        autonym : "Ladin", is_enabled : true, is_rtl : false, name : "Ladin", }, "lmn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Lambadi", },
        "lmo" => Language { autonym : "lombard", is_enabled : true, is_rtl : false, name
        : "Lombard", }, "ln" => Language { autonym : "lingála", is_enabled : true,
        is_rtl : false, name : "Lingala", }, "lns" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Lamnso'", }, "lo" => Language { autonym :
        "ລາວ", is_enabled : true, is_rtl : false, name : "Lao", }, "lol" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Mongo", },
        "lou" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Louisiana Creole", }, "loz" => Language { autonym : "Silozi", is_enabled : true,
        is_rtl : false, name : "Lozi", }, "lrc" => Language { autonym :
        "لۊری شومالی", is_enabled : true, is_rtl : true, name :
        "Northern Luri", }, "lsm" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Saamia", }, "lt" => Language { autonym : "lietuvių", is_enabled
        : true, is_rtl : false, name : "Lithuanian", }, "ltg" => Language { autonym :
        "latgaļu", is_enabled : true, is_rtl : false, name : "Latgalian", }, "lu" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Luba-Katanga", }, "lua" => Language { autonym : "ciluba", is_enabled : true,
        is_rtl : false, name : "Luba-Lulua", }, "lud" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Ludic", }, "lui" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Luiseno", }, "lun" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Lunda", },
        "luo" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Luo", }, "lus" => Language { autonym : "Mizo ţawng", is_enabled : true, is_rtl
        : false, name : "Mizo", }, "lut" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Lushootseed", }, "luy" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Luyia", }, "luz" => Language {
        autonym : "لئری دو\u{659}مینی", is_enabled : true, is_rtl : true, name
        : "Southern Luri", }, "lv" => Language { autonym : "latviešu", is_enabled :
        true, is_rtl : false, name : "Latvian", }, "lzh" => Language { autonym :
        "文言", is_enabled : true, is_rtl : false, name : "Literary Chinese", }, "lzz"
        => Language { autonym : "Lazuri", is_enabled : true, is_rtl : false, name :
        "Laz", }, "mad" => Language { autonym : "Madhurâ", is_enabled : true, is_rtl :
        false, name : "Madurese", }, "maf" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Mafa", }, "mag" => Language { autonym :
        "मगही", is_enabled : true, is_rtl : false, name : "Magahi", }, "mai" =>
        Language { autonym : "म\u{948}थिली", is_enabled : true, is_rtl : false,
        name : "Maithili", }, "mak" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Makasar", }, "mak-bugi" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Makasar (Buginese script)", }, "man"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mandingo", }, "map" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Austronesian languages", }, "map-bms" => Language { autonym :
        "Basa Banyumasan", is_enabled : true, is_rtl : false, name : "Banyumasan", },
        "mas" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Masai", }, "maw" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Mampruli", }, "mcn" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Massa", }, "mcp" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Maka", }, "mde" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Maba", }, "mdf" => Language { autonym
        : "мокшень", is_enabled : true, is_rtl : false, name : "Moksha", }, "mdh"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Maguindanaon", }, "mdr" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Mandar", }, "men" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Mende", }, "mer" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Meru", }, "mey" => Language { autonym : "",
        is_enabled : false, is_rtl : true, name : "Hassaniyya", }, "mfa" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Kelantan-Pattani Malay", }, "mfe" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Morisyen", }, "mg" => Language { autonym :
        "Malagasy", is_enabled : true, is_rtl : false, name : "Malagasy", }, "mga" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Middle Irish", }, "mgh" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Makhuwa-Meetto", }, "mgo" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Metaʼ", }, "mh" => Language { autonym : "Ebon",
        is_enabled : true, is_rtl : false, name : "Marshallese", }, "mhk" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Mungaka", }, "mhn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Mòcheno",
        }, "mhr" => Language { autonym : "олык марий", is_enabled : true, is_rtl
        : false, name : "Eastern Mari", }, "mi" => Language { autonym : "Māori",
        is_enabled : true, is_rtl : false, name : "Māori", }, "mic" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Mi'kmaw", }, "mid" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Mandaic", },
        "min" => Language { autonym : "Minangkabau", is_enabled : true, is_rtl : false,
        name : "Minangkabau", }, "miq" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Miskito", }, "mis" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "unsupported language", }, "mix" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Mixtec", }, "mjx-beng"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mahali (Bengali script)", }, "mk" => Language { autonym :
        "македонски", is_enabled : true, is_rtl : false, name : "Macedonian",
        }, "mkh" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mon-Khmer", }, "ml" => Language { autonym : "മലയ\u{d3e}ളം", is_enabled
        : true, is_rtl : false, name : "Malayalam", }, "mn" => Language { autonym :
        "монгол", is_enabled : true, is_rtl : false, name : "Mongolian", },
        "mn-cyrl" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mongolian (Cyrillic script)", }, "mn-mong" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Mongolian (Mongolian script)", },
        "mnc" => Language { autonym : "manju gisun", is_enabled : true, is_rtl : false,
        name : "Manchu", }, "mnc-latn" => Language { autonym : "manju gisun", is_enabled
        : true, is_rtl : false, name : "Manchu (Latin script)", }, "mnc-mong" => Language
        { autonym : "ᠮᠠᠨᠵᡠ ᡤᡳᠰᡠᠨ", is_enabled : true, is_rtl : false,
        name : "Manchu (Mongolian script)", }, "mni" => Language { autonym :
        "ꯃꯤꯇꯩ ꯂꯣꯟ", is_enabled : true, is_rtl : false, name : "Manipuri",
        }, "mni-beng" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Manipuri (Bengali script)", }, "mnj" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Munji", }, "mno" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Manobo languages", },
        "mnq" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Minriq", }, "mns" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Mansi", }, "mnw" => Language { autonym :
        "ဘာသာမန\u{103a}", is_enabled : true, is_rtl : false, name : "Mon", },
        "mo" => Language { autonym : "молдовеняскэ", is_enabled : true,
        is_rtl : false, name : "Moldovan", }, "moe" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Innu-aimun", }, "moh" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Mohawk", }, "mos" =>
        Language { autonym : "moore", is_enabled : true, is_rtl : false, name : "Mossi",
        }, "mr" => Language { autonym : "मराठी", is_enabled : true, is_rtl :
        false, name : "Marathi", }, "mr-modi" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Marathi (Modi script)", }, "mrh" => Language {
        autonym : "Mara", is_enabled : true, is_rtl : false, name : "Mara", }, "mrj" =>
        Language { autonym : "кырык мары", is_enabled : true, is_rtl : false,
        name : "Western Mari", }, "mrt" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Marghi Central", }, "mrv" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Mangareva", }, "ms" => Language {
        autonym : "Bahasa Melayu", is_enabled : true, is_rtl : false, name : "Malay", },
        "ms-arab" => Language { autonym : "بهاس ملايو", is_enabled : true,
        is_rtl : true, name : "Malay (Jawi script)", }, "msi" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Sabah Malay", }, "mt" => Language {
        autonym : "Malti", is_enabled : true, is_rtl : false, name : "Maltese", }, "mua"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name : "Mundang",
        }, "mui" => Language { autonym : "Baso Palembang", is_enabled : true, is_rtl :
        false, name : "Musi", }, "mul" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "multiple languages", }, "mun" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Munda languages", }, "mus" =>
        Language { autonym : "Mvskoke", is_enabled : true, is_rtl : false, name :
        "Muscogee", }, "mvf" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Peripheral Mongolian", }, "mvi" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Miyako", }, "mvi-hira" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Miyako (Hiragana script)", }, "mvv" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Tagol", }, "mwl" => Language { autonym :
        "Mirandés", is_enabled : true, is_rtl : false, name : "Mirandese", }, "mwr" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Marwari", },
        "mwv" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mentawai", }, "mww" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Hmong Daw", }, "mww-latn" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Hmong Daw (Latin script)", }, "my" => Language {
        autonym : "မြန\u{103a}မာဘာသာ", is_enabled : true, is_rtl :
        false, name : "Burmese", }, "mye" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Myene", }, "myn" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Mayan languages", }, "myv" => Language { autonym :
        "эрзянь", is_enabled : true, is_rtl : false, name : "Erzya", }, "mzn" =>
        Language { autonym : "ماز\u{650}رونی", is_enabled : true, is_rtl : true,
        name : "Mazanderani", }, "na" => Language { autonym : "Dorerin Naoero",
        is_enabled : true, is_rtl : false, name : "Nauru", }, "nah" => Language { autonym
        : "Nāhuatl", is_enabled : true, is_rtl : false, name : "Nahuatl", }, "nai" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Indigenous languages of North America", }, "nan" => Language { autonym :
        "閩南語 / Bân-lâm-gí", is_enabled : true, is_rtl : false, name : "Minnan",
        }, "nan-hani" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Minnan (Han script)", }, "nan-hans" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Minnan (Simplified Han script)", },
        "nan-hant" => Language { autonym : "閩南語（傳統漢字）", is_enabled :
        true, is_rtl : false, name : "Minnan (Traditional Han script)", },
        "nan-latn-pehoeji" => Language { autonym : "Bân-lâm-gí (Pe\u{30d}h-ōe-jī)",
        is_enabled : true, is_rtl : false, name : "Minnan (Pe\u{30d}h-ōe-jī)", },
        "nan-latn-tailo" => Language { autonym : "Bân-lâm-gí (Tâi-lô)", is_enabled :
        true, is_rtl : false, name : "Minnan (Tâi-lô)", }, "nap" => Language { autonym
        : "Napulitano", is_enabled : true, is_rtl : false, name : "Neapolitan", }, "naq"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name : "Nama", },
        "nb" => Language { autonym : "norsk bokmål", is_enabled : true, is_rtl : false,
        name : "Norwegian Bokmål", }, "nd" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "North Ndebele", }, "nds" => Language { autonym :
        "Plattdüütsch", is_enabled : true, is_rtl : false, name : "Low German", },
        "nds-nl" => Language { autonym : "Nedersaksies", is_enabled : true, is_rtl :
        false, name : "Low Saxon", }, "ne" => Language { autonym :
        "न\u{947}पाली", is_enabled : true, is_rtl : false, name : "Nepali", },
        "new" => Language { autonym : "न\u{947}पाल भाषा", is_enabled :
        true, is_rtl : false, name : "Newari", }, "ng" => Language { autonym :
        "Oshiwambo", is_enabled : true, is_rtl : false, name : "Ndonga", }, "nge" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Ngémba", },
        "nia" => Language { autonym : "Li Niha", is_enabled : true, is_rtl : false, name
        : "Nias", }, "nic" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Niger–Congo languages", }, "nit" => Language { autonym :
        "క\u{c4a}ల\u{c3e}మ\u{c3f}", is_enabled : true, is_rtl : false, name :
        "Southeastern Kolami", }, "niu" => Language { autonym : "Niuē", is_enabled :
        true, is_rtl : false, name : "Niuean", }, "njo" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Ao Naga", }, "nl" => Language {
        autonym : "Nederlands", is_enabled : true, is_rtl : false, name : "Dutch", },
        "nl-be" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Flemish", }, "nl-informal" => Language { autonym : "Nederlands (informeel)",
        is_enabled : true, is_rtl : false, name : "Dutch (informal address)", }, "nla" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Ngombala",
        }, "nmg" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kwasio", }, "nmz" => Language { autonym : "nawdm", is_enabled : true, is_rtl :
        false, name : "Nawdm", }, "nn" => Language { autonym : "norsk nynorsk",
        is_enabled : true, is_rtl : false, name : "Norwegian Nynorsk", }, "nn-hognorsk"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Norwegian Høgnorsk", }, "nnh" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Ngiemboon", }, "nnz" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Nda'Nda'", }, "no" => Language {
        autonym : "norsk", is_enabled : true, is_rtl : false, name : "Norwegian", },
        "nod" => Language { autonym : "ᨣᩤ\u{1a74}ᨾᩮ\u{1a6c}\u{1a65}ᨦ",
        is_enabled : true, is_rtl : false, name : "Northern Thai", }, "nod-thai" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Northern Thai (Thai script)", }, "nog" => Language { autonym : "ногайша",
        is_enabled : true, is_rtl : false, name : "Nogai", }, "non" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Old Norse", }, "non-runr" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Old Norse (Runic script)", }, "nov" => Language { autonym : "Novial", is_enabled
        : true, is_rtl : false, name : "Novial", }, "nqo" => Language { autonym :
        "ߒߞߏ", is_enabled : true, is_rtl : true, name : "N’Ko", }, "nr" => Language
        { autonym : "isiNdebele seSewula", is_enabled : true, is_rtl : false, name :
        "South Ndebele", }, "nrf-gg" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Guernésiais", }, "nrf-je" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Jèrriais", }, "nrm" => Language {
        autonym : "Nouormand", is_enabled : true, is_rtl : false, name : "Norman", },
        "nsk" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Naskapi", }, "nsl" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Norwegian Sign Language", }, "nso" => Language { autonym :
        "Sesotho sa Leboa", is_enabled : true, is_rtl : false, name : "Northern Sotho",
        }, "ntd" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sesayap Tidung", }, "nub" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Nubian languages", }, "nup" => Language { autonym : "Nupe",
        is_enabled : true, is_rtl : false, name : "Nupe", }, "nus" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Nuer", }, "nv" => Language {
        autonym : "Diné bizaad", is_enabled : true, is_rtl : false, name : "Navajo", },
        "nwc" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Classical Newari", }, "nxm" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Numidian", }, "ny" => Language { autonym : "Chi-Chewa",
        is_enabled : true, is_rtl : false, name : "Nyanja", }, "nym" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Nyamwezi", }, "nyn" =>
        Language { autonym : "runyankore", is_enabled : true, is_rtl : false, name :
        "Nyankole", }, "nyo" => Language { autonym : "Orunyoro", is_enabled : true,
        is_rtl : false, name : "Nyoro", }, "nys" => Language { autonym : "Nyunga",
        is_enabled : true, is_rtl : false, name : "Nyungar", }, "nzi" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Nzima", }, "obt" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Old Breton",
        }, "oc" => Language { autonym : "occitan", is_enabled : true, is_rtl : false,
        name : "Occitan", }, "oco" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Old Cornish", }, "odt" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Old Dutch", }, "ofs" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Old Frisian", }, "oj" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Ojibwa", }, "ojb" =>
        Language { autonym : "Ojibwemowin", is_enabled : true, is_rtl : false, name :
        "Northwestern Ojibwa", }, "ojc" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Central Ojibwa", }, "ojp" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Old Japanese", }, "ojp-hani" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Old Japanese (Kanji script)", }, "ojp-hira" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Old Japanese (Hiragana script)", },
        "ojs" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Oji-Cree", }, "ojw" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Western Ojibwa", }, "oka" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Okanagan", }, "olo" => Language { autonym :
        "livvinkarjala", is_enabled : true, is_rtl : false, name : "Livvi-Karelian", },
        "om" => Language { autonym : "Oromoo", is_enabled : true, is_rtl : false, name :
        "Oromo", }, "oma" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Omaha-Ponca", }, "ood" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "O'odham", }, "or" => Language { autonym :
        "ଓଡ\u{b3c}\u{b3f}ଆ", is_enabled : true, is_rtl : false, name : "Odia", },
        "os" => Language { autonym : "ирон", is_enabled : true, is_rtl : false, name
        : "Ossetic", }, "osa" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Osage", }, "osa-latn" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Osage (Latin script)", }, "osi" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Osing", }, "osx" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Old Saxon",
        }, "ota" => Language { autonym : "", is_enabled : false, is_rtl : true, name :
        "Ottoman Turkish", }, "otk" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Old Turkish", }, "oto" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Otomian languages", }, "ovd" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Elfdalian",
        }, "owl" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Old Welsh", }, "pa" => Language { autonym : "ਪ\u{a70}ਜਾਬੀ", is_enabled
        : true, is_rtl : false, name : "Punjabi", }, "pa-guru" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Punjabi (Gurmukhi script)", },
        "paa" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Papuan languages", }, "pag" => Language { autonym : "Pangasinan", is_enabled :
        true, is_rtl : false, name : "Pangasinan", }, "pal" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Pahlavi", }, "pal-phli" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Pahlavi (Inscriptional Pahlavi script)", }, "pal-phlp" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name :
        "Pahlavi (Psalter Pahlavi script)", }, "pal-phlv" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Pahlavi (Book Pahlavi script)", },
        "pam" => Language { autonym : "Kapampangan", is_enabled : true, is_rtl : false,
        name : "Pampanga", }, "pao" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Northern Paiute", }, "pap" => Language { autonym :
        "Papiamentu", is_enabled : true, is_rtl : false, name : "Papiamento", }, "pap-aw"
        => Language { autonym : "Papiamento (Aruba)", is_enabled : true, is_rtl : false,
        name : "Papiamento (Aruba)", }, "paq" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Parya", }, "pau" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Palauan", }, "pbb" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Páez", }, "pcd" =>
        Language { autonym : "Picard", is_enabled : true, is_rtl : false, name :
        "Picard", }, "pcm" => Language { autonym : "Naijá", is_enabled : true, is_rtl :
        false, name : "Nigerian Pidgin", }, "pdc" => Language { autonym : "Deitsch",
        is_enabled : true, is_rtl : false, name : "Pennsylvania German", }, "pdt" =>
        Language { autonym : "Plautdietsch", is_enabled : true, is_rtl : false, name :
        "Plautdietsch", }, "peo" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Old Persian", }, "pfl" => Language { autonym : "Pälzisch",
        is_enabled : true, is_rtl : false, name : "Palatine German", }, "pgd" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Gāndhārī", },
        "pgd-arab" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gāndhārī (Arabic script)", }, "pgd-deva" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Gāndhārī (Devanagari script)", },
        "pgd-khar" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Gāndhārī (Kharoshthi script)", }, "pgl" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Primitive Irish", }, "phi" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Philippine languages", }, "phl" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Palula", }, "phn" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Phoenician", }, "phn-latn" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Phoenician (Latin script)", },
        "phn-phnx" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Phoenician (Phoenician script)", }, "phr" => Language { autonym : "", is_enabled
        : false, is_rtl : true, name : "Pahari-Potwari", }, "pi" => Language { autonym :
        "पालि", is_enabled : true, is_rtl : false, name : "Pali", }, "pi-sidd" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Pali (Siddham script)", }, "pih" => Language { autonym : "Norfuk / Pitkern",
        is_enabled : true, is_rtl : false, name : "Pitcairn-Norfolk", }, "pis" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Pijin", },
        "pjt" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Pitjantjatjara", }, "pkc" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Paekche", }, "pko" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Pökoot", }, "pks" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Pakistan Sign Language", }, "pl" =>
        Language { autonym : "polski", is_enabled : true, is_rtl : false, name :
        "Polish", }, "plv" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Southwest Palawano", }, "plw" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Brooke's Point Palawano", }, "pms" =>
        Language { autonym : "Piemontèis", is_enabled : true, is_rtl : false, name :
        "Piedmontese", }, "pnb" => Language { autonym : "پنجابی", is_enabled :
        true, is_rtl : true, name : "Western Punjabi", }, "pnt" => Language { autonym :
        "Ποντιακά", is_enabled : true, is_rtl : false, name : "Pontic", }, "pon"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Pohnpeian", }, "pov" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Upper Guinea Crioulo", }, "ppl" => Language { autonym : "Nawat",
        is_enabled : true, is_rtl : false, name : "Nawat", }, "ppu" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Papora-Hoanya", }, "pqm" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Maliseet-Passamaquoddy", }, "pra" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Prakrit", }, "prc" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Parachi", }, "prg" => Language {
        autonym : "prūsiskan", is_enabled : true, is_rtl : false, name : "Prussian", },
        "pro" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Old Provençal", }, "prs" => Language { autonym : "", is_enabled : false, is_rtl
        : true, name : "Dari", }, "ps" => Language { autonym : "پښتو", is_enabled :
        true, is_rtl : true, name : "Pashto", }, "ps-af" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Pashto (Afghanistan)", }, "ps-pk" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Pashto (Pakistan)", }, "psh" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Southwest Pashai", }, "psi" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Southeast Pashai", }, "psu" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sauraseni Prākrit", }, "psu-arab" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Sauraseni Prākrit (Arabic script)", }, "psu-brah"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sauraseni Prākrit (Brahmi script)", }, "psu-deva" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name :
        "Sauraseni Prākrit (Devanagari script)", }, "psu-guru" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name :
        "Sauraseni Prākrit (Gurmukhi script)", }, "pt" => Language { autonym :
        "português", is_enabled : true, is_rtl : false, name : "Portuguese", },
        "pt-ao1990" => Language { autonym : "", is_enabled : false, is_rtl : false, name
        : "Portuguese (1990 Orthographic Agreement)", }, "pt-br" => Language { autonym :
        "português do Brasil", is_enabled : true, is_rtl : false, name :
        "Brazilian Portuguese", }, "pt-colb1945" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Portuguese (1945 Orthographic Agreement)", },
        "pt-pt" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "European Portuguese", }, "pwn" => Language { autonym : "pinayuanan", is_enabled
        : true, is_rtl : false, name : "Paiwan", }, "pwo" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Western Pwo", }, "pyu" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Puyuma", }, "qu" =>
        Language { autonym : "Runa Simi", is_enabled : true, is_rtl : false, name :
        "Quechua", }, "quc" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kʼicheʼ", }, "qug" => Language { autonym : "Runa shimi",
        is_enabled : true, is_rtl : false, name : "Chimborazo Highland Quichua", }, "qwh"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Huaylas Ancash Quechua", }, "qxp" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Puno Quechua", }, "qxq" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Qashqai", }, "qya" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Quenya", }, "rag" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Logooli", },
        "rah" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Rabha", }, "raj" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Rajasthani", }, "rap" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Rapanui", }, "rar" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Rarotongan", }, "rcf" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Réunion Creole French", }, "rej"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name : "Rejang",
        }, "rgn" => Language { autonym : "Rumagnôl", is_enabled : true, is_rtl : false,
        name : "Romagnol", }, "rhg" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Rohingya", }, "rhg-arab" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Rohingya (Arabic script)", },
        "rhg-rohg" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Rohingya (Hanifi Rohingya script)", }, "rif" => Language { autonym : "Tarifit",
        is_enabled : true, is_rtl : false, name : "Riffian", }, "rki" => Language {
        autonym : "ရခ\u{102d}\u{102f}င\u{103a}", is_enabled : true, is_rtl : false,
        name : "Arakanese", }, "rkt" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Rangpuri", }, "rm" => Language { autonym : "rumantsch",
        is_enabled : true, is_rtl : false, name : "Romansh", }, "rm-puter" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Putèr", }, "rm-rumgr"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Rumantsch Grischun", }, "rm-surmiran" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Surmiran", }, "rm-sursilv" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Sursilvan", }, "rm-sutsilv" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Sutsilvan",
        }, "rm-vallader" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Vallader", }, "rmc" => Language { autonym : "romaňi čhib", is_enabled :
        true, is_rtl : false, name : "Carpathian Romani", }, "rmf" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Finnish Kalo", }, "rmg" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Traveller Norwegian", }, "rml" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Baltic Romani", }, "rml-cyrl" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Baltic Romani (Cyrillic script)", },
        "rmn" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Balkan Romani", }, "rmo" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Sinte Romani", }, "rmw" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Welsh-Romani", }, "rmy" => Language { autonym :
        "romani čhib", is_enabled : true, is_rtl : false, name : "Vlax Romani", }, "rn"
        => Language { autonym : "ikirundi", is_enabled : true, is_rtl : false, name :
        "Rundi", }, "ro" => Language { autonym : "română", is_enabled : true, is_rtl :
        false, name : "Romanian", }, "ro-md" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Moldavian", }, "roa" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Romance languages", }, "roa-rup" =>
        Language { autonym : "armãneashti", is_enabled : true, is_rtl : false, name :
        "Aromanian", }, "roa-tara" => Language { autonym : "tarandíne", is_enabled :
        true, is_rtl : false, name : "Tarantino", }, "rof" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Rombo", }, "rom" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Romany", }, "rsk" =>
        Language { autonym : "руски", is_enabled : true, is_rtl : false, name :
        "Pannonian Rusyn", }, "rtm" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Rotuman", }, "ru" => Language { autonym :
        "русский", is_enabled : true, is_rtl : false, name : "Russian", },
        "ru-petr1708" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Russian (Petrine orthography)", }, "rue" => Language { autonym :
        "русиньскый", is_enabled : true, is_rtl : false, name : "Rusyn", },
        "rug" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Roviana", }, "ruo" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Istro Romanian", }, "rup" => Language { autonym : "armãneashti",
        is_enabled : true, is_rtl : false, name : "Aromanian", }, "ruq" => Language {
        autonym : "Vlăheşte", is_enabled : true, is_rtl : false, name :
        "Megleno-Romanian", }, "ruq-cyrl" => Language { autonym : "Влахесте",
        is_enabled : true, is_rtl : false, name : "Megleno-Romanian (Cyrillic script)",
        }, "ruq-latn" => Language { autonym : "Vlăheşte", is_enabled : true, is_rtl :
        false, name : "Megleno-Romanian (Latin script)", }, "rut" => Language { autonym :
        "мыхаӀбишды", is_enabled : true, is_rtl : false, name : "Rutul", },
        "rw" => Language { autonym : "Ikinyarwanda", is_enabled : true, is_rtl : false,
        name : "Kinyarwanda", }, "rwk" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Rwa", }, "rwr" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Marwari (India)", }, "rys" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Yaeyama", }, "rys-hira" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Yaeyama (Hiragana script)", }, "ryu" => Language { autonym :
        "うちなーぐち", is_enabled : true, is_rtl : false, name : "Okinawan", },
        "ryu-hira" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Okinawan (Hiragana script)", }, "sa" => Language { autonym :
        "स\u{902}स\u{94d}क\u{943}तम\u{94d}", is_enabled : true, is_rtl : false,
        name : "Sanskrit", }, "sa-sidd" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Sanskrit (Siddham script)", }, "sad" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Sandawe", }, "sah" =>
        Language { autonym : "саха тыла", is_enabled : true, is_rtl : false, name
        : "Yakut", }, "sai" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "South American indigenous languages", }, "sal" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Salishan languages", },
        "sam" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Samaritan Aramaic", }, "saq" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Samburu", }, "sas" => Language { autonym : "Sasak",
        is_enabled : true, is_rtl : false, name : "Sasak", }, "sat" => Language { autonym
        : "ᱥᱟᱱᱛᱟᱲᱤ", is_enabled : true, is_rtl : false, name : "Santali",
        }, "sat-beng" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Santali (Bengali script)", }, "sat-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Santali (Latin script)", },
        "sat-orya" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Santali (Oriya script)", }, "saz" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Saurashtra", }, "sba" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Ngambay", }, "sbp" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Sangu", }, "sc" =>
        Language { autonym : "sardu", is_enabled : true, is_rtl : false, name :
        "Sardinian", }, "scl" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Shina", }, "scn" => Language { autonym : "sicilianu", is_enabled :
        true, is_rtl : false, name : "Sicilian", }, "sco" => Language { autonym :
        "Scots", is_enabled : true, is_rtl : false, name : "Scots", }, "sd" => Language {
        autonym : "سنڌي", is_enabled : true, is_rtl : true, name : "Sindhi", },
        "sd-deva" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sindhi (Devanagari script)", }, "sd-gujr" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Sindhi (Gujarati script)", }, "sd-khoj" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sindhi (Khojki script)", }, "sd-sind" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Sindhi (Khudawadi script)", }, "sdc" => Language {
        autonym : "Sassaresu", is_enabled : true, is_rtl : false, name :
        "Sassarese Sardinian", }, "sdh" => Language { autonym : "کوردی خوارگ",
        is_enabled : true, is_rtl : true, name : "Southern Kurdish", }, "sdh-arab" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Southern Kurdish (Arabic script)", }, "sdh-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Southern Kurdish (Latin script)", },
        "sdo" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Bukar–Sadong", }, "se" => Language { autonym : "davvisámegiella", is_enabled
        : true, is_rtl : false, name : "Northern Sami", }, "se-fi" => Language { autonym
        : "davvisámegiella (Suoma bealde)", is_enabled : true, is_rtl : false, name :
        "Northern Sami (Finland)", }, "se-no" => Language { autonym :
        "davvisámegiella (Norgga bealde)", is_enabled : true, is_rtl : false, name :
        "Northern Sami (Norway)", }, "se-se" => Language { autonym :
        "davvisámegiella (Ruoŧa bealde)", is_enabled : true, is_rtl : false, name :
        "Northern Sami (Sweden)", }, "sea" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Semai", }, "see" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Seneca", }, "seh" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Sena", }, "sei" =>
        Language { autonym : "Cmique Itom", is_enabled : true, is_rtl : false, name :
        "Seri", }, "sel" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Selkup", }, "sem" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Semitic languages", }, "ser" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Serrano", }, "ses" => Language {
        autonym : "Koyraboro Senni", is_enabled : true, is_rtl : false, name :
        "Koyraboro Senni", }, "sg" => Language { autonym : "Sängö", is_enabled : true,
        is_rtl : false, name : "Sango", }, "sga" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Old Irish", }, "sgh" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Shughni", }, "sgh-arab" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Shughni (Arabic script)", }, "sgh-cyrl" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Shughni (Cyrillic script)", }, "sgh-latn" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Shughni (Latin script)", }, "sgn" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "sign languages", }, "sgs" => Language { autonym :
        "žemaitėška", is_enabled : true, is_rtl : false, name : "Samogitian", },
        "sgy-arab" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sanglechi (Arabic script)", }, "sgy-latn" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Sanglechi (Latin script)", }, "sh" => Language {
        autonym : "srpskohrvatski / српскохрватски", is_enabled : true,
        is_rtl : false, name : "Serbo-Croatian", }, "sh-cyrl" => Language { autonym :
        "српскохрватски (ћирилица)", is_enabled : true, is_rtl :
        false, name : "Serbo-Croatian (Cyrillic script)", }, "sh-latn" => Language {
        autonym : "srpskohrvatski (latinica)", is_enabled : true, is_rtl : false, name :
        "Serbo-Croatian (Latin script)", }, "shd" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Kundal Shahi", }, "shi" => Language { autonym :
        "Taclḥit", is_enabled : true, is_rtl : false, name : "Tachelhit", }, "shi-latn"
        => Language { autonym : "Taclḥit", is_enabled : true, is_rtl : false, name :
        "Tachelhit (Latin script)", }, "shi-tfng" => Language { autonym :
        "ⵜⴰⵛⵍⵃⵉⵜ", is_enabled : true, is_rtl : false, name :
        "Tachelhit (Tifinagh script)", }, "shn" => Language { autonym : "တ\u{1086}း",
        is_enabled : true, is_rtl : false, name : "Shan", }, "shu" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Chadian Arabic", }, "shy" =>
        Language { autonym : "tacawit", is_enabled : true, is_rtl : false, name :
        "Shawiya", }, "shy-arab" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Shawiya (Arabic script)", }, "shy-latn" => Language { autonym :
        "tacawit", is_enabled : true, is_rtl : false, name : "Shawiya (Latin script)", },
        "shy-tfng" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Shawiya (Tifinagh script)", }, "si" => Language { autonym :
        "ස\u{dd2}ංහල", is_enabled : true, is_rtl : false, name : "Sinhala", },
        "sia" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Akkala Sami", }, "sid" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Sidamo", }, "simple" => Language { autonym : "Simple English",
        is_enabled : true, is_rtl : false, name : "Simple English", }, "sio" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Siouan languages", },
        "sit" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sino-Tibetan languages", }, "sjd" => Language { autonym :
        "кӣллт са\u{304}мь кӣлл", is_enabled : true, is_rtl : false, name :
        "Kildin Sami", }, "sje" => Language { autonym : "bidumsámegiella", is_enabled :
        true, is_rtl : false, name : "Pite Sami", }, "sjk" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Kemi Sami", }, "sjn" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Sindarin", }, "sjo" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Xibe", },
        "sjt" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Ter Sami", }, "sju" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Ume Sami", }, "sk" => Language { autonym : "slovenčina",
        is_enabled : true, is_rtl : false, name : "Slovak", }, "skr" => Language {
        autonym : "سرائیکی", is_enabled : true, is_rtl : true, name : "Saraiki",
        }, "skr-arab" => Language { autonym : "سرائیکی", is_enabled : true, is_rtl
        : true, name : "Saraiki (Arabic script)", }, "sl" => Language { autonym :
        "slovenščina", is_enabled : true, is_rtl : false, name : "Slovenian", }, "sla"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Slavic languages", }, "slh" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Southern Lushootseed", }, "sli" => Language { autonym :
        "Schläsch", is_enabled : true, is_rtl : false, name : "Lower Silesian", }, "slr"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name : "Salar",
        }, "sly" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Selayar", }, "sm" => Language { autonym : "Gagana Samoa", is_enabled : true,
        is_rtl : false, name : "Samoan", }, "sma" => Language { autonym :
        "åarjelsaemien", is_enabled : true, is_rtl : false, name : "Southern Sami", },
        "smi" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sámi languages", }, "smj" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Lule Sami", }, "smn" => Language { autonym :
        "anarâškielâ", is_enabled : true, is_rtl : false, name : "Inari Sami", },
        "sms" => Language { autonym : "nuõrttsääʹmǩiõll", is_enabled : true, is_rtl
        : false, name : "Skolt Sami", }, "sn" => Language { autonym : "chiShona",
        is_enabled : true, is_rtl : false, name : "Shona", }, "sne" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Jagoi", }, "snk" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Soninke", }, "so" =>
        Language { autonym : "Soomaaliga", is_enabled : true, is_rtl : false, name :
        "Somali", }, "sog" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Sogdien", }, "son" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Songhay languages", }, "spv" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Sambalpuri", }, "sq" => Language {
        autonym : "shqip", is_enabled : true, is_rtl : false, name : "Albanian", }, "sr"
        => Language { autonym : "српски / srpski", is_enabled : true, is_rtl :
        false, name : "Serbian", }, "sr-cyrl" => Language { autonym :
        "српски (ћирилица)", is_enabled : false, is_rtl : false, name :
        "Serbian (Cyrillic script)", }, "sr-ec" => Language { autonym :
        "српски (ћирилица)", is_enabled : true, is_rtl : false, name :
        "Serbian (Cyrillic script)", }, "sr-el" => Language { autonym :
        "srpski (latinica)", is_enabled : true, is_rtl : false, name :
        "Serbian (Latin script)", }, "sr-latn" => Language { autonym :
        "srpski (latinica)", is_enabled : false, is_rtl : false, name :
        "Serbian (Latin script)", }, "sr-me" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Montenegrin", }, "srh-arab" => Language { autonym
        : "", is_enabled : false, is_rtl : false, name : "Sarikoli (Arabic script)", },
        "srh-cyrl" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sarikoli (Cyrillic script)", }, "srh-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Sarikoli (Latin script)", }, "srk" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Serudung",
        }, "srn" => Language { autonym : "Sranantongo", is_enabled : true, is_rtl :
        false, name : "Sranan Tongo", }, "sro" => Language { autonym :
        "sardu campidanesu", is_enabled : true, is_rtl : false, name :
        "Campidanese Sardinian", }, "srq" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Sirionó", }, "srr" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Serer", }, "ss" => Language { autonym
        : "SiSwati", is_enabled : true, is_rtl : false, name : "Swati", }, "ssa" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Nilo-Saharan languages", }, "ssb" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Southern Sama", }, "ssf" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Thao", }, "ssy" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Saho", }, "st" =>
        Language { autonym : "Sesotho", is_enabled : true, is_rtl : false, name :
        "Southern Sotho", }, "sth" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Shelta", }, "stq" => Language { autonym : "Seeltersk",
        is_enabled : true, is_rtl : false, name : "Saterland Frisian", }, "str" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Straits Salish", }, "sty" => Language { autonym : "себертатар",
        is_enabled : true, is_rtl : false, name : "Siberian Tatar", }, "su" => Language {
        autonym : "Sunda", is_enabled : true, is_rtl : false, name : "Sundanese", },
        "suk" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sukuma", }, "sus" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Susu", }, "sux" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Sumerian", }, "sux-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Sumerian (Latin script)", },
        "sux-xsux" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sumerian (Cuneiform script)", }, "suz" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Sunwar", }, "sv" => Language { autonym :
        "svenska", is_enabled : true, is_rtl : false, name : "Swedish", }, "sva" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Svan", },
        "sw" => Language { autonym : "Kiswahili", is_enabled : true, is_rtl : false, name
        : "Swahili", }, "sw-cd" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Congo Swahili", }, "swb" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Comorian", }, "sxr" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Saaroa", }, "sxu" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Upper Saxon", }, "syc"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Classical Syriac", }, "syl" => Language { autonym : "ꠍꠤꠟꠐꠤ",
        is_enabled : true, is_rtl : false, name : "Sylheti", }, "syl-beng" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Sylheti (Bengali script)", }, "syl-sylo" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Sylheti (Sylheti Nagri script)", }, "syr" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Syriac", },
        "szl" => Language { autonym : "ślůnski", is_enabled : true, is_rtl : false,
        name : "Silesian", }, "szy" => Language { autonym : "Sakizaya", is_enabled :
        true, is_rtl : false, name : "Sakizaya", }, "ta" => Language { autonym :
        "தமிழ\u{bcd}", is_enabled : true, is_rtl : false, name : "Tamil", },
        "tai" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Tai languages", }, "tao" => Language { autonym : "", is_enabled : false, is_rtl
        : false, name : "Yami", }, "tay" => Language { autonym : "Tayal", is_enabled :
        true, is_rtl : false, name : "Atayal", }, "tbl" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Tboli", }, "tce" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Southern Tutchone", },
        "tcy" => Language { autonym : "ತುಳು", is_enabled : true, is_rtl : false,
        name : "Tulu", }, "tdd" => Language { autonym :
        "ᥖᥭᥰ ᥖᥬᥲ ᥑᥨᥒᥰ", is_enabled : true, is_rtl : false, name :
        "Tai Nuea", }, "te" => Language { autonym : "త\u{c46}లుగు", is_enabled
        : true, is_rtl : false, name : "Telugu", }, "tem" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Timne", }, "teo" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Teso", }, "ter" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Tereno", },
        "tet" => Language { autonym : "tetun", is_enabled : true, is_rtl : false, name :
        "Tetum", }, "tg" => Language { autonym : "тоҷикӣ", is_enabled : true,
        is_rtl : false, name : "Tajik", }, "tg-cyrl" => Language { autonym :
        "тоҷикӣ", is_enabled : true, is_rtl : false, name :
        "Tajik (Cyrillic script)", }, "tg-latn" => Language { autonym : "tojikī",
        is_enabled : true, is_rtl : false, name : "Tajik (Latin script)", }, "tgx" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Tagish", },
        "th" => Language { autonym : "ไทย", is_enabled : true, is_rtl : false, name
        : "Thai", }, "thq" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kochila Tharu", }, "thr" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Rana Tharu", }, "tht" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Tahltan", }, "ti" => Language {
        autonym : "ትግርኛ", is_enabled : true, is_rtl : false, name : "Tigrinya",
        }, "tig" => Language { autonym : "ትግሬ", is_enabled : true, is_rtl : false,
        name : "Tigre", }, "tih" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Timugon", }, "tiv" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Tiv", }, "tji" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Northern Tujia", }, "tk" => Language { autonym :
        "Türkmençe", is_enabled : true, is_rtl : false, name : "Turkmen", }, "tkl" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Tokelauan",
        }, "tkr" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Tsakhur", }, "tl" => Language { autonym : "Tagalog", is_enabled : true, is_rtl :
        false, name : "Tagalog", }, "tlb" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Tobelo", }, "tlh" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Klingon", }, "tlh-latn" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Klingon (Latin script)", },
        "tlh-piqd" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Klingon (Klingon script)", }, "tli" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Tlingit", }, "tly" => Language { autonym :
        "tolışi", is_enabled : true, is_rtl : false, name : "Talysh", }, "tly-cyrl" =>
        Language { autonym : "толыши", is_enabled : true, is_rtl : false, name :
        "Talysh (Cyrillic script)", }, "tmh" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Tamashek", }, "tmr" => Language { autonym : "",
        is_enabled : false, is_rtl : true, name : "Jewish Babylonian Aramaic", }, "tn" =>
        Language { autonym : "Setswana", is_enabled : true, is_rtl : false, name :
        "Tswana", }, "tnq" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Taíno", }, "to" => Language { autonym : "lea faka-Tonga",
        is_enabled : true, is_rtl : false, name : "Tongan", }, "tog" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Nyasa Tonga", }, "toi"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Tonga (Botatwe)", }, "tok" => Language { autonym : "toki pona", is_enabled :
        true, is_rtl : false, name : "Toki Pona", }, "tpi" => Language { autonym :
        "Tok Pisin", is_enabled : true, is_rtl : false, name : "Tok Pisin", }, "tr" =>
        Language { autonym : "Türkçe", is_enabled : true, is_rtl : false, name :
        "Turkish", }, "trp" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Kokborok", }, "tru" => Language { autonym : "Ṫuroyo", is_enabled
        : true, is_rtl : false, name : "Turoyo", }, "trv" => Language { autonym :
        "Seediq", is_enabled : true, is_rtl : false, name : "Taroko", }, "trw" =>
        Language { autonym : "", is_enabled : false, is_rtl : true, name : "Torwali", },
        "ts" => Language { autonym : "Xitsonga", is_enabled : true, is_rtl : false, name
        : "Tsonga", }, "tsd" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Tsakonian", }, "tsg" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Tausug", }, "tsi" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Tsimshian", }, "tsu" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Tsou", }, "tt" =>
        Language { autonym : "татарча / tatarça", is_enabled : true, is_rtl :
        false, name : "Tatar", }, "tt-cyrl" => Language { autonym : "татарча",
        is_enabled : true, is_rtl : false, name : "Tatar (Cyrillic script)", }, "tt-latn"
        => Language { autonym : "tatarça", is_enabled : true, is_rtl : false, name :
        "Tatar (Latin script)", }, "ttj" => Language { autonym : "Orutooro", is_enabled :
        true, is_rtl : false, name : "Tooro", }, "ttm" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Northern Tutchone", }, "ttt" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Muslim Tat",
        }, "tui" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Tupuri", }, "tum" => Language { autonym : "chiTumbuka", is_enabled : true,
        is_rtl : false, name : "Tumbuka", }, "tup" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Tupian languages", }, "tut" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Altaic languages", },
        "tvl" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Tuvalu", }, "tvu" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Tunen", }, "tw" => Language { autonym : "Twi", is_enabled : true,
        is_rtl : false, name : "Twi", }, "twd" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Tweants", }, "twq" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Tasawaq", }, "txa" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Tombonuwo", }, "txg" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Tangut", },
        "txo-beng" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Toto (Bengali script)", }, "txo-toto" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Toto (Toto script)", }, "txx" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Tatana", }, "ty" =>
        Language { autonym : "reo tahiti", is_enabled : true, is_rtl : false, name :
        "Tahitian", }, "tyv" => Language { autonym : "тыва дыл", is_enabled :
        true, is_rtl : false, name : "Tuvinian", }, "tzl" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Talossan", }, "tzm" => Language {
        autonym : "ⵜⴰⵎⴰⵣⵉⵖⵜ", is_enabled : true, is_rtl : false, name :
        "Central Atlas Tamazight", }, "udm" => Language { autonym : "удмурт",
        is_enabled : true, is_rtl : false, name : "Udmurt", }, "ug" => Language { autonym
        : "ئۇيغۇرچە / Uyghurche", is_enabled : true, is_rtl : true, name :
        "Uyghur", }, "ug-arab" => Language { autonym : "ئۇيغۇرچە", is_enabled :
        true, is_rtl : true, name : "Uyghur (Arabic script)", }, "ug-cyrl" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Uyghur (Cyrillic script)", }, "ug-latn" => Language { autonym : "Uyghurche",
        is_enabled : true, is_rtl : false, name : "Uyghur (Latin script)", }, "uga" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Ugaritic",
        }, "uk" => Language { autonym : "українська", is_enabled : true, is_rtl
        : false, name : "Ukrainian", }, "ulc" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Ulch", }, "uln" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Unserdeutsch", }, "umb" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Umbundu", }, "umu" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Munsee", },
        "und" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "undetermined language", }, "unr" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Mundari", }, "unr-deva" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Mundari (Devanagari script)", },
        "unr-nagm" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Mundari (Nag Mundari script)", }, "ur" => Language { autonym : "اردو",
        is_enabled : true, is_rtl : true, name : "Urdu", }, "urk" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Urak Lawoiʼ", }, "ush" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Ushoji", },
        "uun" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Pazeh", }, "uz" => Language { autonym : "oʻzbekcha / ўзбекча",
        is_enabled : true, is_rtl : false, name : "Uzbek", }, "uz-cyrl" => Language {
        autonym : "ўзбекча", is_enabled : true, is_rtl : false, name :
        "Uzbek (Cyrillic script)", }, "uz-latn" => Language { autonym : "oʻzbekcha",
        is_enabled : true, is_rtl : false, name : "Uzbek (Latin script)", }, "vai" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Vai", },
        "ve" => Language { autonym : "Tshivenda", is_enabled : true, is_rtl : false, name
        : "Venda", }, "vec" => Language { autonym : "vèneto", is_enabled : true, is_rtl
        : false, name : "Venetian", }, "vep" => Language { autonym : "vepsän kel’",
        is_enabled : true, is_rtl : false, name : "Veps", }, "vi" => Language { autonym :
        "Tiếng Việt", is_enabled : true, is_rtl : false, name : "Vietnamese", },
        "vi-hani" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Vietnamese (Han script)", }, "vls" => Language { autonym : "West-Vlams",
        is_enabled : true, is_rtl : false, name : "West Flemish", }, "vmf" => Language {
        autonym : "Mainfränkisch", is_enabled : true, is_rtl : false, name :
        "Main-Franconian", }, "vmw" => Language { autonym : "emakhuwa", is_enabled :
        true, is_rtl : false, name : "Makhuwa", }, "vo" => Language { autonym :
        "Volapük", is_enabled : true, is_rtl : false, name : "Volapük", }, "vot" =>
        Language { autonym : "Vaďďa", is_enabled : true, is_rtl : false, name :
        "Votic", }, "vro" => Language { autonym : "võro", is_enabled : true, is_rtl :
        false, name : "Võro", }, "vun" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Vunjo", }, "vut" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Vute", }, "wa" => Language { autonym : "walon",
        is_enabled : true, is_rtl : false, name : "Walloon", }, "wae" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Walser", }, "wak" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Wakashan languages", }, "wal" => Language { autonym : "wolaytta", is_enabled :
        true, is_rtl : false, name : "Wolaytta", }, "war" => Language { autonym :
        "Winaray", is_enabled : true, is_rtl : false, name : "Waray", }, "was" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Washo", },
        "wbl-arab" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Wakhi (Arabic script)", }, "wbl-arab-af" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Wakhi (Arabic script, Afghanistan)", },
        "wbl-arab-cn" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Wakhi (Arabic script, China)", }, "wbl-arab-pk" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Wakhi (Arabic script, Pakistan)",
        }, "wbl-cyrl" => Language { autonym : "", is_enabled : false, is_rtl : false,
        name : "Wakhi (Cyrillic script)", }, "wbl-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Wakhi (Latin script)", }, "wbp" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Warlpiri",
        }, "wen" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Sorbian languages", }, "wes" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Pidgin (Cameroon)", }, "wlm" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Middle Welsh", }, "wls" => Language {
        autonym : "Fakaʻuvea", is_enabled : true, is_rtl : false, name : "Wallisian", },
        "wlx" => Language { autonym : "waale", is_enabled : true, is_rtl : false, name :
        "Wali", }, "wo" => Language { autonym : "Wolof", is_enabled : true, is_rtl :
        false, name : "Wolof", }, "wsg" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Adilabad Gondi", }, "wsv" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Wotapuri-Katarqalai", }, "wuu" =>
        Language { autonym : "吴语", is_enabled : true, is_rtl : false, name : "Wu", },
        "wuu-hans" => Language { autonym : "吴语（简体）", is_enabled : true,
        is_rtl : false, name : "Wu (Simplified Han script)", }, "wuu-hant" => Language {
        autonym : "吳語（正體）", is_enabled : true, is_rtl : false, name :
        "Wu (Traditional Han script)", }, "wya" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Wyandot", }, "wyi" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Woiwurrung", }, "xal" => Language {
        autonym : "хальмг", is_enabled : true, is_rtl : false, name : "Kalmyk", },
        "xbm" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Middle Breton", }, "xh" => Language { autonym : "isiXhosa", is_enabled : true,
        is_rtl : false, name : "Xhosa", }, "xmf" => Language { autonym :
        "მარგალური", is_enabled : true, is_rtl : false, name :
        "Mingrelian", }, "xmm" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Manado Malay", }, "xnb" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Kanakanavu", }, "xno" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Anglo-Norman", }, "xnr" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Kangri", }, "xnr-deva"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Kangri (Devanagari script)", }, "xnr-takr" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Kangri (Takri script)", }, "xog" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Soga", },
        "xon" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Konkomba", }, "xpu" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Punic", }, "xsu" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Sanumá", }, "xsy" => Language { autonym : "saisiyat",
        is_enabled : true, is_rtl : false, name : "Saisiyat", }, "yah-cyrl" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name :
        "Yazghulami (Cyrillic script)", }, "yah-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Yazghulami (Latin script)", },
        "yai-cyrl" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Yaghnobi (Cyrillic script)", }, "yai-latn" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Yaghnobi (Latin script)", }, "yao" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Yao", },
        "yap" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Yapese", }, "yas" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Nugunu", }, "yat" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Yambeta", }, "yav" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Yangben", }, "ybb" => Language { autonym : "",
        is_enabled : false, is_rtl : false, name : "Yemba", }, "ydd" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Eastern Yiddish", },
        "ydg" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Yidgha", }, "yec" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Yeniche", }, "yi" => Language { autonym : "יי\u{5b4}דיש",
        is_enabled : true, is_rtl : true, name : "Yiddish", }, "ykg" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Tundra Yukaghir", },
        "yo" => Language { autonym : "Yorùbá", is_enabled : true, is_rtl : false, name
        : "Yoruba", }, "yoi" => Language { autonym : "", is_enabled : false, is_rtl :
        false, name : "Yonaguni", }, "yoi-hira" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Yonaguni (Hiragana script)", }, "yox" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Yoron", }, "yox-hira"
        => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Yoron (Hiragana script)", }, "ypk" => Language { autonym : "", is_enabled :
        false, is_rtl : false, name : "Yupik languages", }, "yrk" => Language { autonym :
        "", is_enabled : false, is_rtl : false, name : "Nenets", }, "yrl" => Language {
        autonym : "Nhẽẽgatú", is_enabled : true, is_rtl : false, name : "Nheengatu",
        }, "yua" => Language { autonym : "maaya t’aan", is_enabled : true, is_rtl :
        false, name : "Yucatec Maya", }, "yue" => Language { autonym : "粵語",
        is_enabled : true, is_rtl : false, name : "Cantonese", }, "yue-hans" => Language
        { autonym : "粵语（简体）", is_enabled : true, is_rtl : false, name :
        "Cantonese (Simplified Han script)", }, "yue-hant" => Language { autonym :
        "粵語（繁體）", is_enabled : true, is_rtl : false, name :
        "Cantonese (Traditional Han script)", }, "za" => Language { autonym :
        "Vahcuengh", is_enabled : true, is_rtl : false, name : "Zhuang", }, "zai" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Isthmus Zapotec", }, "zap" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Zapotec", }, "zbl" => Language { autonym : "", is_enabled
        : false, is_rtl : false, name : "Blissymbols", }, "zea" => Language { autonym :
        "Zeêuws", is_enabled : true, is_rtl : false, name : "Zeelandic", }, "zen" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name : "Zenaga", },
        "zgh" => Language { autonym :
        "ⵜⴰⵎⴰⵣⵉⵖⵜ ⵜⴰⵏⴰⵡⴰⵢⵜ", is_enabled : true, is_rtl :
        false, name : "Standard Moroccan Tamazight", }, "zgh-latn" => Language { autonym
        : "tamaziɣt tanawayt", is_enabled : true, is_rtl : false, name :
        "Standard Moroccan Tamazight (Latin script)", }, "zh" => Language { autonym :
        "中文", is_enabled : true, is_rtl : false, name : "Chinese", }, "zh-classical"
        => Language { autonym : "文言", is_enabled : true, is_rtl : false, name :
        "Literary Chinese", }, "zh-cn" => Language { autonym :
        "中文（中国大陆）", is_enabled : true, is_rtl : false, name :
        "Chinese (China)", }, "zh-hans" => Language { autonym : "中文（简体）",
        is_enabled : true, is_rtl : false, name : "Simplified Chinese", }, "zh-hant" =>
        Language { autonym : "中文（繁體）", is_enabled : true, is_rtl : false,
        name : "Traditional Chinese", }, "zh-hk" => Language { autonym :
        "中文（香港）", is_enabled : true, is_rtl : false, name :
        "Chinese (Hong Kong)", }, "zh-min-nan" => Language { autonym :
        "閩南語 / Bân-lâm-gí", is_enabled : true, is_rtl : false, name : "Minnan",
        }, "zh-mo" => Language { autonym : "中文（澳門）", is_enabled : true,
        is_rtl : false, name : "Chinese (Macau)", }, "zh-my" => Language { autonym :
        "中文（马来西亚）", is_enabled : true, is_rtl : false, name :
        "Chinese (Malaysia)", }, "zh-sg" => Language { autonym : "中文（新加坡）",
        is_enabled : true, is_rtl : false, name : "Chinese (Singapore)", }, "zh-tw" =>
        Language { autonym : "中文（臺灣）", is_enabled : true, is_rtl : false,
        name : "Chinese (Taiwan)", }, "zh-yue" => Language { autonym : "粵語",
        is_enabled : true, is_rtl : false, name : "Cantonese", }, "zmi" => Language {
        autonym : "", is_enabled : false, is_rtl : false, name : "Negeri Sembilan Malay",
        }, "znd" => Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "Zande languages", }, "zpu" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Yalálag Zapotec", }, "zu" => Language { autonym :
        "isiZulu", is_enabled : true, is_rtl : false, name : "Zulu", }, "zun" => Language
        { autonym : "", is_enabled : false, is_rtl : false, name : "Zuni", }, "zxx" =>
        Language { autonym : "", is_enabled : false, is_rtl : false, name :
        "no linguistic content", }, "zza" => Language { autonym : "", is_enabled : false,
        is_rtl : false, name : "Zaza", }
    },
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
