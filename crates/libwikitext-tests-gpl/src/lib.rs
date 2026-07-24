//! Wikitext rendering test cases from MediaWiki.

#![cfg(test)]

mod config;
mod parser;
mod runner;

const BASE_DIR: &str = "./src/tests";

test_from_file! {
    annotation_parser_tests => "core/annotationParserTests",
    attribute_expander_tests => "core/attributeExpanderTests",
    bad_characters => "core/badCharacters",
    comments => "core/comments",
    definition_lists => "core/definitionLists",
    empty => "core/empty",
    ext_links => "core/extLinks",
    funcs_parser_tests => "parser-functions/funcsParserTests",
    headings => "core/headings",
    indent_pre => "core/indentPre",
    indicators => "core/indicators",
    interlanguage_links => "core/interlanguageLinks",
    interwiki_links => "core/interwikiLinks",
    lang_parser_tests => "core/langParserTests",
    magic_links => "core/magicLinks",
    magic_words => "core/magicWords",
    media => "core/media",
    parser_tests => "core/parserTests",
    preprocessor => "core/preprocessor",
    pre_tags => "core/preTags",
    pst => "core/pst",
    p_wrapping => "core/pWrapping",
    quotes => "core/quotes",
    redirects => "core/redirects",
    regressions => "core/regressions",
    string_function_tests => "parser-functions/stringParserTests",
    table_fixups_parser_tests => "core/tableFixupsParserTests",
    tables => "core/tables",
    timed_media_handler_parser_tests => "core/timedMediaHandlerParserTests",
    wiki_rs => "core/wiki.rs",
    wt_escaping => "core/wtEscaping",
}

macro_rules! test_from_file {
    ($($ident:ident => $path:literal),* $(,)?) => {
        $(#[test]
        fn $ident() {
            runner::run_tests_from_file($path, format!("{BASE_DIR}/{}.txt", $path));
        })*
    }
}

use test_from_file;
