//! Routes for axum.

use crate::{
    AppState, LoadMode,
    config::CONFIG,
    db,
    renderer::{self, RenderOutput},
    title::Title,
    wikitext::{FileMap, Parser, inspect},
};
use axum::{
    Form,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use rayon::{iter::ParallelIterator, slice::ParallelSliceMut};
use sailfish::TemplateSimple;
use std::{
    num::NonZeroUsize,
    sync::{Arc, mpsc},
    time::Instant,
};

/// All errors that may occur during page rendering.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// An article database error.
    #[error(transparent)]
    Database(#[from] db::Error),
    /// A Wikitext article renderer error.
    #[error(transparent)]
    Renderer(#[from] renderer::Error),
    /// A templating engine error.
    #[error(transparent)]
    Template(#[from] sailfish::RenderError),
    /// A source code viewer syntax highlighter error.
    #[error(transparent)]
    Source(#[from] syntect::Error),
    /// A source code viewer syntax string formatting error.
    #[error(transparent)]
    Fmt(#[from] core::fmt::Error),
    /// A renderer thread message transmission error.
    #[error(transparent)]
    RenderTx(#[from] mpsc::SendError<renderer::In>),
    /// A renderer thread message receipt error.
    #[error(transparent)]
    RenderRx(#[from] mpsc::RecvError),
    /// An renderer thread pool management error.
    #[error(transparent)]
    Pool(#[from] r2d2::Error),
    /// A non-utf-8 header could not be converted to a string.
    #[error(transparent)]
    ToStr(#[from] axum::http::header::ToStrError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::Database(error) => match error {
                db::Error::NotFound => (StatusCode::NOT_FOUND, format!("{error}")),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            },
            Error::Renderer(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::Template(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::Source(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::Fmt(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::RenderTx(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::RenderRx(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::Pool(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
            Error::ToStr(error) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")),
        }
        .into_response()
    }
}

/// The actions supported *by wiki.rs* for an article page.
#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ArticleAction {
    /// View.
    View,
    /// Render. Treated the same as [`View`](ArticleAction::View).
    Tree,
}

/// Query options for `/article`.
#[derive(serde::Deserialize)]
pub(crate) struct ArticleQuery {
    /// The page action.
    ///
    /// This is defined, but not actually used, because most actions are not
    /// relevant to wiki.rs, but templates will still generate links using other
    /// actions, and *something* should happen in that case (which currently
    /// is just returning an error page that the action is invalid).
    #[serde(rename(deserialize = "action"))]
    _action: Option<ArticleAction>,

    /// The load strategy.
    mode: Option<LoadMode>,

    /// Controls automatic redirection in the view action.
    redirect: Option<String>,
}

/// The article page route handler.
pub(crate) async fn article(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(ArticleQuery {
        _action,
        mode: load_mode,
        redirect,
    }): Query<ArticleQuery>,
) -> Result<impl IntoResponse, Error> {
    #[derive(TemplateSimple)]
    #[template(path = "article.html")]
    struct ArticleTemplate<'a> {
        /// The base path for URLs.
        base_path: &'a str,
        /// The base URI for URLs.
        base_uri: &'a str,
        /// The canonical URL of the article.
        canonical: &'a str,
        /// The title of the article.
        title: &'a str,
        /// The Wikitext renderer output.
        output: &'a RenderOutput,
        /// The name of the wiki.
        site: &'a str,
    }

    let name = name.replace('_', " ");
    let mut iter = name.chars();
    let first = iter.next();
    let rest = iter.as_str();
    let name = if let Some(first) = first {
        first.to_uppercase().to_string() + rest
    } else {
        name
    };

    let article = state.database.get(&Title::new(&name, None))?;
    let redirect = redirect.as_deref() != Some("no");
    let canonical = if redirect && let Some(title) = &article.redirect {
        title
    } else {
        &name
    };

    let start = Instant::now();
    let load_mode = load_mode.unwrap_or(state.load_mode);

    let command = renderer::Command::Article {
        article: Arc::clone(&article),
        load_mode,
        redirect,
    };

    let output = call_renderer(&state, command)?;

    log::trace!("Rendered article in {:.2?}", start.elapsed());

    let base_uri = if state.base_uri.scheme_str().is_some() {
        state.base_uri.to_string()
    } else {
        format!("http://{}", state.base_uri)
    };

    ArticleTemplate {
        base_path: state.base_uri.path(),
        base_uri: &base_uri,
        canonical,
        title: &article.title,
        output: &output,
        site: state.database.name(),
    }
    .render_once()
    .map(html_result)
    .map(IntoResponse::into_response)
    .map_err(Into::into)
}

/// The ad-hoc Wikitext expression evaluator, initial page.
pub(crate) async fn eval_get(State(state): State<AppState>) -> Result<impl IntoResponse, Error> {
    raw_source(state.base_uri.path(), "", "html", Some(<_>::default()))
        .map(IntoResponse::into_response)
}

/// Preprocessor display options for `/eval`.
#[derive(Clone, Copy, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EvalPp {
    /// Show unprocessed source as a tree.
    Tree,
    /// Show preprocessed source.
    Pre,
    /// Show preprocessed source as a tree.
    PreTree,
    /// Show post-processed result.
    #[default]
    Post,
}

/// Form options for `/eval`.
#[derive(Default, serde::Deserialize)]
pub(crate) struct EvalForm {
    /// Arguments to set in the environment for parameter replacements.
    args: String,
    /// The Wikitext to evaluate.
    code: String,
    /// If `Some(true)`, treat the Wikitext as if it is being included in
    /// another page.
    include: Option<bool>,
    /// If `Some(true)`, also show the contents of strip markers.
    markers: Option<bool>,
    /// If `Some(true)`, show the parse tree instead of the rendered output.
    mode: EvalPp,
    /// The name to use for the root frame.
    page_name: String,
}

/// The ad-hoc Wikitext expression evaluator.
pub(crate) async fn eval_post(
    State(state): State<AppState>,
    Form(body): Form<EvalForm>,
) -> Result<impl IntoResponse, Error> {
    let command = renderer::Command::Eval {
        args: (body.include == Some(true)).then(|| body.args.clone()),
        code: body.code.clone(),
        markers: body.markers == Some(true),
        mode: body.mode,
        page_name: body.page_name.clone(),
    };
    let time = Instant::now();
    let output = call_renderer(&state, command)?;
    log::trace!("Rendered article in {:.2?}", time.elapsed());
    raw_source(state.base_uri.path(), &output.content, "html", Some(body))
}

/// The external link page route handler.
pub(crate) async fn external(
    State(state): State<AppState>,
    Path(mut target): Path<String>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<impl IntoResponse, Error> {
    #[derive(TemplateSimple)]
    #[template(path = "external.html")]
    struct ExternalLink<'a> {
        /// The base path for URLs.
        base_path: &'a str,
        /// The destination URL.
        target: String,
        /// The URL of the referring page, spelt properly to spite RFC 2616.
        referrer: Option<&'a str>,
        /// The name of the wiki.
        site: &'a str,
    }

    if let Some(query) = query {
        target.push('?');
        target += &query;
    }

    let referrer = headers
        .get(header::REFERER)
        .map(|header| header.to_str())
        .transpose()?;

    ExternalLink {
        base_path: state.base_uri.path(),
        target,
        referrer,
        site: state.database.name(),
    }
    .render_once()
    .map(html_result)
    .map_err(Into::into)
}

/// Returns an HTML response with appropriate cache-control headers.
fn html_result(result: String) -> impl IntoResponse {
    (
        [
            (header::CACHE_CONTROL, "max-age=604800, public"),
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        ],
        result,
    )
}

/// The font resource route handler.
pub(crate) async fn fonts(Path(font): Path<String>) -> impl IntoResponse {
    const FONTS: &[(&str, &[u8])] = &[
        (
            "Archivo.woff2",
            include_bytes!("../res/fonts/Archivo.woff2"),
        ),
        (
            "Archivo-Italic.woff2",
            include_bytes!("../res/fonts/Archivo-Italic.woff2"),
        ),
        (
            "Inconsolata.woff2",
            include_bytes!("../res/fonts/Inconsolata.woff2"),
        ),
        (
            "SourceSerif4.woff2",
            include_bytes!("../res/fonts/SourceSerif4.woff2"),
        ),
        (
            "SourceSerif4-Italic.woff2",
            include_bytes!("../res/fonts/SourceSerif4-Italic.woff2"),
        ),
    ];

    if let Some(body) = FONTS
        .iter()
        .find_map(|(name, data)| (*name == font).then_some(*data))
    {
        Ok((
            [
                (header::CONTENT_TYPE, "font/woff2"),
                (header::CACHE_CONTROL, "max-age=806400, public, immutable"),
            ],
            body,
        ))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// The media resource route handler.
pub(crate) async fn media(Path(_): Path<String>) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "max-age=806400, public, immutable"),
        ],
        include_str!("../res/placeholder.svg"),
    )
}

/// The index page route handler.
pub(crate) async fn index_page(State(state): State<AppState>) -> Result<impl IntoResponse, Error> {
    #[derive(TemplateSimple)]
    #[template(path = "index.html")]
    struct Index<'a> {
        /// The base path for URLs.
        base_path: &'a str,
        /// The name of the wiki.
        site: &'a str,
    }

    Index {
        base_path: state.base_uri.path(),
        site: state.database.name(),
    }
    .render_once()
    .map(html_result)
    .map_err(Into::into)
}

/// Query options for `/search`.
#[derive(serde::Deserialize)]
pub(crate) struct SearchQuery {
    /// The search query. This is treated as a regular expression string.
    q: String,
    /// The current page of search results to view.
    page: Option<NonZeroUsize>,
    /// The number of results per page.
    per_page: Option<NonZeroUsize>,
}

/// The search results route handler.
pub(crate) async fn search(
    State(state): State<AppState>,
    Query(SearchQuery {
        q: query,
        page,
        per_page,
    }): Query<SearchQuery>,
) -> impl IntoResponse {
    #[derive(TemplateSimple)]
    #[template(path = "search.html")]
    struct SearchResult<'a> {
        /// The base path for URLs.
        base_path: &'a str,
        /// The query string.
        query: &'a str,
        /// The search results.
        results: &'a [&'a str],
        /// Total number of results.
        total: usize,
        /// Current page number.
        page: usize,
        /// Number of results per page.
        per_page: usize,
        /// Total number of result pages.
        page_count: usize,
        /// The name of the wiki.
        site: &'a str,
    }

    let plain = regex::escape(&query) == query;
    let query = regex::RegexBuilder::new(&query)
        .case_insensitive(true)
        .build()
        .unwrap();

    log::debug!("Searching for {query}");
    let time = Instant::now();
    // Hard limit of 100_000 is chosen arbitrarily.
    // TODO: Actually look to see what kind of resource restriction, if any, is
    // appropriate here.
    let mut results = state
        .database
        .search(&query)
        .take_any(100_000)
        .collect::<Vec<&str>>();
    log::trace!("Found {} matches in {:.2?}", results.len(), time.elapsed());

    if plain {
        fn starts_with(a: unicase::UniCase<&str>, b: unicase::UniCase<&str>) -> bool {
            b.get(0..a.len())
                .is_some_and(|b| a == unicase::UniCase::new(b))
        }

        let query = unicase::UniCase::new(query.as_str());
        results.par_sort_unstable_by(|a, b| {
            let a = unicase::UniCase::new(*a);
            let b = unicase::UniCase::new(*b);
            match (starts_with(query, a), starts_with(query, b)) {
                (true, false) => core::cmp::Ordering::Less,
                (false, true) => core::cmp::Ordering::Greater,
                (_, _) => a.cmp(&b),
            }
        });
    } else {
        results.par_sort_unstable_by(|a, b| {
            let a = unicase::UniCase::new(*a);
            let b = unicase::UniCase::new(*b);
            a.cmp(&b)
        });
    }
    log::trace!("Sorted results in {:.2?}", time.elapsed());

    let per_page = per_page.map_or(500, usize::from);
    let page = page.map_or(0, |page| usize::from(page) - 1);
    let page_count = results.len().div_ceil(per_page);
    let range = page * per_page..results.len().min((page + 1) * per_page);

    SearchResult {
        base_path: state.base_uri.path(),
        query: query.as_str(),
        results: &results[range],
        total: results.len(),
        page,
        per_page,
        page_count,
        site: state.database.name(),
    }
    .render_once()
    .map(html_result)
    .map_err(Error::from)
}

/// The rendering mode for a source page.
#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceMode {
    /// Raw text.
    Raw,
    /// Parser tree.
    Tree,
}

/// Query options for `/source`.
#[derive(serde::Deserialize)]
pub(crate) struct SourceQuery {
    /// The view mode.
    mode: Option<SourceMode>,
    /// When in tree view, whether to process the Wikitext in include mode.
    include: Option<String>,
}

/// The source code viewer route handler.
pub(crate) async fn source(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(SourceQuery { mode, include }): Query<SourceQuery>,
) -> Result<impl IntoResponse, Error> {
    let title = Title::new(&name, None);
    let article = state.database.get(&title).map_err(Error::Database)?;

    match mode {
        None | Some(SourceMode::Raw) => {
            raw_source(state.base_uri.path(), &article.body, &article.model, None)
                .map(IntoResponse::into_response)
        }
        Some(SourceMode::Tree) => {
            let source = FileMap::new(&article.body);
            let tree = Parser::new(&CONFIG)
                .parse(&source, include.is_some())
                .map_err(renderer::Error::from)?;
            Ok(format!("{:#?}", inspect(&source, &tree.root)).into_response())
        }
    }
}

/// Calls to the renderer thread using the given command.
fn call_renderer(
    state: &crate::WikiState,
    command: renderer::Command,
) -> Result<RenderOutput, Error> {
    let (tx, rx) = mpsc::channel();
    state.renderer.get()?.send(renderer::In { command, tx })?;
    let output = rx.recv()??;
    Ok(output)
}

/// Renders source code for the given data model into HTML.
// Clippy: This syntax highlighting library sucks and should be replaced by a
// better one anyway, whenever this breaks. There seems to be no non-deprecated
// API for this.
#[allow(deprecated)]
fn raw_source(
    base_path: &str,
    source: &str,
    model: &str,
    form: Option<EvalForm>,
) -> Result<impl IntoResponse + use<>, Error> {
    use syntect::{
        highlighting::ThemeSet,
        html::{ClassStyle, css_for_theme_with_class_style, line_tokens_to_classed_spans},
        parsing::{ParseState, SCOPE_REPO, Scope, ScopeStack, SyntaxDefinition, SyntaxSet},
        util::LinesWithEndings,
    };

    #[derive(TemplateSimple)]
    #[template(path = "source.html")]
    struct RawSource<'a, I>
    where
        I: Iterator<Item = Result<(usize, usize, String), sailfish::RenderError>>,
    {
        /// The base path for URLs.
        base_path: &'a str,
        /// The page CSS.
        css: String,
        /// The original source text.
        form: Option<EvalForm>,
        /// The lines of source code.
        lines: I,
    }

    // syntect kind of sucks and I don’t understand why people use it? The API
    // is very confusing and un-Rusty. This function had to be copied out *and*
    // must use a ‘deprecated’ public API because there is simply no other way
    // to do what I would think should be an obvious use case: emit classy HTML
    // where each new line is emitted into a separate element in the output
    // document, and so any spans of styles which span multiple lines need to be
    // closed at the end of each line and opened again on the next. This is not
    // even conceptually difficult. The documentation implies this is a thing,
    // and then tells users they should be using inline styles instead? What??
    // Did I time warp back to 2002??? How is this OK?????? question mark
    fn scope_to_classes(s: &mut String, scope: Scope, style: ClassStyle) {
        let repo = SCOPE_REPO.lock().unwrap();
        for i in 0..scope.len() {
            let atom = scope.atom_at(i as usize);
            let atom_s = repo.atom_str(atom);
            if i != 0 {
                s.push(' ');
            }
            if let ClassStyle::SpacedPrefixed { prefix } = style {
                s.push_str(prefix);
            }
            s.push_str(atom_s);
        }
    }

    let mut ss = SyntaxSet::load_defaults_newlines().into_builder();
    ss.add(
        SyntaxDefinition::load_from_str(
            include_str!("../res/MediawikiNG.sublime-syntax"),
            true,
            Some("wikitext"),
        )
        .unwrap(),
    );
    let ss = ss.build();

    let syntax = if model == "Scribunto" {
        ss.find_syntax_by_name("Lua")
    } else if model == "html" {
        ss.find_syntax_by_name("HTML")
    } else {
        ss.find_syntax_by_extension("mediawiki")
    }
    .unwrap_or(ss.find_syntax_plain_text());

    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let class_style = ClassStyle::SpacedPrefixed { prefix: "syntect-" };
    let css = css_for_theme_with_class_style(theme, class_style)?;

    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut offset = 0;

    let lines = LinesWithEndings::from(source)
        .enumerate()
        .map(|(index, line)| {
            let mut output = String::new();

            for scope in stack.as_slice() {
                output += r#"<span class=""#;
                scope_to_classes(&mut output, *scope, class_style);
                output += r#"">"#;
            }

            let regions = state.parse_line(line, &ss).unwrap();
            let (html, to_close) =
                line_tokens_to_classed_spans(line, &regions, class_style, &mut stack)
                    .map_err(|err| sailfish::RenderError::Msg(err.to_string()))?;
            output += &html;

            for _ in 0..stack.len() + (to_close.min(0).unsigned_abs()) {
                output += "</span>";
            }

            let start = offset;
            offset += line.len();

            Ok((start, index + 1, output))
        });

    RawSource {
        base_path,
        css,
        form,
        lines,
    }
    .render_once()
    .map(html_result)
    .map_err(Into::into)
}

/// The CSS resource route handler.
#[cfg(not(feature = "debug-styles"))]
pub(crate) async fn styles() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css"),
            (header::CACHE_CONTROL, "max-age=604800, public, immutable"),
        ],
        include_str!("../res/styles.css"),
    )
}

pub(crate) mod filter {
    use sailfish::{
        RenderError,
        runtime::{Buffer, Render},
    };

    /// Escapes inline CSS according to the HTML5 rules.
    pub fn css<T>(expr: &T) -> Css<'_, T>
    where
        T: Render + ?Sized,
    {
        Css(expr)
    }

    /// An escaper for inline CSS.
    pub(crate) struct Css<'a, T>(&'a T)
    where
        T: Render + ?Sized;
    impl<T> Render for Css<'_, T>
    where
        T: Render + ?Sized,
    {
        fn render(&self, b: &mut Buffer) -> Result<(), RenderError> {
            self.0.render(b)
        }

        fn render_escaped(&self, b: &mut Buffer) -> Result<(), RenderError> {
            let mut tmp = Buffer::new();
            self.render(&mut tmp)?;
            let mut iter = tmp.as_str().chars();
            let (mut in_str, mut depth) = (None, 0);
            while let Some(c) = iter.next() {
                match c {
                    '"' | '\'' => {
                        if in_str == Some(c) {
                            in_str = None;
                        } else if in_str.is_none() {
                            in_str = Some(c);
                        }
                        b.push(c);
                    }
                    '{' => {
                        if in_str.is_none() {
                            depth += 1;
                        }
                        b.push(c);
                    }
                    '}' => {
                        if in_str.is_some() {
                            b.push(c);
                        } else if depth != 0 {
                            depth -= 1;
                            b.push(c);
                        } else {
                            log::warn!("Zero depth closing bracket");
                        }
                    }
                    '\\' => {
                        b.push(c);
                        // Avoid confusion tracking brackets and strings. Not
                        // all escape sequences are checked here because we are
                        // actually sanitising both HTML and CSS, and so
                        // `</style>` needs to be escaped for the HTML parser
                        // even if it is written as `\</style>`.
                        if iter
                            .as_str()
                            .starts_with(['{', '}', '\'', '\\', '\"', '/', '*'])
                        {
                            b.push(iter.next().unwrap());
                        }
                    }
                    '/' if iter.as_str().starts_with('*') && in_str.is_none() => {
                        in_str = Some('*');
                        b.push(c);
                    }
                    '*' if iter.as_str().starts_with('/') && in_str == Some('*') => {
                        in_str = None;
                        b.push(c);
                    }
                    '<' if iter.as_str().starts_with("/style>") => {
                        b.push_str("<\\");
                    }
                    _ => b.push(c),
                }
            }
            Ok(())
        }
    }
}
