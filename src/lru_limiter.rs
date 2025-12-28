//! A limiter for [`schnellru`] which limits the size of the cache according to
//! its total size in bytes, including heap allocations.

/// A trait for implementing generic heap size calculations for a
/// [`ByMemoryUsage`] limiter.
pub(crate) trait HeapUsageCalculator {
    /// Calculates the amount of *heap* memory used by `value`.
    fn size_of(&self) -> usize;
}

/// A limiter for a map which is limited by memory usage.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ByMemoryUsage {
    /// Current *heap* memory usage.
    heap_size: usize,
    /// Maximum *total* (heap + map) allowed usage.
    max_bytes: usize,
}

impl ByMemoryUsage {
    /// Creates a new memory usage limiter with a given *total* memory limit in
    /// bytes.
    pub const fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            heap_size: 0,
        }
    }

    /// Gets the amount of heap memory used, in bytes.
    #[inline]
    pub fn heap_usage(&self) -> usize {
        self.heap_size
    }

    /// Calculates the amount of *heap* memory used by `value`.
    #[inline]
    fn size_of<K: HeapUsageCalculator, V: HeapUsageCalculator>(key: &K, value: &V) -> usize {
        K::size_of(key) + V::size_of(value)
    }
}

impl<K: HeapUsageCalculator, V: HeapUsageCalculator> schnellru::Limiter<K, V> for ByMemoryUsage {
    type KeyToInsert<'a> = K;
    type LinkType = u32;

    #[inline]
    fn is_over_the_limit(&self, length: usize) -> bool {
        length * (size_of::<K>() + size_of::<V>()) + self.heap_size > self.max_bytes
    }

    #[inline]
    fn on_insert(&mut self, _: usize, key: Self::KeyToInsert<'_>, value: V) -> Option<(K, V)> {
        let new_size = Self::size_of(&key, &value);
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
        old_key: &mut K,
        new_key: K,
        old_value: &mut V,
        new_value: &mut V,
    ) -> bool {
        let new_size = Self::size_of(&new_key, new_value);
        if new_size <= self.max_bytes {
            self.heap_size = self.heap_size - Self::size_of(old_key, old_value) + new_size;
            true
        } else {
            false
        }
    }

    #[inline]
    fn on_removed(&mut self, key: &mut K, value: &mut V) {
        self.heap_size -= Self::size_of(key, value);
    }

    #[inline]
    fn on_cleared(&mut self) {
        self.heap_size = 0;
    }

    #[inline]
    fn on_grow(&mut self, new_memory_usage: usize) -> bool {
        new_memory_usage + self.heap_size <= self.max_bytes
    }
}

impl HeapUsageCalculator for String {
    #[inline]
    fn size_of(&self) -> usize {
        self.capacity()
    }
}

impl HeapUsageCalculator for u64 {
    #[inline]
    fn size_of(&self) -> usize {
        0
    }
}
