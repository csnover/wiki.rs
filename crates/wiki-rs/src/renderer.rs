//! The renderer manager.

use super::{Limits, db::Database};
use libphp_rs::DateTime;
use libwikitext_common::{
    db::{Article, DatabaseProvider},
    url::Url,
};
use libwikitext_data::MESSAGES;
use libwikitext_render::{
    Error, EvalPp, LoadMode, Paths, RenderOutput, Statics, TemplateCache, make_template_cache,
    render_article, render_string,
};
use std::sync::{Arc, mpsc};

/// A renderer channel message command.
pub(crate) enum Command {
    /// Render an article.
    Article {
        /// The article to render.
        article: Arc<Article>,
        /// The load mode to use when rendering the article.
        load_mode: LoadMode,
        /// If true, follow the article’s redirect before rendering.
        redirect: bool,
    },
    /// Render some arbitrary Wikitext.
    Eval {
        /// Arguments for parameters in the Wikitext.
        args: Option<String>,
        /// The Wikitext.
        code: String,
        /// If true, append marker content to the output.
        markers: bool,
        /// Which rendering step to return.
        mode: EvalPp,
        /// The root page name.
        page_name: String,
    },
    /// Extract the redirect target from an article.
    Redirect {
        /// The redirect source article.
        article: Arc<Article>,
    },
}

/// The input format for a renderer channel message.
pub(crate) struct In {
    /// The renderer command.
    pub command: Command,
    /// The return channel.
    pub tx: mpsc::Sender<Out>,
}

/// The output format for a renderer channel message.
pub type Out = Result<RenderOutput, Error>;

/// Manager for renderer connections.
pub(crate) struct Manager {
    /// The base URI to provide to spawned renderers.
    base_uri: Url,
    /// The article database to provide to spawned renderers.
    database: Arc<crate::db::Database<'static>>,
    /// Time and memory limits.
    limits: Limits,
    /// Template cache.
    template_cache: TemplateCache,
}

impl Manager {
    /// Creates a new render manager.
    pub fn new(base_uri: &Url, database: &Arc<Database<'static>>, limits: Limits) -> Self {
        Self {
            base_uri: base_uri.clone(),
            database: Arc::clone(database),
            limits,
            template_cache: make_template_cache(limits.template_cache),
        }
    }
}

impl r2d2::ManageConnection for Manager {
    type Connection = mpsc::Sender<In>;

    type Error = Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let (tx, rx) = mpsc::channel::<In>();
        let base_uri = self.base_uri.clone();
        let limits = self.limits;
        let template_cache = Arc::clone(&self.template_cache);
        let db = Arc::clone(&self.database);
        let base_time = db.creation_date().map_or_else(DateTime::now, |date| {
            DateTime::from_unix_timestamp(date.unix_timestamp())
        })?;
        std::thread::spawn(move || {
            let config = db.config();
            let db = Arc::clone(&db) as Arc<dyn DatabaseProvider>;
            let mut statics = Statics::builder()
                .base_time(base_time)
                .base_uri(base_uri)
                .db(db)
                .limits(limits.renderer)
                .parser(config)
                .template_cache(template_cache)
                .paths(Paths {
                    article: "article",
                    external: Some("external"),
                    media: "media",
                })
                .build();

            for In { command, tx } in rx {
                let output = match command {
                    Command::Article {
                        article,
                        load_mode,
                        redirect,
                    } => render_article(&mut statics, &MESSAGES, &article, load_mode, redirect),
                    Command::Eval {
                        args,
                        code,
                        markers,
                        mode,
                        page_name,
                    } => render_string(
                        &mut statics,
                        &MESSAGES,
                        &page_name,
                        &code,
                        args.as_deref(),
                        mode,
                        markers,
                    ),
                    Command::Redirect { article } => statics
                        .parser
                        .parse_redirect(article.body())
                        .map(|redirect| RenderOutput {
                            categories: <_>::default(),
                            content: redirect.to_owned(),
                            indicators: <_>::default(),
                            outline: <_>::default(),
                            styles: <_>::default(),
                        })
                        .map_err(Error::from),
                };
                let _ = tx.send(output);
                statics.vm.gc_collect();
            }
        });

        Ok(tx)
    }

    fn has_broken(&self, _: &mut Self::Connection) -> bool {
        false
    }

    fn is_valid(&self, _: &mut Self::Connection) -> Result<(), Self::Error> {
        Ok(())
    }
}
