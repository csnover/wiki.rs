use super::*;
use crate::{
    config::CONFIG,
    wikitext::{codemap::FileMap, tests::test_parser::Chunk},
};
use std::{fs::File, io::Read, path::Path};

mod extras;
mod test_parser;

const BASE_DIR: &str = "./src/wikitext/tests/parser";

test_from_file! {
    annotation_parser_tests => "annotationParserTests",
    attribute_expander_tests => "attributeExpanderTests",
    bad_characters => "badCharacters",
    comments => "comments",
    definition_lists => "definitionLists",
    dom_normalizer_tests => "domNormalizerTests",
    encap_parser_tests => "encapParserTests",
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

#[track_caller]
fn run_tests_from_file(config: &Configuration, path: impl AsRef<Path>) {
    let _ = env_logger::try_init();

    let code = {
        let mut file = File::open(&path).unwrap();
        let mut code = String::new();
        file.read_to_string(&mut code).unwrap();
        code
    };

    let tests = test_parser::Testfile::parse(&code).unwrap();

    for chunk in tests.chunks {
        match chunk.node {
            Chunk::Article { title, _text } => {
                log::warn!("TODO: Mock article database for {title}");
            }
            Chunk::FunctionHooks => {
                panic!("but no tests use this?!");
            }
            Chunk::Test { name, sections } => {
                if let Some(test_parser::SectionText::Text(wikitext)) = sections.get("wikitext") {
                    log::info!("Running {name} ...");
                    run_test_with_config(config, wikitext);
                } else {
                    log::warn!("Could not find wikitext for {name}!");
                }
            }
            Chunk::Comment | Chunk::Line | Chunk::Hooks => {
                // just ignore these, hooks is used only by
                // timedMediaHandlerParserTests and the other ones are just to
                // collect garbage
            }
        }
    }
}

#[track_caller]
fn run_test(input: &str) {
    run_test_with_config(&CONFIG, input);
}

#[track_caller]
fn run_test_with_config(config: &Configuration, input: &str) {
    let result = Parser::new(config).parse(input, false).unwrap();
    eprintln!(
        "{:#?}",
        inspectors::inspect(&FileMap::new(input), &result.root)
    );
}

macro_rules! test_from_file {
    ($($ident:ident => $path:literal),* $(,)?) => {
        $(#[test]
        fn $ident() {
            run_tests_from_file(&CONFIG, format!("{BASE_DIR}/{}.txt", $path));
            panic!();
        })*
    }
}

use test_from_file;
