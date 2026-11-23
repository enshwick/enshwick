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

/// The first [`MAX_FOLLOWED`] distinct entities of `entities` but 0, and
/// how many there are.
fn distinct(entities: impl IntoIterator<Item = u32>) -> ([u32; MAX_FOLLOWED], usize) {
    let mut taken = [0; MAX_FOLLOWED];
    let mut count = 0;
    for entity in entities {
        if count == MAX_FOLLOWED {
            break;
        }
        if entity != 0 && !taken[..count].contains(&entity) {
            taken[count] = entity;
            count += 1;
        }
    }
    (taken, count)
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

    /// Asks the steps to read `entities`, each once and at most
    /// [`MAX_FOLLOWED`] of them, the rest reading as [`Seen::Unread`]. An
    /// entity wanted before keeps its cell and its read; a cell given up
    /// forgets its read.
    pub(crate) fn want(&self, entities: impl IntoIterator<Item = u32>) {
        let (fresh, count) = distinct(entities);
        let fresh = &fresh[..count];
        let mut next = [0; MAX_FOLLOWED];
        for (cell, kept) in self.wanted.iter().zip(&mut next) {
            let entity = cell.load(Ordering::Relaxed);
            if fresh.contains(&entity) {
                *kept = entity;
            }
        }
        for &entity in fresh {
            if !next.contains(&entity)
                && let Some(free) = next.iter_mut().find(|cell| **cell == 0)
            {
                *free = entity;
            }
        }
        for ((cell, read), entity) in self.wanted.iter().zip(&self.read).zip(next) {
            if cell.swap(entity, Ordering::Relaxed) != entity {
                read.write([0; 5]);
            }
        }
    }

    /// Whether `entity` gets a cell beside the followed `entities`: it is
    /// one of them, or they leave a cell free.
    pub(crate) fn fits(entity: u32, entities: impl IntoIterator<Item = u32>) -> bool {
        let (taken, count) = distinct(entities);
        count < MAX_FOLLOWED || taken.contains(&entity)
    }

    /// Whether any entity is wanted: a step with none reads nothing.
    pub(crate) fn any(&self) -> bool {
        self.wanted
            .iter()
            .any(|cell| cell.load(Ordering::Relaxed) != 0)
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

    /// Where `entity` stood at the last step's read of the cell it is
    /// wanted in.
    pub(crate) fn seen(&self, entity: u32) -> Seen {
        if entity == 0 {
            return Seen::Unread;
        }
        self.wanted
            .iter()
            .position(|cell| cell.load(Ordering::Relaxed) == entity)
            .and_then(|cell| self.read[cell].load())
            .filter(|words| words[0] == u64::from(entity))
            .map_or(Seen::Unread, |[_, present, x, y, z]| {
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "the fixed-point coordinates travel as their bits"
                )]
                let at = [x as i64, y as i64, z as i64];
                if present == 1 {
                    Seen::At(at)
                } else {
                    Seen::Gone
                }
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
    fn an_entity_given_up_forgets_its_read_and_one_still_wanted_keeps_it() {
        let followed = Followed::new();
        followed.want([5, 6]);
        followed.step(|entity| (entity == 6).then_some([1, 2, 3]));
        followed.want([6]);
        assert_eq!(followed.seen(5), Seen::Unread, "not Gone from its old read");
        assert_eq!(followed.seen(6), Seen::At([1, 2, 3]));
        followed.want([7, 6]);
        assert_eq!(followed.seen(7), Seen::Unread, "not 5's old read");
        assert_eq!(followed.seen(6), Seen::At([1, 2, 3]));
    }

    #[test]
    fn an_entity_wanted_twice_takes_one_cell() {
        let most = u32::try_from(MAX_FOLLOWED).unwrap();
        let followed = Followed::new();
        followed.want((1..=most).flat_map(|entity| [entity, entity]));
        followed.step(|_| Some([0; 3]));
        assert_eq!(followed.seen(most), Seen::At([0; 3]));
        assert!(!Followed::fits(99, 1..=most));
        assert!(Followed::fits(1, 1..=most));
        assert!(Followed::fits(99, 2..=most));
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
