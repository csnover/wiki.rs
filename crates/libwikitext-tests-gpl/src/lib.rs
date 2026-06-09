//! Wikitext rendering test cases from MediaWiki.

#![cfg(test)]

mod config;
mod parser;
mod runner;

const BASE_DIR: &str = "./src/tests";

test_from_file! {
    annotation_parser_tests => "annotationParserTests",
    attribute_expander_tests => "attributeExpanderTests",
    bad_characters => "badCharacters",
    comments => "comments",
    definition_lists => "definitionLists",
    // dom_normalizer_tests => "domNormalizerTests",
    empty => "empty",
    // encap_parser_tests => "encapParserTests",
    ext_links => "extLinks",
    headings => "headings",
    // i18n_parser_tests => "i18nParserTests",
    indent_pre => "indentPre",
    indicators => "indicators",
    interlanguage_links => "interlanguageLinks",
    interwiki_links => "interwikiLinks",
    lang_parser_tests => "langParserTests",
    magic_links => "magicLinks",
    magic_words => "magicWords",
    media => "media",
    parser_tests => "parserTests",
    // p_fragment_handler_tests => "pFragmentHandlerTests",
    preprocessor => "preprocessor",
    pre_tags => "preTags",
    pst => "pst",
    p_wrapping => "pWrapping",
    quotes => "quotes",
    redirects => "redirects",
    regressions => "regressions",
    // section_wrapping_parser_tests => "sectionWrappingParserTests",
    // selser_wrapping_parser_tests => "selserWrappingParserTests",
    separator_tests => "separatorTests",
    table_fixups_parser_tests => "tableFixupsParserTests",
    tables => "tables",
    // timed_media_handler_parser_tests => "timedMediaHandlerParserTests",
    // tree_builder => "treeBuilder",
    // v3_parser_functions => "v3ParserFunctions",
    wt_escaping => "wtEscaping",
}

#[test]
fn ad_hoc() {
    let source = "{{{{1x|1}}{{1x|x}}|foo}}}";

    let parser = libwikitext_parse::Parser::new(&config::CONFIG);
    let root = parser.preprocess(source, true).unwrap();
    // let root = parser.parse(source).unwrap();
    eprintln!(
        "{:#?}",
        libwikitext_parse::inspect(&libwikitext_parse::FileMap::new(source), &root.root)
    );
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
