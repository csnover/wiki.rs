use super::article::Article;
use std::sync::Arc;

/// A limiter for a map which is limited by memory usage.
#[derive(Copy, Clone, Debug)]
pub struct ByMemoryUsage {
    heap_size: usize,
    max_bytes: usize,
}

impl ByMemoryUsage {
    /// Creates a new memory usage limiter with a given limit in bytes.
    pub const fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            heap_size: 0,
        }
    }

    #[inline]
    fn size_of(value: &Arc<Article>) -> usize {
        core::mem::size_of::<Arc<Article>>()
            + core::mem::size_of::<Article>()
            + value.title.capacity()
            + value.body.capacity()
    }
}

impl<K> schnellru::Limiter<K, Arc<Article>> for ByMemoryUsage {
    type KeyToInsert<'a> = K;
    type LinkType = u32;

    #[inline]
    fn is_over_the_limit(&self, _: usize) -> bool {
        self.heap_size > self.max_bytes
    }

    #[inline]
    fn on_insert(
        &mut self,
        _: usize,
        key: Self::KeyToInsert<'_>,
        value: Arc<Article>,
    ) -> Option<(K, Arc<Article>)> {
        let new_size = Self::size_of(&value);
        if new_size <= self.max_bytes {
            self.heap_size += new_size;
            Some((key, value))
        } else {
            None
        }
    }

    #[inline]
    fn on_replace(
        &mut self,
        _: usize,
        _: &mut K,
        _: K,
        old_value: &mut Arc<Article>,
        new_value: &mut Arc<Article>,
    ) -> bool {
        let new_size = Self::size_of(new_value);
        if new_size <= self.max_bytes {
            self.heap_size = self.heap_size - Self::size_of(old_value) + new_size;
            true
        } else {
            false
        }
    }

    #[inline]
    fn on_removed(&mut self, _: &mut K, value: &mut Arc<Article>) {
        self.heap_size -= Self::size_of(value);
    }

    #[inline]
    fn on_cleared(&mut self) {
        self.heap_size = 0;
    }

    #[inline]
    fn on_grow(&mut self, new_memory_usage: usize) -> bool {
        new_memory_usage <= self.max_bytes
    }
}

impl From<usize> for ByMemoryUsage {
    fn from(max_bytes: usize) -> Self {
        Self::new(max_bytes)
    }
}
