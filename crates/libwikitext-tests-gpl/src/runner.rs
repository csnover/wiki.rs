use super::{
    config::CONFIG,
    parser::{Chunk, OPTION_TO_META, SectionText, Sections, Testfile},
};
use core::{cell::RefCell, fmt::Write as _};
use libphp_rs::{DateTime, strtr};
use libwikitext_common::{
    db::{Article, DatabaseProvider, MockDatabase},
    decode_html,
    url::Url,
};
use libwikitext_data::MESSAGES;
use libwikitext_parse::{FileMap, Parser, inspect};
use libwikitext_render::{
    LoadMode, OutputMode, Paths, PluginExtensionTag, PluginFnArgs, PluginParserFn, PluginResult,
    PluginState, PluginTagArgs, RenderOutput, Statics, preprocess_article, render_article,
};
use regex::{Regex, RegexBuilder};
use serde_json_borrow::Value;
use similar_asserts::SimpleDiff;
use std::{
    borrow::Cow,
    collections::HashMap,
    fs::File,
    io::Read as _,
    path::Path,
    sync::{Arc, LazyLock, Mutex},
};

struct AsideTag;

impl PluginExtensionTag for AsideTag {
    fn call(
        &self,
        out: &mut String,
        _: &mut PluginState<'_, '_, '_, '_>,
        _: PluginTagArgs<'_, '_, '_>,
    ) -> PluginResult<OutputMode> {
        write!(out, "<aside>Some aside content</aside>")?;
        Ok(OutputMode::Block)
    }
}

struct DivTagPf;

impl PluginExtensionTag for DivTagPf {
    fn call(
        &self,
        out: &mut String,
        state: &mut PluginState<'_, '_, '_, '_>,
        args: PluginTagArgs<'_, '_, '_>,
    ) -> PluginResult<OutputMode> {
        let tag = if args.tag_name() == "divtag" {
            "div"
        } else {
            "span"
        };

        let raw = args.get(state, "raw")?.is_some();
        let raw_html = args.get(state, "israwhtml")?.is_some();
        let content = if let Some(body) = args.body() {
            if raw {
                Cow::Borrowed(body)
            } else {
                Cow::Owned(args.eval(state, body, raw_html)?)
            }
        } else {
            <_>::default()
        };
        write!(out, "<{tag}>{content}</{tag}>")?;
        Ok(if tag == "div" {
            OutputMode::Block
        } else {
            OutputMode::Inline
        })
    }
}

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
            match args.eval(state, index)?.unwrap().as_ref() {
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

struct PWrapTest;

impl PluginExtensionTag for PWrapTest {
    fn call(
        &self,
        out: &mut String,
        _: &mut PluginState<'_, '_, '_, '_>,
        _: PluginTagArgs<'_, '_, '_>,
    ) -> PluginResult<OutputMode> {
        write!(out, "<!--CMT--><style>p{{}}</style>")?;
        Ok(OutputMode::Inline)
    }
}

struct SealTag;

impl PluginExtensionTag for SealTag {
    fn call(
        &self,
        out: &mut String,
        _: &mut PluginState<'_, '_, '_, '_>,
        _: PluginTagArgs<'_, '_, '_>,
    ) -> PluginResult<OutputMode> {
        write!(out, "<span></span>")?;
        Ok(OutputMode::Inline)
    }
}

#[derive(Default)]
struct StaticTag(Mutex<RefCell<String>>);

impl PluginExtensionTag for StaticTag {
    fn call(
        &self,
        out: &mut String,
        state: &mut PluginState<'_, '_, '_, '_>,
        args: PluginTagArgs<'_, '_, '_>,
    ) -> PluginResult<OutputMode> {
        Ok(if args.get(state, "action")?.as_deref() == Some("flush") {
            write!(
                out,
                "{}",
                core::mem::take(&mut *self.0.lock().unwrap().borrow_mut())
            )?;
            OutputMode::Raw
        } else {
            if let Some(body) = args.body() {
                self.0.lock().unwrap().borrow_mut().push_str(body);
            }
            OutputMode::Empty
        })
    }
}

static STATIC_TAG: LazyLock<StaticTag> = LazyLock::new(StaticTag::default);

struct TagTag;

impl PluginExtensionTag for TagTag {
    fn call(
        &self,
        out: &mut String,
        state: &mut PluginState<'_, '_, '_, '_>,
        args: PluginTagArgs<'_, '_, '_>,
    ) -> PluginResult<OutputMode> {
        fn escape_single_quote(text: &str) -> Cow<'_, str> {
            strtr(text, &[("'", "\\'")])
        }

        write!(out, "<pre>")?;
        if let Some(body) = args.body() {
            write!(out, "'{}'", escape_single_quote(body))?;
        } else {
            out.push_str("NULL");
        }
        out.push_str("\narray (\n");
        for arg in args.iter(state) {
            let (name, value) = arg?;
            write!(out, "  '{}' => ", escape_single_quote(&name))?;
            if let Some(value) = value {
                writeln!(out, "'{}',", escape_single_quote(&value))?;
            } else {
                writeln!(out, "NULL,")?;
            }
        }
        write!(out, ")\n</pre>")?;
        Ok(OutputMode::Block)
    }
}

#[track_caller]
pub(super) fn run_tests_from_file(suite: &str, path: impl AsRef<Path>) {
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

    let mut db = MockDatabase::new("MediaWiki", &CONFIG);

    let (mut total, mut fails, mut skips) = (0, 0, 0);

    // Like everything in MediaWiki, multiple passes are *required* to parse the
    // test suite file correctly
    load_articles(base_time, &mut db, &tests.chunks);

    let mut db = Arc::new(db);

    for chunk in tests.chunks {
        #[rustfmt::skip]
        let Chunk::Test { name, pos, sections, } = chunk else {
            continue;
        };

        total += 1;

        let name = strtr(name, &[("\n", " - ")]);
        let line = file_map.find_line_col(pos).line;
        let target = &format!("{suite}.txt:{line}");
        let options = sections.get("options").unwrap_or(&empty_options);
        let config = sections.get("config").unwrap_or(&empty_options);

        let Some(wikitext) = sections.get("wikitext").and_then(SectionText::text) else {
            log::warn!(target: target, "Could not find wikitext for {name}!");
            skips += 1;
            continue;
        };

        if check_skips(target, &name, options, config) {
            skips += 1;
            continue;
        }

        log::info!(target: target, "Running {name}");

        let expect_failure = options.get::<&str>("wiki-rs-expect-failure");
        let log_level = if expect_failure.is_some() {
            log::Level::Warn
        } else {
            log::Level::Error
        };

        let Some(result) = render_test(target, log_level, base_time, &mut db, options, wikitext)
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

    let passes = total - fails - skips;
    assert!(
        fails == 0,
        "{passes} passed; {fails} failed; {skips} ignored"
    );
}

fn check_skips(
    target: &str,
    name: &str,
    options: &SectionText<'_>,
    config: &SectionText<'_>,
) -> bool {
    if let Some(reason) = options.get::<&str>("wiki-rs-skip") {
        log::info!(target: target, "Skipping {name}: {reason}");
        true
    } else if options.contains("comment") {
        // This is some parser variant used only for revision comments
        log::warn!(target: target, "TODO {name}: comment not implemented");
        true
    } else if options.contains("msg") {
        // This is some parser variant used only for interface messages
        log::warn!(target: target, "TODO {name}: msg not implemented");
        true
    } else if options.contains("preload") {
        log::warn!(target: target, "TODO {name}: preload not implemented");
        true
    } else if options.get("styletag") == Some(true) {
        log::warn!(target: target, "TODO {name}: styletag not implemented");
        true
    } else if config.get("wgInterwikiMagic") == Some(false) {
        log::warn!(target: target, "TODO {name}: disable interwiki magic not implemented");
        true
    } else if config.get("wgAllowDisplayTitle") == Some(false) {
        log::warn!(target: target, "TODO {name}: disable {{{{DISPLAYTITLE}}}} not implemented");
        true
    } else if config.get("wgRawHtml") == Some(true) || options.get("wgrawhtml") == Some(true) {
        log::warn!(target: target, "TODO {name}: raw html not implemented");
        true
    } else if options.contains("section") {
        log::warn!(target: target, "TODO {name}: section extraction not implemented");
        true
    } else if options.contains("pmid-interwiki") {
        log::warn!(target: target, "TODO {name}: pmid-interwiki not implemented");
        true
    } else if options.contains("replace") {
        log::warn!(target: target, "TODO {name}: section replacement not implemented");
        true
    } else if options.contains("language") {
        log::warn!(target: target, "TODO {name}: language switching not implemented");
        true
    } else if options.contains("wgNonincludableNamespaces") {
        log::warn!(target: target, "TODO {name}: runtime non-includable namespaces not implemented");
        true
    } else if options.contains("maxincludesize") || options.contains("maxtemplatedepth") {
        log::warn!(target: target, "TODO {name}: resource limits not implemented");
        true
    } else if options.contains("disabled") {
        log::info!(target: target, "Skipping {name}: disabled");
        true
    } else if options.contains("pst") {
        log::info!(target: target, "Skipping {name}: pre-save transform");
        true
    } else if options.contains("annotations") {
        log::info!(target: target, "Skipping {name}: Parsoid annotations");
        true
    } else if options
        .get("parsoid.modes")
        .is_some_and(any_of(&["html2wt"]))
    {
        log::info!(target: target, "Skipping {name}: Parsoid html2wt");
        true
    } else if options
        .get::<&[Value<'_>]>("parsoid.modes")
        .is_some_and(|modes| modes.len() == 1 && modes[0].as_str() == Some("wt2wt"))
    {
        log::info!(target: target, "Skipping {name}: Parsoid wt2wt-only");
        true
    } else if options
        .get("parsoid.modes")
        .is_some_and(any_of(&["selser"]))
    {
        log::info!(target: target, "Skipping {name}: Parsoid selser");
        true
    } else {
        false
    }
}

fn load_articles(base_time: DateTime, db: &mut MockDatabase<'_>, chunks: &[Chunk<'_>]) {
    let redirect_parser = Parser::new(&CONFIG);
    for chunk in chunks {
        match chunk {
            Chunk::Article { title, text } => {
                let article = Article::builder()
                    .id(0)
                    .body(text)
                    .title(title)
                    .model("wikitext")
                    .revision_id(1337)
                    .revision_timestamp(base_time.to_offset_time().to_utc());

                db.insert(if let Ok(redirect) = redirect_parser.parse_redirect(text) {
                    article.redirect(redirect).build()
                } else {
                    article.build()
                });
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
}

fn print_debug(
    target: &str,
    statics: &mut Statics<'_>,
    article: &Arc<Article>,
    show_pp_ast: bool,
    show_pp: bool,
    show_ast: bool,
) {
    let pp_ast = show_pp_ast.then(|| statics.parser.preprocess(article.body(), false));

    let pp = (show_pp || show_ast)
        .then(|| preprocess_article(statics, &MESSAGES, article, LoadMode::Module, false));

    let ast = if show_ast && let Some(Ok(pp)) = &pp {
        Some((pp, statics.parser.parse(pp)))
    } else {
        None
    };

    if let Some(Ok(pp_ast)) = pp_ast {
        log::info!(target: target, "PP AST: {:?}", inspect(&FileMap::new(article.body()), &pp_ast.root));
    }

    if let Some(Ok(pp)) = &pp {
        log::info!(target: target, "PP:\n{pp}");
    }

    if let Some((pp, Ok(ast))) = ast {
        log::info!(target: target, "AST: {:?}", inspect(&FileMap::new(pp), &ast));
    }
}

fn any_of(choices: &[&str]) -> impl FnOnce(&[Value<'_>]) -> bool {
    |values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .any(|value| choices.contains(&value))
    }
}

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
        .or_else(|| {
            let use_parsoid_metadata = options.contains("wiki-rs-use-parsoid-metadata");
            sections.get(if use_parsoid_metadata {
                "metadata/parsoid"
            } else {
                "metadata/php"
            })
        })
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
            let mut heuristic = "unpretty";
            let actual = unpretty(actual);
            fail = expected_html != actual;

            if fail && let Cow::Owned(actual) = list_ws(&actual) {
                heuristic = "unpretty + list ws";
                fail = expected_html != actual;
            }

            if fail && let Cow::Owned(expected_html) = remove_tbody(expected_html) {
                heuristic = "unpretty + remove tbody";
                fail = expected_html != actual;

                if fail
                    && let actual = table_ws(&actual)
                    && let expected_html = table_ws(&expected_html)
                    && (matches!(actual, Cow::Owned(_)) || matches!(expected_html, Cow::Owned(_)))
                {
                    heuristic = "unpretty + remove tbody + table ws";
                    fail = expected_html != actual;

                    if fail && let Cow::Owned(expected_html) = unwrap_heading(&expected_html) {
                        heuristic = "unpretty + remove tbody + table ws + unwrap heading";
                        fail = expected_html != actual;
                    }
                }

                if fail && let Cow::Owned(expected_html) = replace_url(&expected_html) {
                    heuristic = "unpretty + remove tbody + replace url";
                    fail = expected_html != actual;
                }

                if fail && let Cow::Owned(expected_html) = styles(&expected_html) {
                    heuristic = "unpretty + remove tbody + styles";
                    fail = expected_html != actual;

                    if fail && let Cow::Owned(expected_html) = decode_html(&expected_html) {
                        heuristic = "unpretty + remove tbody + styles + decode html";
                        fail = expected_html != actual;
                    }
                } else if fail && let Cow::Owned(expected_html) = decode_html(&expected_html) {
                    heuristic = "unpretty + remove tbody + decode html";
                    fail = expected_html != actual;
                }
            }

            if fail && let Cow::Owned(expected_html) = styles(expected_html) {
                heuristic = "unpretty + styles";
                fail = expected_html != actual;
            }

            if fail && let Cow::Owned(expected_html) = unwrap_heading(expected_html) {
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

            if fail && let Cow::Owned(expected_html) = replace_url(expected_html) {
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
            } else if options.contains(option)
                && options.get::<&str>("wiki-rs-expect-missing") == Some(option)
            {
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

        if let Some(cats) = &meta.cats {
            for missing in cats.iter().filter(|cat| {
                !result
                    .categories
                    .contains(&strtr(cat.category, &[("_", " ")]))
            }) {
                log::log!(target: target, log_level, "Missing expected category {:?}", missing.category);
                fail = true;
            }
        } else if options.contains("cat") && !result.categories.is_empty() {
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

fn list_ws(html: &str) -> Cow<'_, str> {
    strtr(html, &[("</li>\n<li>", "</li><li>")])
}

fn remove_tbody(html: &str) -> Cow<'_, str> {
    strtr(html, &[("</tbody>", ""), ("<tbody>", "")])
}

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
        .base_uri(Url::from_static("http://example.org").unwrap())
        .extension_tags(HashMap::from_iter([
            ("asidetag", &AsideTag as _),
            ("divtag", &DivTagPf as _),
            ("pwraptest", &PWrapTest as _),
            ("sealtag", &SealTag as _),
            ("spantag", &DivTagPf as _),
            ("statictag", &*STATIC_TAG as _),
            ("tag", &TagTag as _),
            ("tåg", &TagTag as _),
        ]))
        .db(Arc::clone(db) as Arc<dyn DatabaseProvider>)
        .parser(db.config())
        .parser_fns(HashMap::from_iter([
            ("divtagpf", &DivTagPf as _),
            ("spantagpf", &DivTagPf as _),
        ]))
        .paths(Paths {
            article: "wiki",
            external: None,
            media: "http://example.com/images/3/3a",
        })
        .build();

    let article = Arc::new(article);

    let before_pp_ast = wants_pp_ast(target);
    let before_pp = wants_pp(target);
    let before_ast = wants_ast(target);

    print_debug(
        target,
        &mut statics,
        &article,
        before_pp_ast,
        before_pp,
        before_ast,
    );

    let result = render_article(&mut statics, &MESSAGES, &article, LoadMode::Module, false);

    if let Err(err) = &result {
        log::log!(target: target, log_level, "Render failed: {err}");
    }

    if result.is_err() {
        print_debug(
            target,
            &mut statics,
            &article,
            !before_pp_ast,
            !before_pp,
            !before_ast,
        );
    }

    if insert_page {
        drop(statics);
        Arc::get_mut(db).unwrap().remove(page_name);
    }

    result.ok()
}

fn replace_url(html: &str) -> Cow<'_, str> {
    static RE_PHP_URL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"/index\.php\?title=([^&]+)(?:&amp;)?").unwrap());
    RE_PHP_URL.replace_all(html, "/wiki/$1?")
}

fn styles(html: &str) -> Cow<'_, str> {
    static RE_PREFIX_STYLES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(<[^>]+ )style="([^"]+)""#).unwrap());
    static RE_PREFIX_STYLE_DECL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*([^\s:]+)\s*:\s*([^;]+);?\s*").unwrap());

    fn re_style(caps: &regex::Captures<'_>) -> String {
        let (orig, [prefix, decls]) = caps.extract();
        if let Cow::Owned(s) = RE_PREFIX_STYLE_DECL.replace_all(decls, "--mw-output-$1:$2;") {
            format!(r#"{prefix}style="{s}""#)
        } else {
            orig.to_owned()
        }
    }

    RE_PREFIX_STYLES.replace_all(html, re_style)
}

fn table_ws(html: &str) -> Cow<'_, str> {
    static RE_WS: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"(<(?:/(?:caption|td|th)|table|/?tr)[^>\n]*>)\n\s*")
            .case_insensitive(true)
            .build()
            .unwrap()
    });
    RE_WS.replace_all(html, "$1$2")
}

fn unpretty(html: &str) -> Cow<'_, str> {
    static REPLS: &[(&str, &str)] = &[
        ("<wbr>", "<wbr />"),
        ("<br>", "<br />"),
        ("<hr>", "<hr />"),
        ("‘", "'"),
        ("’", "'"),
        ("“", "\""),
        ("”", "\""),
        ("…", "..."),
    ];
    strtr(html, REPLS)
}

fn unwrap_heading(html: &str) -> Cow<'_, str> {
    static RE_PHP_HEADING: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r#"^<div class="mw-heading mw-heading\d">(.*?)(?:<span class="mw-editsection"><span class="mw-editsection-bracket">\[</span><a href="[^"]+" title="[^"]+">edit</a><span class="mw-editsection-bracket">]</span></span>)?</div>$"#)
            .multi_line(true)
            .build()
            .unwrap()
    });
    RE_PHP_HEADING.replace_all(html, "$1")
}

fn wants_ast(target: &str) -> bool {
    std::env::var("WIKI_RS_SHOW_AST").is_ok_and(|v| v == "1" || target.contains(&v))
}

fn wants_pp(target: &str) -> bool {
    std::env::var("WIKI_RS_SHOW_PP").is_ok_and(|v| v == "1" || target.contains(&v))
}

fn wants_pp_ast(target: &str) -> bool {
    std::env::var("WIKI_RS_SHOW_PP_AST").is_ok_and(|v| v == "1" || target.contains(&v))
}
