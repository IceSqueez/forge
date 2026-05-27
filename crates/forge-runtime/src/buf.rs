use std::collections::VecDeque;

pub(crate) struct RingBuffer<T> {
    inner: VecDeque<T>,
    cap: usize,
}

impl<T> RingBuffer<T> {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(cap),
            cap,
        }
    }

    pub(crate) fn push(&mut self, item: T) {
        if self.inner.len() == self.cap {
            self.inner.pop_front();
        }
        self.inner.push_back(item);
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        self.inner.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn push_within_capacity_retains_all() {
        let mut rb: RingBuffer<u32> = RingBuffer::new(4);
        for i in 0..4 {
            rb.push(i);
        }
        let items: Vec<u32> = rb.iter().copied().collect();
        assert_eq!(items, [0, 1, 2, 3]);
    }

    #[test]
    fn push_beyond_capacity_evicts_oldest() {
        let mut rb: RingBuffer<u32> = RingBuffer::new(3);
        for i in 0..5 {
            rb.push(i);
        }
        let items: Vec<u32> = rb.iter().copied().collect();
        assert_eq!(items, [2, 3, 4]);
        assert_eq!(rb.len(), 3);
    }

    #[test]
    fn iter_rev_yields_newest_first() {
        let mut rb: RingBuffer<u32> = RingBuffer::new(4);
        for i in 0..4 {
            rb.push(i);
        }
        let newest_first: Vec<u32> = rb.iter().rev().copied().collect();
        assert_eq!(newest_first, [3, 2, 1, 0]);
    }

    #[test]
    fn empty_buffer_len_is_zero() {
        let rb: RingBuffer<u32> = RingBuffer::new(10);
        assert_eq!(rb.len(), 0);
    }
}
