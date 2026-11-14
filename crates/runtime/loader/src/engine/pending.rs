//! Work queued from any thread for a later pass to take whole: a step, a
//! frame or a world event. A pass with nothing queued reads one atomic and
//! takes no lock.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_base::Relock;

/// Items queued in order for the next pass, which takes them all at once.
///
/// # Examples
///
/// ```
/// use enshwick_loader::engine::pending::Pending;
///
/// static EVENTS: Pending<u32> = Pending::new();
/// assert!(EVENTS.take().is_empty(), "an idle pass takes no lock");
/// EVENTS.push(7);
/// assert!(EVENTS.push_within(2, [8].into_iter()));
/// assert!(!EVENTS.push_within(2, [9].into_iter()), "past the bound");
/// assert_eq!(EVENTS.take(), [7, 8]);
/// ```
#[derive(Debug, Default)]
pub struct Pending<T> {
    /// Set while `items` may hold anything.
    any: AtomicBool,
    items: Mutex<Vec<T>>,
}

impl<T> Pending<T> {
    /// An empty queue, for a static or a domain's state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            any: AtomicBool::new(false),
            items: Mutex::new(Vec::new()),
        }
    }

    /// Queues `item`.
    pub fn push(&self, item: T) {
        let mut items = self.items.relock();
        items.push(item);
        self.any.store(true, Ordering::Release);
    }

    /// Queues `more`, in order.
    pub fn extend(&self, more: impl IntoIterator<Item = T>) {
        let mut items = self.items.relock();
        items.extend(more);
        if !items.is_empty() {
            self.any.store(true, Ordering::Release);
        }
    }

    /// Queues `more`, all or none: false, queuing nothing, when the queue
    /// would pass `bound` items. The bound is checked by `more`'s length
    /// before it yields, so items made by [`core::iter::once_with`] are not
    /// built past it.
    pub fn push_within(&self, bound: usize, more: impl ExactSizeIterator<Item = T>) -> bool {
        let mut items = self.items.relock();
        if items.len().saturating_add(more.len()) > bound {
            return false;
        }
        items.extend(more);
        if !items.is_empty() {
            self.any.store(true, Ordering::Release);
        }
        true
    }

    /// Whether anything may be queued, read without the lock.
    #[inline]
    pub fn any(&self) -> bool {
        self.any.load(Ordering::Acquire)
    }

    /// Takes everything queued, oldest first, out of the lock, so the caller
    /// may run code that queues more; nothing and no lock when the queue is
    /// empty.
    #[inline]
    pub fn take(&self) -> Vec<T> {
        if !self.any() {
            return Vec::new();
        }
        self.take_queued()
    }

    #[cold]
    fn take_queued(&self) -> Vec<T> {
        let mut items = self.items.relock();
        self.any.store(false, Ordering::Release);
        core::mem::take(&mut *items)
    }

    /// Drops everything queued, as when the world it was for goes away.
    pub fn clear(&self) {
        drop(self.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_take_hands_out_everything_in_order_and_leaves_the_queue_empty() {
        let queue = Pending::new();
        assert!(!queue.any());
        assert!(queue.take().is_empty());
        queue.push(1);
        queue.extend([2, 3]);
        assert!(queue.any());
        assert_eq!(queue.take(), [1, 2, 3]);
        queue.extend([]);
        assert!(!queue.any(), "nothing added leaves nothing to take");
        assert!(!queue.any());
        assert!(queue.take().is_empty());
    }

    #[test]
    fn a_batch_past_the_bound_queues_nothing_and_one_within_it_all() {
        let queue = Pending::new();
        assert!(queue.push_within(3, [1, 2].into_iter()));
        assert!(!queue.push_within(3, [3, 4].into_iter()), "4 items pass 3");
        assert!(
            queue.push_within(3, core::iter::once(3)),
            "the bound itself passes"
        );
        assert_eq!(queue.take(), [1, 2, 3]);
        assert!(queue.push_within(3, core::iter::empty()));
        assert!(!queue.any(), "an empty batch leaves nothing to take");
    }

    #[test]
    fn a_batch_past_the_bound_builds_none_of_its_items() {
        let queue = Pending::new();
        queue.push(1);
        let refused = core::iter::once_with(|| -> u32 { unreachable!("built past the bound") });
        assert!(!queue.push_within(1, refused));
        assert_eq!(queue.take(), [1]);
    }

    #[test]
    fn a_cleared_queue_takes_nothing() {
        let queue = Pending::new();
        queue.push("a world's event");
        queue.clear();
        assert!(!queue.any());
        assert!(queue.take().is_empty());
    }

    #[test]
    fn every_item_pushed_from_many_threads_is_taken_exactly_once() {
        let queue = std::sync::Arc::new(Pending::new());
        let pushers: Vec<_> = (0..4)
            .map(|thread| {
                let queue = std::sync::Arc::clone(&queue);
                std::thread::spawn(move || {
                    for i in 0..250 {
                        queue.push(thread * 1000 + i);
                    }
                })
            })
            .collect();
        let mut taken = Vec::new();
        while pushers.iter().any(|p| !p.is_finished()) {
            taken.extend(queue.take());
        }
        for pusher in pushers {
            assert!(pusher.join().is_ok());
        }
        taken.extend(queue.take());
        taken.sort_unstable();
        let expected: Vec<_> = (0..4)
            .flat_map(|thread| (0..250).map(move |i| thread * 1000 + i))
            .collect();
        assert_eq!(taken, expected);
    }
}
