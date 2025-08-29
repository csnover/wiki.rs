//! A limiter for [`schnellru`] which limits the size of the cache according to
//! its total size in bytes.

use core::marker::PhantomData;

/// A trait for implementing generic item size calculations for a
/// [`ByMemoryUsage`] limiter.
pub trait ByMemoryUsageCalculator {
    /// The target type to size.
    type Target;

    /// Calculates the size of `value`.
    fn size_of(value: &Self::Target) -> usize;
}

/// A limiter for a map which is limited by memory usage.
#[derive(Copy, Clone, Debug)]
pub struct ByMemoryUsage<T: ByMemoryUsageCalculator> {
    /// Current memory usage.
    heap_size: usize,
    /// Maximum allowed usage.
    max_bytes: usize,
    /// [`PhantomData`] for the generic item size calculator.
    __: PhantomData<T>,
}

impl<T: ByMemoryUsageCalculator> ByMemoryUsage<T> {
    /// Creates a new memory usage limiter with a given limit in bytes.
    pub const fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            heap_size: 0,
            __: PhantomData,
        }
    }

    /// Calculates the size of a token tree.
    #[inline]
    fn size_of(value: &T::Target) -> usize {
        T::size_of(value)
    }
}

impl<T: ByMemoryUsageCalculator, K> schnellru::Limiter<K, T::Target> for ByMemoryUsage<T> {
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
        value: T::Target,
    ) -> Option<(K, T::Target)> {
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
        old_value: &mut T::Target,
        new_value: &mut T::Target,
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
    fn on_removed(&mut self, _: &mut K, value: &mut T::Target) {
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

impl<T: ByMemoryUsageCalculator> From<usize> for ByMemoryUsage<T> {
    fn from(max_bytes: usize) -> Self {
        Self::new(max_bytes)
    }
}
