use super::{
    config::CONFIG,
    test_parser::{Chunk, OPTION_TO_META, SectionText, Sections, Testfile},
};
use core::fmt::Write as _;
use http::Uri;
use libphp_rs::{DateTime, strtr};
use libwikitext_common::{
    db::{Article, DatabaseProvider, MockDatabase},
    decode_html,
};
use libwikitext_data::MESSAGES;
use libwikitext_parse::{FileMap, inspect};
use libwikitext_render::{
    LoadMode, Paths, PluginFnArgs, PluginParserFn, PluginResult, PluginState, RenderOutput,
    Statics, render_article,
};
use regex::{Regex, RegexBuilder};
use similar_asserts::SimpleDiff;
use std::{
    borrow::Cow,
    collections::HashMap,
    fs::File,
    io::Read as _,
    path::Path,
    sync::{Arc, LazyLock},
};

const BASE_DIR: &str = "./src/tests";

test_from_file! {
    annotation_parser_tests => "annotationParserTests",
    attribute_expander_tests => "attributeExpanderTests",
    bad_characters => "badCharacters",
    comments => "comments",
    definition_lists => "definitionLists",
    // dom_normalizer_tests => "domNormalizerTests",
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
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(None)
        .parse_default_env()
        .try_init();

    let code = {
        let mut file = File::open(&path).unwrap();
        let mut code = String::new();
        file.read_to_string(&mut code).unwrap();
        code
    };

    let base_time = DateTime::UNIX_EPOCH
        .replace_minute(2)
        .unwrap()
        .replace_second(3)
        .unwrap();

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
                db.insert(
                    Article::builder()
                        .id(0)
                        .body(text)
                        .title(title)
                        .model("wikitext")
                        .revision_id(1337)
                        .revision_timestamp(base_time.to_offset_time().to_utc())
                        .build(),
                );
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

    let mut db = Arc::new(db);

    for chunk in tests.chunks {
        if let Chunk::Test {
            name,
            pos,
            sections,
        } = chunk
        {
            let line = file_map.find_line_col(pos).line;
            let target = &format!("{suite}.txt:{line}");
            let options = sections.get("options").unwrap_or(&empty_options);

            let Some(wikitext) = sections.get("wikitext").and_then(SectionText::text) else {
                log::warn!(target: target, "Could not find wikitext for {name}!");
                continue;
            };

            if let Some(reason) = options.get("wiki-rs-skip") {
                log::info!(target: target, "Skipping {name:?}: {reason}");
                continue;
            }

            log::info!(target: target, "Running {name:?}");
            total += 1;

            let expect_failure = options.get("wiki-rs-expect-failure");
            let log_level = if expect_failure.is_some() {
                log::Level::Warn
            } else {
                log::Level::Error
            };

            let Some(result) =
                render_test(target, log_level, base_time, &mut db, options, wikitext)
            else {
                fails += 1;
                continue;
            };

            let fail = check_test_results(target, log_level, &sections, options, &result);

            if let Some(reason) = expect_failure {
                if fail {
                    log::warn!(target: target, "Ignoring test failure: {reason}");
                } else {
                    log::error!(target: target, "passed, but should have failed");
                    fails += 1;
                }
            } else {
                fails += i32::from(fail);
                if !fail {
                    log::info!(target: target, "pass");
                }
            }
        }
    }

    assert!(fails == 0, "failed {fails}/{total}");
}

struct DivTagPf;
impl PluginParserFn for DivTagPf {
    fn call(
        &self,
        out: &mut String,
        state: &mut PluginState<'_, '_, '_, '_>,
        args: PluginFnArgs<'_, '_, '_>,
    ) -> PluginResult {
        let tag = if args.callee() == "divtagpf" {
            "div"
        } else {
            "span"
        };

        let mut raw = false;
        let mut raw_html = false;
        let len = args.len();
        for index in 1..len {
            match &*args.eval(state, index)?.unwrap() {
                "raw" => raw = true,
                "isRawHTML" => raw_html = true,
                flag => log::warn!("TODO: #divtagpf: flag {flag}"),
            }
        }

        let content = if raw {
            args.eval(state, 0)
        } else if raw_html {
            args.eval_as_document(state, 0)
        } else {
            args.eval_as_fragment(state, 0)
        }?
        .unwrap_or_default();

        write!(out, "<{tag}>{content}</{tag}>")?;
        Ok(())
    }
}

#[track_caller]
fn render_test(
    target: &str,
    log_level: log::Level,
    base_time: DateTime,
    db: &mut Arc<MockDatabase<'static>>,
    options: &SectionText<'_>,
    wikitext: &str,
) -> Option<RenderOutput> {
    let page_name = options.get("title").unwrap_or("Parser test");

    // The use of static IDs and revision IDs is NOT SAFE in the presence of a
    // template cache!
    let article = Article::builder()
        .id(0)
        .title(page_name)
        .body(wikitext)
        .model("wikitext")
        .revision_author("127.0.0.1")
        .revision_timestamp(base_time.to_offset_time().to_utc())
        .revision_id(1337)
        .build();

    // This is some hack for magicWords.txt which uses some unknown
    // quirk of the MediaWiki test environment to selectively decide
    // that some pages exist, and I am not wasting my time digging in
    // there to figure out what that quirk is, exactly
    let insert_page = options.contains("lastsavedrevision") || page_name == "Parser test";

    if insert_page {
        Arc::get_mut(db).unwrap().insert(article.clone());
    }

    let mut statics = Statics::builder()
        .base_time(base_time)
        .base_uri(Uri::from_static("http://example.org"))
        .db(Arc::clone(&*db) as Arc<dyn DatabaseProvider>)
        .parser(db.config())
        .parser_fns(HashMap::from_iter([
            ("divtagpf", &DivTagPf as &dyn PluginParserFn),
            ("spantagpf", &DivTagPf as &dyn PluginParserFn),
        ]))
        .paths(Paths {
            article: "wiki",
            external: None,
            media: "http://example.com/images/3/3a",
        })
        .build();

    let result = render_article(
        &mut statics,
        &MESSAGES,
        &Arc::new(article),
        LoadMode::Module,
        false,
    );

    if let Err(err) = &result {
        log::log!(target: target, log_level, "Render failed: {err}");
    }

    if (result.is_err() || std::env::var("WIKI_RS_SHOW_AST").is_ok_and(|v| v == "1"))
        && let Ok(ast) = statics.parser.parse(wikitext, false)
    {
        log::info!(target: target, "AST: {:?}", inspect(&FileMap::new(wikitext), &ast.root));
    }

    if insert_page {
        drop(statics);
        Arc::get_mut(db).unwrap().remove(page_name);
    }

    result.ok()
}

#[track_caller]
fn check_test_results(
    target: &str,
    log_level: log::Level,
    sections: &Sections<'_>,
    options: &SectionText<'_>,
    result: &RenderOutput,
) -> bool {
    let expected_html = sections.get("html/wiki.rs");
    let try_heuristics = expected_html.is_none();
    let expected_html = expected_html
        .or_else(|| sections.get("html/php"))
        .or_else(|| sections.get("html/*"))
        .or_else(|| sections.get("html"))
        // The parsoid output is almost always full of garbage but in a few
        // cases wiki.rs actually matches its output, so use it as a last resort
        .or_else(|| sections.get("html/parsoid"))
        .and_then(SectionText::text);

    let expected_meta = sections
        .get("metadata/wiki.rs")
        .or_else(|| sections.get("metadata/php"))
        .or_else(|| sections.get("metadata"))
        .and_then(SectionText::meta);

    let mut fail = false;
    if let Some(expected_html) = expected_html {
        // The `{{PAGESIZE}}` test requires to not have trailing eol in the
        // article text, the MW `BlockLevelPass` seems to try to avoid it, but
        // this does not matter at all for HTML output, so just trim any
        // whitespace and then cry later when it turns out it does matter for
        // some insane esoteric edge case
        let actual = result.content.trim_ascii_end();

        fail = expected_html != actual;

        // To avoid having to copy and paste a thousand different outputs that
        // differ only in superficials, use various stupid heuristic methods to
        // force the expected and actual results to kiss. Only do this when
        // there is no explicit wiki.rs section, since the heuristics may mask
        // actually wrong output.
        if fail && try_heuristics {
            static RE_PHP_URL: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"/index\.php\?title=([^&]+)(?:&amp;)?").unwrap());
            static RE_PHP_HEADING: LazyLock<Regex> = LazyLock::new(|| {
                RegexBuilder::new(r#"^<div class="mw-heading mw-heading\d">(.*?)(?:<span class="mw-editsection"><span class="mw-editsection-bracket">\[</span><a href="[^"]+" title="[^"]+">edit</a><span class="mw-editsection-bracket">]</span></span>)?</div>$"#)
                    .multi_line(true)
                    .build()
                    .unwrap()
            });

            let mut heuristic = "unpretty";
            let actual = strtr(
                actual,
                &[
                    ("<wbr>", "<wbr />"),
                    ("<br>", "<br />"),
                    ("<hr>", "<hr />"),
                    ("‘", "'"),
                    ("’", "'"),
                    ("“", "\""),
                    ("”", "\""),
                    ("…", "..."),
                ],
            );
            fail = expected_html != actual;

            if fail
                && let Cow::Owned(expected_html) =
                    strtr(expected_html, &[("</tbody>", ""), ("<tbody>", "")])
            {
                heuristic = "unpretty + remove tbody";
                fail = expected_html != actual;

                if fail && let Cow::Owned(expected_html) = styles(&expected_html) {
                    heuristic = "unpretty + remove tbody + styles";
                    fail = expected_html != actual;
                }
            }

            if fail && let Cow::Owned(expected_html) = styles(expected_html) {
                heuristic = "unpretty + styles";
                fail = expected_html != actual;
            }

            if fail
                && let Cow::Owned(expected_html) = RE_PHP_HEADING.replace_all(expected_html, "$1")
            {
                heuristic = "unpretty + unwrap heading";
                fail = expected_html != actual;

                if fail && let Cow::Owned(expected_html) = decode_html(&expected_html) {
                    heuristic = "unpretty + unwrap heading + decode html";
                    fail = expected_html != actual;
                }
            }

            if fail && let Cow::Owned(expected_html) = decode_html(expected_html) {
                heuristic = "unpretty + decode html";
                fail = expected_html != actual;
            }

            if fail
                && let Cow::Owned(expected_html) =
                    RE_PHP_URL.replace_all(expected_html, "/wiki/$1?")
            {
                heuristic = "unpretty + replace url";
                fail = expected_html != actual;
            }

            if !fail {
                log::warn!("Passed using the {heuristic} heuristic");
            }
        }

        if fail {
            let diff = SimpleDiff::from_str(expected_html, actual, "expected", "actual");
            log::log!(target: target, log_level, "{diff}");
        }
    } else if !options.contains("nohtml") && !result.content.is_empty() {
        log::log!(target: target, log_level, "Missing expected HTML");
        fail = true;
    }

    if let Some(meta) = expected_meta {
        for (option, meta_key) in &OPTION_TO_META {
            if meta.kvs.contains_key(meta_key) {
                log::warn!(target: target, "TODO: Compare {option}");
            } else if options.contains(option) {
                log::log!(target: target, log_level, "Expected {option}");
                fail = true;
            }
        }

        if meta.text.is_some() {
            log::warn!(target: target, "TODO: Compare title or indicators");
        } else if options.contains("showindicators") {
            log::log!(target: target, log_level, "Expected indicators");
            fail = true;
        } else if options.contains("showtitle") {
            log::log!(target: target, log_level, "Expected title");
            fail = true;
        }

        if meta.cats.is_some() {
            log::warn!(target: target, "TODO: Compare categories");
        } else if options.contains("cat") {
            log::log!(target: target, log_level, "Expected categories");
            fail = true;
        }

        if meta.text.is_some() {
            log::warn!(target: target, "TODO: Compare title");
        } else if options.contains("showtitle") {
            log::log!(target: target, log_level, "Expected title");
            fail = true;
        }

        if let Some(expected_toc) = &meta.toc {
            if expected_toc.len() != result.outline.len() {
                log::log!(
                    target: target,
                    log_level,
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
                    log::log!(
                        target: target,
                        log_level,
                        "Outline {index} tag mismatch: expected {}, got {}",
                        actual.level.tag_name(),
                        expected.tag
                    );
                    fail = true;
                }
                if expected.line != actual.html {
                    log::log!(
                        target: target,
                        log_level,
                        "Outline {index} title mismatch: expected {:?}, got {:?}",
                        expected.line,
                        actual.html
                    );
                    fail = true;
                }
            }
        } else if options.contains("showtocdata") {
            log::log!(target: target, log_level, "Missing expected TOC");
            fail = true;
        }
    }

    fail
}

#[track_caller]
fn styles(expected_html: &str) -> Cow<'_, str> {
    static RE_PREFIX_STYLES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"style="([^"]+)""#).unwrap());
    static RE_PREFIX_STYLE_DECL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*([^\s:]+)\s*:\s*([^;]+);?\s*").unwrap());

    fn re_style(caps: &regex::Captures<'_>) -> String {
        let (orig, [decls]) = caps.extract();
        if let Cow::Owned(s) = RE_PREFIX_STYLE_DECL.replace_all(decls, "--mw-output-$1:$2;") {
            format!(r#"style="{s}""#)
        } else {
            orig.to_owned()
        }
    }

    RE_PREFIX_STYLES.replace_all(expected_html, re_style)
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
