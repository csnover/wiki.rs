//! Article database with prefetching thread pool.

use super::{Error, RawDatabase as Database, Result};
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
    queue: Mutex<(bool, HashMap<String, bool>)>,
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
            queue.0 = true;
            self.channel.cvar.notify_all();
        }
    }
}

impl PrefetchableDatabase<'_> {
    /// Creates a new prefetchable database using the given database as the
    /// main database.
    fn from_db(db: Arc<Database<'static>>) -> Self {
        let cvar = Condvar::new();
        let queue = Mutex::new((false, HashMap::with_capacity(32)));
        let channel = Arc::new(Channel { cvar, queue });

        for _ in 0..8 {
            let db = Arc::clone(&db);
            let channel = Arc::clone(&channel);
            std::thread::spawn(move || {
                loop {
                    let Some(title) = ({
                        let mut queue = channel.queue.lock().unwrap();
                        queue.1.iter_mut().find_map(|(title, in_flight)| {
                            if *in_flight {
                                None
                            } else {
                                *in_flight = true;
                                Some(title.clone())
                            }
                        })
                    }) else {
                        let queue = channel.cvar.wait(channel.queue.lock().unwrap()).unwrap();
                        if queue.0 {
                            break;
                        }
                        continue;
                    };

                    let result = db.fetch_article(&title).map_or_else(
                        |err| {
                            if matches!(err, Error::NotFound) {
                                Ok(None)
                            } else {
                                Err(err)
                            }
                        },
                        |article| Ok(Some(Arc::new(article))),
                    );

                    channel.queue.lock().unwrap().1.remove(&title);

                    if let Ok(result) = result {
                        // If it shows up in the cache between then and now,
                        // the main thread probably needed it first and had
                        // the lock already, so just allow this result to be
                        // discarded by using `get_or_insert`
                        db.cache.write().unwrap().get_or_insert(title, || result);
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
    ) -> Result<Self> {
        Ok(Self::from_db(Arc::new(Database::from_file(
            index_path,
            articles_path,
        )?)))
    }

    /// Prefetches an article with the given title if it is not already loaded.
    pub(crate) fn prefetch(&self, title: &str) {
        if self.cache.read().unwrap().peek(title).is_none() {
            let mut queue = self.channel.queue.lock().unwrap();
            if !queue.1.contains_key(title) {
                log::trace!("Prefetching {title}");
                queue.1.insert(title.to_string(), false);
                self.channel.cvar.notify_one();
            }
        }
    }
}

impl<'a> core::ops::Deref for PrefetchableDatabase<'a> {
    type Target = Arc<Database<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}
