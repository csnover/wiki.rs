//! Article database with prefetching thread pool.

use super::{Error, RawDatabase as Database, Result};
use crate::title::Title;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Condvar, Mutex},
};

/// Thread pool channel.
struct Channel {
    /// Signal variable.
    cvar: Condvar,
    /// Queue of article titles to prefetch.
    queue: Mutex<Queue>,
}

/// Work-stealing queue.
struct Queue {
    /// Termination signal.
    terminate: bool,
    /// High-priority titles to prefetch content.
    high_priority: HashMap<Title, bool>,
    /// Low-priority titles to prefetch existence.
    low_priority: HashMap<Title, bool>,
}

/// Prefetch priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Priority {
    /// High-priority.
    High,
    /// Low-priority.
    Low,
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
        if let Ok(mut queue) = self.channel.queue.lock() {
            queue.terminate = true;
            self.channel.cvar.notify_all();
        }
    }
}

impl PrefetchableDatabase<'_> {
    /// Creates a new prefetchable database using the given database as the
    /// main database.
    fn from_db(db: Arc<Database<'static>>) -> Self {
        let cvar = Condvar::new();
        let queue = Mutex::new(Queue {
            terminate: false,
            high_priority: HashMap::with_capacity(32),
            low_priority: HashMap::with_capacity(32),
        });
        let channel = Arc::new(Channel { cvar, queue });

        for _ in 0..8 {
            let db = Arc::clone(&db);
            let channel = Arc::clone(&channel);
            std::thread::spawn(move || {
                loop {
                    let Some((priority, title)) = ({
                        let mut queue = channel.queue.lock().unwrap();
                        let queue = &mut *queue;
                        queue
                            .high_priority
                            .iter_mut()
                            .map(|item| (Priority::High, item))
                            .chain(
                                queue
                                    .low_priority
                                    .iter_mut()
                                    .map(|item| (Priority::Low, item)),
                            )
                            .find_map(|(priority, (title, in_flight))| {
                                if *in_flight {
                                    None
                                } else {
                                    *in_flight = true;
                                    Some((priority, title.clone()))
                                }
                            })
                    }) else {
                        let queue = channel.cvar.wait(channel.queue.lock().unwrap()).unwrap();
                        if queue.terminate {
                            break;
                        }
                        continue;
                    };

                    log::trace!("Prefetching {title} ({priority:?})");

                    if priority == Priority::Low {
                        db.contains(&title);
                    } else {
                        let result = db.fetch_article(title.key()).map_or_else(
                            |err| {
                                if matches!(err, Error::NotFound) {
                                    Ok(None)
                                } else {
                                    Err(err)
                                }
                            },
                            |article| Ok(Some(Arc::new(article))),
                        );

                        channel.queue.lock().unwrap().high_priority.remove(&title);

                        if let Ok(result) = result {
                            // If it shows up in the cache between then and now,
                            // the main thread probably needed it first and had
                            // the lock already, so just allow this result to be
                            // discarded by using `get_or_insert`
                            db.cache
                                .write()
                                .unwrap()
                                .get_or_insert(title.key(), || result);
                        }
                    }
                }
            });
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

    /// Prefetches an article with the given title if it is not already loaded.
    pub(crate) fn prefetch(&self, title: Title, priority: Priority) {
        // The key might be empty if this is a prefetch of a link which is only
        // a fragment
        if title.key().is_empty() || self.cache.read().unwrap().peek(title.key()).is_none() {
            return;
        }

        let mut queue = self.channel.queue.lock().unwrap();

        if queue.high_priority.contains_key(&title) {
            return;
        }

        if let Some(in_flight) = queue.low_priority.get(&title) {
            if priority == Priority::Low {
                return;
            } else if !*in_flight {
                queue.low_priority.remove(&title);
            }
        }

        if priority == Priority::Low {
            queue.low_priority.insert(title, false);
        } else {
            queue.high_priority.insert(title, false);
        }

        self.channel.cvar.notify_one();
    }
}

impl<'a> core::ops::Deref for PrefetchableDatabase<'a> {
    type Target = Arc<Database<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}
