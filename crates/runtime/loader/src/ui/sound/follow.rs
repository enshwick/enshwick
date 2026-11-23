//! The entities sounds follow: the interface frame names them, the client's
//! step reads where each stands in the client's world, and the next frame
//! moves the sounds there. Both sides only load and store atomics.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::engine::copied::Copied;

/// The most entities sounds follow at once, over every mod.
pub(crate) const MAX_FOLLOWED: usize = 64;

/// Where a followed entity stands, as the client's last read found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seen {
    /// No step read it since the frame asked.
    Unread,
    /// It stands at this fixed-point position, 32.32 metres.
    At([i64; 3]),
    /// The client's world has no such entity.
    Gone,
}

/// The entities wanted and what the steps read of them.
pub(crate) struct Followed {
    wanted: [AtomicU32; MAX_FOLLOWED],
    /// Per cell: the entity read, whether it stood (1) or not (0), and its
    /// position, as `[entity, present, x, y, z]`.
    read: [Copied<5>; MAX_FOLLOWED],
}

impl Followed {
    /// Nothing wanted, nothing read.
    pub(crate) const fn new() -> Self {
        Self {
            wanted: [const { AtomicU32::new(0) }; MAX_FOLLOWED],
            read: [const { Copied::new() }; MAX_FOLLOWED],
        }
    }

    /// Asks the steps to read `entities`, at most [`MAX_FOLLOWED`] of them;
    /// the rest read as [`Seen::Unread`].
    pub(crate) fn want(&self, entities: impl IntoIterator<Item = u32>) {
        let mut cells = self.wanted.iter();
        for (cell, entity) in cells.by_ref().zip(entities) {
            cell.store(entity, Ordering::Relaxed);
        }
        cells.for_each(|cell| cell.store(0, Ordering::Relaxed));
    }

    /// Whether any entity is wanted: a step with none reads nothing.
    pub(crate) fn any(&self) -> bool {
        self.wanted.iter().any(|cell| cell.load(Ordering::Relaxed) != 0)
    }

    /// Runs on the client's step: reads where each wanted entity stands
    /// with `read`, `None` for one the world lacks.
    pub(crate) fn step(&self, read: impl Fn(u32) -> Option<[i64; 3]>) {
        for (wanted, out) in self.wanted.iter().zip(&self.read) {
            let entity = wanted.load(Ordering::Relaxed);
            if entity == 0 {
                continue;
            }
            #[expect(
                clippy::cast_sign_loss,
                reason = "the fixed-point coordinates travel as their bits"
            )]
            let words = match read(entity) {
                Some([x, y, z]) => [u64::from(entity), 1, x as u64, y as u64, z as u64],
                None => [u64::from(entity), 0, 0, 0, 0],
            };
            out.write(words);
        }
    }

    /// Where `entity` stood at the last step's read.
    pub(crate) fn seen(&self, entity: u32) -> Seen {
        self.read
            .iter()
            .filter_map(Copied::load)
            .find(|words| words[0] == u64::from(entity) && entity != 0)
            .map_or(Seen::Unread, |[_, present, x, y, z]| {
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "the fixed-point coordinates travel as their bits"
                )]
                let at = [x as i64, y as i64, z as i64];
                if present == 1 { Seen::At(at) } else { Seen::Gone }
            })
    }

    /// Forgets every read, as when the client's world changes.
    pub(crate) fn clear(&self) {
        self.want([]);
        for cell in &self.read {
            cell.write([0; 5]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_reads_only_the_wanted_entities() {
        let followed = Followed::new();
        followed.step(|_| unreachable!("nothing is wanted"));
        followed.want([5, 6]);
        assert!(followed.any());
        followed.step(|entity| (entity == 5).then_some([1, -2, 3]));
        assert_eq!(followed.seen(5), Seen::At([1, -2, 3]));
        assert_eq!(followed.seen(6), Seen::Gone);
        assert_eq!(followed.seen(7), Seen::Unread);
    }

    #[test]
    fn clearing_forgets_every_read_and_want() {
        let followed = Followed::new();
        followed.want([5]);
        followed.step(|_| Some([0; 3]));
        followed.clear();
        assert!(!followed.any());
        assert_eq!(followed.seen(5), Seen::Unread);
    }
}
