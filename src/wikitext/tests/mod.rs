use super::{codemap::FileMap, *};
use crate::{
    config::CONFIG,
    db::Database,
    renderer::{RenderOutput, render_test},
};
use std::{collections::HashMap, fs::File, io::Read as _, path::Path, sync::Arc};
use test_parser::{Chunk, SectionText};

mod extras;
mod test_parser;

const BASE_DIR: &str = "./src/wikitext/tests";

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
fn run_tests_from_file(config: &'static Configuration, path: impl AsRef<Path>) {
    let empty_options = SectionText::Kv(HashMap::new());

    let _ = env_logger::try_init();

    let code = {
        let mut file = File::open(&path).unwrap();
        let mut code = String::new();
        file.read_to_string(&mut code).unwrap();
        code
    };

    let tests = test_parser::Testfile::parse(&code).unwrap();

    let mut db = Arc::new(Database::new());

    let mut total = 0;
    let mut fails = 0;

    for chunk in tests.chunks {
        match chunk {
            Chunk::Article { title, text } => {
                Arc::get_mut(&mut db).unwrap().insert(title, text);
            }
            Chunk::FunctionHooks => {
                panic!("but no tests use this?!");
            }
            Chunk::Test { name, sections } => {
                let Some(wikitext) = sections.get("wikitext").and_then(SectionText::text) else {
                    log::warn!("Could not find wikitext for {name}!");
                    continue;
                };

                log::info!("Running {name} ...");
                total += 1;

                let options = sections.get("options").unwrap_or(&empty_options);
                let page_name = options.get("title").unwrap_or("Parser test");
                let result = match render_test(config, &db, page_name, wikitext) {
                    Ok(result) => result,
                    Err(err) => {
                        log::error!("Render failed: {err}");
                        fails += 1;
                        continue;
                    }
                };

                let fail = run_test_from_file(&sections, options, &result);
                fails += i32::from(fail);
                if !fail {
                    log::info!("pass!");
                }
            }
            Chunk::Comment | Chunk::Line | Chunk::Hooks => {
                // just ignore these, hooks is used only by
                // timedMediaHandlerParserTests and the other ones are just to
                // collect garbage
            }
        }
    }

    assert!(fails == 0, "failed {fails}/{total}");
}

#[track_caller]
fn run_test_from_file(
    sections: &test_parser::Sections<'_>,
    options: &SectionText<'_>,
    result: &RenderOutput,
) -> bool {
    let expected_html = sections
        .get("html/wiki.rs")
        .or_else(|| sections.get("html/php"))
        .or_else(|| sections.get("html"))
        .and_then(SectionText::text);

    let expected_meta = sections
        .get("metadata/wiki.rs")
        .or_else(|| sections.get("metadata/php"))
        .or_else(|| sections.get("metadata"))
        .and_then(SectionText::meta);

    let mut fail = false;
    if let Some(expected_html) = expected_html
        && result.content != expected_html
    {
        log::error!(
            "{}",
            similar_asserts::SimpleDiff::from_str(
                expected_html,
                &result.content,
                "expected",
                "actual"
            )
        );
        fail = true;
    }

    if let Some(meta) = expected_meta {
        if meta.flags.is_some() {
            log::warn!("TODO: Compare flags");
        } else if options.get("showflags").is_some() {
            log::error!("Expected flags");
            fail = true;
        }

        if meta.title.is_some() {
            log::warn!("TODO: Compare title");
        } else if options.get("showtitle").is_some() {
            log::error!("Expected title");
            fail = true;
        }

        if options.get("showtocdata").is_some() != meta.toc.is_empty() {
            log::error!(
                "TOC data mismatch: expected {}, got {}",
                options.get("showtocdata").is_some(),
                !meta.toc.is_empty()
            );
            fail = true;
        }

        if meta.toc.len() != result.outline.len() {
            log::error!(
                "Outline length mismatch: expected {}, got {}",
                meta.toc.len(),
                result.outline.len()
            );
            fail = true;
        }

        // Keep going even if the length mismatch since maybe the
        // knowledge of where the mismatch happened is useful
        for (index, (expected, actual)) in meta.toc.iter().zip(result.outline.iter()).enumerate() {
            if expected.tag != actual.level.tag_name() {
                log::error!(
                    "Outline {index} tag mismatch: expected {}, got {}",
                    actual.level.tag_name(),
                    expected.tag
                );
                fail = true;
            }
            if expected.line != actual.html {
                log::error!(
                    "Outline {index} title mismatch: expected {:?}, got {:?}",
                    expected.line,
                    actual.html
                );
                fail = true;
            }
        }
    }

    fail
}

#[track_caller]
fn run_test_for_goldenfile(test_name: &str, input: &str) {
    use std::io::Write as _;

    let mut mint = goldenfile::Mint::new(format!("{BASE_DIR}/goldenfiles"));
    let mut file = mint.new_goldenfile(format!("{test_name}.txt")).unwrap();
    let result = Parser::new(&CONFIG).parse(input, false).unwrap();
    let _ = writeln!(
        file,
        "{:#?}",
        inspectors::inspect(&FileMap::new(input), &result.root)
    );
}

macro_rules! test_from_file {
    ($($ident:ident => $path:literal),* $(,)?) => {
        $(#[test]
        fn $ident() {
            run_tests_from_file(&CONFIG, format!("{BASE_DIR}/parser/{}.txt", $path));
        })*
    }
}

use test_from_file;
