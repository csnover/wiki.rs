use super::test_parser::{Chunk, SectionText, Sections, Testfile};
use http::Uri;
use libphp_rs::DateTime;
use libwikitext_common::db::{DatabaseProvider, MockDatabase};
use libwikitext_data::{CONFIG, MESSAGES};
use libwikitext_parse::{FileMap, inspect};
use libwikitext_render::{RenderOutput, Statics, render_test};
use std::{collections::HashMap, fs::File, io::Read as _, path::Path, sync::Arc};

const BASE_DIR: &str = "./src/tests";

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
fn run_tests_from_file(suite: &str, path: impl AsRef<Path>) {
    let empty_options = SectionText::Kv(HashMap::new());

    let _ = env_logger::builder()
        .parse_default_env()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(None)
        .try_init();

    let code = {
        let mut file = File::open(&path).unwrap();
        let mut code = String::new();
        file.read_to_string(&mut code).unwrap();
        code
    };

    let tests = Testfile::parse(&code).unwrap();
    let file_map = FileMap::new(&code);

    let mut db = MockDatabase::new(&CONFIG);

    let mut total = 0;
    let mut fails = 0;

    // Like everything in MediaWiki, multiple passes are *required* to parse the
    // file correctly
    for chunk in &tests.chunks {
        match chunk {
            Chunk::Article { title, text } => {
                db.insert(title, text);
            }
            Chunk::FunctionHooks => {
                panic!("but no tests use this?!");
            }
            Chunk::Comment | Chunk::Line | Chunk::Hooks | Chunk::Test { .. } => {
                // Hooks is used only by timedMediaHandlerParserTests and
                // Line and Comment are just garbage
            }
        }
    }

    let db = Arc::new(db);
    let mut statics = Statics::builder()
        .base_time(DateTime::UNIX_EPOCH)
        .base_uri(Uri::from_static("http://example.com"))
        .db(Arc::clone(&db) as Arc<dyn DatabaseProvider>)
        .parser(db.config())
        .build();

    for chunk in tests.chunks {
        if let Chunk::Test {
            name,
            pos,
            sections,
        } = chunk
        {
            let line = file_map.find_line_col(pos).line;
            let target = &format!("{suite}.txt:{line}");

            let Some(wikitext) = sections.get("wikitext").and_then(SectionText::text) else {
                log::warn!(target: target, "Could not find wikitext for {name}!");
                continue;
            };

            log::info!(target: target, "Running {name:?}");
            total += 1;

            let options = sections.get("options").unwrap_or(&empty_options);
            let page_name = options.get("title").unwrap_or("Parser test");
            let result = match render_test(&mut statics, &MESSAGES, page_name, wikitext) {
                Ok(result) => result,
                Err(err) => {
                    log::error!(target: target, "Render failed: {err}");
                    if let Ok(ast) = statics.parser.parse(wikitext, false) {
                        log::info!(target: target, "AST: {:?}", inspect(&FileMap::new(wikitext), &ast.root));
                    }
                    fails += 1;
                    continue;
                }
            };

            let fail = run_test_from_file(target, &sections, options, &result);
            fails += i32::from(fail);
            if !fail {
                log::info!(target: target, "pass");
            }
        }
    }

    assert!(fails == 0, "failed {fails}/{total}");
}

#[track_caller]
fn run_test_from_file(
    target: &str,
    sections: &Sections<'_>,
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
            target: target,
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
            log::warn!(target: target, "TODO: Compare flags");
        } else if options.contains("showflags") {
            log::error!(target: target, "Expected flags");
            fail = true;
        }

        if meta.title.is_some() {
            log::warn!(target: target, "TODO: Compare title");
        } else if options.contains("showtitle") {
            log::error!(target: target, "Expected title");
            fail = true;
        }

        if let Some(expected_toc) = &meta.toc {
            if expected_toc.len() != result.outline.len() {
                log::error!(
                    target: target,
                    "Outline length mismatch: expected {}, got {}",
                    expected_toc.len(),
                    result.outline.len()
                );
                fail = true;
            }

            // Keep going even if the length mismatch since maybe the
            // knowledge of where the mismatch happened is useful
            for (index, (expected, actual)) in
                expected_toc.iter().zip(result.outline.iter()).enumerate()
            {
                if expected.tag != actual.level.tag_name() {
                    log::error!(
                        target: target,
                        "Outline {index} tag mismatch: expected {}, got {}",
                        actual.level.tag_name(),
                        expected.tag
                    );
                    fail = true;
                }
                if expected.line != actual.html {
                    log::error!(
                        target: target,
                        "Outline {index} title mismatch: expected {:?}, got {:?}",
                        expected.line,
                        actual.html
                    );
                    fail = true;
                }
            }
        } else if options.contains("showtocdata") {
            log::error!(target: target, "Missing expected TOC");
            fail = true;
        }
    }

    fail
}

macro_rules! test_from_file {
    ($($ident:ident => $path:literal),* $(,)?) => {
        $(#[test]
        fn $ident() {
            run_tests_from_file($path, format!("{BASE_DIR}/{}.txt", $path));
        })*
    }
}

use test_from_file;
