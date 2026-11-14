//! A map over the cores, for tests that read every entry of a game
//! executable or archive, which take minutes on one core of a debug build.

use std::sync::atomic::{AtomicUsize, Ordering};

/// `f` of each item, in the items' order, computed on every core: each
/// thread takes the next item as it finishes one, since items differ widely
/// in cost.
pub(crate) fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(items.len().max(1));
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(at) else {
                            return done;
                        };
                        done.push((at, f(item)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect()
    });
    done.sort_unstable_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, result)| result).collect()
}
