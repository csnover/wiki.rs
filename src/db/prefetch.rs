//! Article database with prefetching thread pool.
//!
//! There are two operations when rendering an article that involve interaction
//! with the database:
//!
//! 1. Fetching templates to expand.
//! 2. Determining the validity of internal wiki links.
//!
//! The prefetcher races the main rendering thread by receiving a list of all
//! template and link targets, then scanning the index and decompressing any
//! article data on separate threads in parallel.
//!
//! Because the MediaWiki dump index is in an undefined order, trying to find an
//! article in the index requires a full table scan. The prefetcher tries to
//! reduce the number of scans by batching these requests.

use super::{Article, RawDatabase as Database, Result, index::IndexEntry, is_lobotomised};
use crate::title::Title;
use indexmap::{IndexMap, IndexSet};
use parking_lot::{Condvar, Mutex, MutexGuard};
use std::{collections::HashSet, path::Path, sync::Arc, time::Instant};

/// Thread pool channel.
struct Channel {
    /// Signal variable.
    cvar: Condvar,
    /// Queue of article titles to prefetch.
    queue: Mutex<Queue>,
}

/// The job queue.
#[derive(Default)]
struct Queue {
    /// The number of in-flight jobs.
    in_flight: usize,
    /// Queued jobs.
    jobs: IndexMap<Title, PrefetchState>,
    /// The number of pending high-priority content prefetches in the job queue.
    pending_content: usize,
    /// The number of pending low-priority existence checks in the job queue.
    pending_exist: usize,
    /// The number of pending high-priority existence-then-content prefetches in
    /// the job queue.
    pending_exist_content: usize,
    /// Thread termination signal.
    terminate: bool,
}

/// The prefetch state of a title.
#[derive(Debug)]
enum PrefetchState {
    /// Title content preload is in progress.
    InFlightContent,
    /// Title existence check is in progress.
    InFlightExist,
    /// Title existence check is in progress and then will transition to a
    /// content request.
    InFlightExistContent,
    /// Title is pending a request for content preload.
    PendingContent(IndexEntry),
    /// Title is pending a request for an existence check.
    PendingExist,
    /// Title is pending a request for an existence check and then content
    /// preload.
    PendingExistContent,
}

/// A prefetch job.
enum Job {
    /// Fetch the content for the given title.
    Content(Title, IndexEntry),
    /// Fetch the exists state for all of the titles in the first hash set,
    /// and content for the titles in the second hash set.
    Exist(HashSet<Title>, HashSet<Title>),
}

/// An article database with non-blocking prefetching.
pub(crate) struct PrefetchableDatabase<'a> {
    /// Thread pool channel.
    channel: Arc<Channel>,
    /// Article database.
    db: Arc<Database<'a>>,
}

impl Drop for PrefetchableDatabase<'_> {
    fn drop(&mut self) {
        self.channel.queue.lock().terminate = true;
        self.channel.cvar.notify_all();
    }
}

impl PrefetchableDatabase<'_> {
    /// Creates a new prefetchable database using the given database as the
    /// main database.
    fn from_db(db: Arc<Database<'static>>) -> Self {
        let cvar = Condvar::new();
        let queue = Mutex::new(Queue {
            // The initial capacity is mostly arbitrary, based on the number of
            // expected peak jobs on a country-sized page
            jobs: IndexMap::with_capacity(1024),
            ..Default::default()
        });
        let channel = Arc::new(Channel { cvar, queue });

        // TODO: Do something less bizarre
        let threads = std::thread::available_parallelism()
            .map_or(2, usize::from)
            .max(2)
            - 1;

        log::info!("Starting prefetch pool with {threads} threads");

        for _ in 0..threads {
            let db = Arc::clone(&db);
            let channel = Arc::clone(&channel);
            std::thread::Builder::new()
                .name("wiki-rs-prefetch".into())
                .spawn(move || {
                    loop {
                        let mut queue = channel.queue.lock();
                        channel.cvar.wait(&mut queue);

                        if queue.terminate {
                            break;
                        }

                        let Some(job) = find_work(&mut queue) else {
                            continue;
                        };

                        MutexGuard::unlocked(&mut queue, || {
                            do_work(&db, &channel, job);
                        });

                        if queue.in_flight == 0
                            && queue.pending_content == 0
                            && queue.pending_exist == 0
                            && queue.pending_exist_content == 0
                        {
                            log::trace!("Finished prefetching; peak jobs was {}", queue.jobs.len());
                            queue.jobs.clear();
                        }
                    }
                })
                .unwrap();
        }

        Self { channel, db }
    }

    /// Creates a new prefetchable database from the given uncompressed text
    /// index and compressed multistream.xml.bz2 file.
    pub fn from_file(
        index_path: impl AsRef<Path>,
        articles_path: impl AsRef<Path>,
        cache_size_limit: usize,
    ) -> Result<Self> {
        Ok(Self::from_db(Arc::new(Database::from_file(
            index_path,
            articles_path,
            cache_size_limit,
        )?)))
    }

    /// Returns true if the database contains an article with the given title.
    #[inline]
    pub fn contains(&self, title: &Title) -> bool {
        let key = title.key();
        self.db.contains_cached(title, key).unwrap_or_else(|| {
            self.cancel_prefetch(title, key, false);
            self.index.find_article(key).is_some()
        })
    }

    /// Gets an article with the given title from the database. The article will
    /// be cached in memory.
    #[inline]
    pub fn get(&self, title: &Title) -> Result<Arc<Article>> {
        let key = title.key();
        self.cancel_prefetch(title, key, true);
        self.db.get(title)
    }

    /// Prefetches a collection of titles.
    ///
    /// Because the MW database dump index is totally unordered, finding a title
    /// in the index requires a full table scan. Batching titles into request
    /// sets reduces the number of scans required, increasing performance.
    ///
    /// Both templates and links need to check for existence in the index, but
    /// templates are both more time-critical and also require decompressing
    /// article data, so they are collected separately.
    pub fn prefetch_all(&self, templates: IndexSet<Title>, links: IndexSet<Title>) {
        if templates.is_empty() && links.is_empty() {
            return;
        }

        let mut queue = self.channel.queue.lock();
        // borrowck cannot see disjoint field borrows through MutexGuard
        let queue = &mut *queue;

        // Each time the scanner sends new lists, they need to be inserted in
        // the input order at the top of the queue. To avoid quadratic
        // behaviour, the set is filtered into a temporary collection so it can
        // be inserted all at once.
        let mut wake_content = false;
        let mut insertions = templates
            .into_iter()
            .filter_map(|title| {
                let key = title.key();
                if !self.can_prefetch(&title, key) {
                    return None;
                }

                if let Some(state) = queue.jobs.get_mut(&title) {
                    if matches!(
                        state,
                        PrefetchState::InFlightExist | PrefetchState::PendingExist
                    ) {
                        // This should practically never happen since it would
                        // be unusual for a page to link to a template and then
                        // expand the same template later. As such, this branch
                        // is basically untested, and might be broken.
                        log::warn!("A very unlikely thing happened; this branch might be broken");
                        if matches!(state, PrefetchState::PendingExist) {
                            queue.pending_exist -= 1;
                            queue.pending_exist_content += 1;
                            *state = PrefetchState::PendingExistContent;
                        } else {
                            *state = PrefetchState::InFlightExistContent;
                        }
                    }
                    None
                } else {
                    let state = if let Some(entry) = self.index.is_cached(key) {
                        let Some(entry) = entry else {
                            // Well, turns out that this does not exist, so it
                            // does not need to be prefetched any more than it
                            // already was
                            return None;
                        };
                        wake_content = true;
                        queue.pending_content += 1;
                        PrefetchState::PendingContent(entry)
                    } else {
                        queue.pending_exist_content += 1;
                        PrefetchState::PendingExistContent
                    };
                    Some((title, state))
                }
            })
            .collect::<IndexMap<_, _>>();

        for title in links {
            let key = title.key();
            if !self.can_prefetch(&title, key)
                || self.index.is_cached(key).is_some()
                || queue.jobs.contains_key(&title)
                || insertions.contains_key(&title)
            {
                continue;
            }

            queue.pending_exist += 1;
            insertions.insert(title, PrefetchState::PendingExist);
        }

        // “The input iterator `replace_with` is only consumed when the `Splice`
        // value is dropped.”
        drop(queue.jobs.splice(0..0, insertions));

        if !queue.jobs.is_empty() {
            if wake_content {
                self.channel.cvar.notify_all();
            } else {
                self.channel.cvar.notify_one();
            }
        }
    }

    /// Cancels a prefetch for the given title, if one exists.
    ///
    /// It would also be possible to return a condvar to have the renderer
    /// thread wait for the prefetcher, but in testing this was much slower than
    /// just doing the work twice.
    fn cancel_prefetch(&self, title: &Title, key: &str, cancel_content: bool) {
        if !self.can_prefetch(title, key) {
            return;
        }

        let mut queue = self.channel.queue.lock();
        // borrowck cannot see disjoint field borrows through MutexGuard
        let queue = &mut *queue;

        if let Some(state) = queue.jobs.get_mut(title) {
            match state {
                PrefetchState::PendingContent(_)
                | PrefetchState::PendingExist
                | PrefetchState::PendingExistContent => {
                    // Womp womp, loser
                    log::trace!("Prefetching lost the race for {title}");
                    match state {
                        PrefetchState::PendingContent(_) => queue.pending_content -= 1,
                        PrefetchState::PendingExist => queue.pending_exist -= 1,
                        PrefetchState::PendingExistContent => queue.pending_exist_content -= 1,
                        PrefetchState::InFlightContent
                        | PrefetchState::InFlightExist
                        | PrefetchState::InFlightExistContent => unreachable!(),
                    }
                    // Just park the job in an ignorable state to avoid
                    // wasting time reordering the jobs list or accidentally
                    // re-requesting something that the renderer thread will
                    // fetch
                    *state = PrefetchState::InFlightContent;
                }
                PrefetchState::InFlightContent => {
                    log::trace!("Prefetching lost the race for content {title}");
                }
                PrefetchState::InFlightExist | PrefetchState::InFlightExistContent => {
                    log::trace!("Prefetching lost the race for existence {title}");
                    // The renderer thread will be getting the content itself
                    // so tell the prefetcher not to try, if it was going to try
                    if cancel_content {
                        *state = PrefetchState::InFlightExist;
                    }
                }
            }
        }
    }

    /// Returns true if the given title is in a content-prefetchable state.
    #[inline]
    fn can_prefetch(&self, title: &Title, key: &str) -> bool {
        !is_lobotomised(key) && self.may_exist(title) && self.cache.read().peek(key).is_none()
    }
}

/// Wake up, brush your teeth, go to work.
#[inline]
fn do_work(db: &Database<'_>, channel: &Channel, job: Job) {
    match job {
        Job::Exist(titles, then_fetch) => {
            let start = Instant::now();
            let count = titles.len();
            db.index.prefetch(titles, |title, entry| {
                let mut queue = channel.queue.lock();
                if let Some(entry) = entry
                    && then_fetch.contains(&title)
                {
                    if let Some(state) = queue.jobs.get_mut(&title)
                        && matches!(*state, PrefetchState::InFlightExistContent)
                    {
                        *state = PrefetchState::PendingContent(entry);
                        queue.pending_content += 1;
                        channel.cvar.notify_one();
                    } else {
                        // It is possible that during the race the prefetcher
                        // managed to start checking for existence but then the
                        // renderer caught up and changed the job kind because
                        // it needs the content *now* and will fetch it itself,
                        // so only ones that are still in the original in-flight
                        // state should be passed to fetch content
                        log::trace!("Discarding work on {title}");
                    }
                }
                queue.in_flight -= 1;
            });
            log::trace!("Scanned {count} in {:?}", start.elapsed());
        }
        Job::Content(title, entry) => {
            log::trace!("Prefetching content for {title}");
            // If an error occurs, the article will not make it to the article
            // cache, and so when the renderer calls `get` later, the same error
            // will occur and then it can be handled
            let key = title.key();
            if let Ok(article) = db.extract_article(key, entry) {
                db.cache
                    .write()
                    .insert(key.to_string(), Some(Arc::new(article)));
            }
            channel.queue.lock().in_flight -= 1;
        }
    }
}

/// Finds the next highest priority job from the prefetch queue.
#[inline]
fn find_work(queue: &mut Queue) -> Option<Job> {
    if queue.pending_exist_content != 0 {
        // TODO: Maybe this should actually just allow the two index scans,
        // since it is unclear whether early-returns dedicating more threads to
        // decompression would be faster than this approach of preferring fewer
        // index scans
        let expected = queue.pending_exist + queue.pending_exist_content;
        queue.in_flight += expected;
        queue.pending_exist_content = 0;
        queue.pending_exist = 0;
        let (mut exist, mut fetch) = (HashSet::new(), HashSet::new());
        queue.jobs.iter_mut().for_each(|(title, state)| {
            if matches!(
                *state,
                PrefetchState::PendingExist | PrefetchState::PendingExistContent
            ) {
                if matches!(*state, PrefetchState::PendingExistContent) {
                    *state = PrefetchState::InFlightExistContent;
                    fetch.insert(title.clone());
                } else {
                    *state = PrefetchState::InFlightExist;
                }
                exist.insert(title.clone());
            }
        });
        debug_assert_eq!(
            exist.len(),
            expected,
            "expected {expected} titles for early scan, got {}",
            exist.len()
        );
        Some(Job::Exist(exist, fetch))
    } else if queue.pending_content != 0 {
        queue.jobs.iter_mut().find_map(|(title, state)| {
            if let PrefetchState::PendingContent(entry) = *state {
                queue.in_flight += 1;
                queue.pending_content -= 1;
                *state = PrefetchState::InFlightContent;
                Some(Job::Content(title.clone(), entry))
            } else {
                None
            }
        })
    } else if queue.pending_exist != 0 {
        let expected = queue.pending_exist;
        queue.in_flight += expected;
        queue.pending_exist = 0;
        let titles = queue
            .jobs
            .iter_mut()
            .filter_map(|(title, state)| {
                matches!(*state, PrefetchState::PendingExist).then(|| {
                    *state = PrefetchState::InFlightExist;
                    title.clone()
                })
            })
            .collect::<HashSet<_>>();
        debug_assert_eq!(
            titles.len(),
            expected,
            "expected {expected} titles for late scan, got {}",
            titles.len()
        );
        Some(Job::Exist(titles, <_>::default()))
    } else {
        None
    }
}

impl<'a> core::ops::Deref for PrefetchableDatabase<'a> {
    type Target = Arc<Database<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}
