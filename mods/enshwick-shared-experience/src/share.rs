//! The arithmetic of sharing: a character's cumulative experience, the
//! server's high-water mark and how much a character catches up in one tick.

/// What a character's `Experience` and `Level` hold in one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Character {
    /// Character level, from 1.
    pub level: u32,
    /// Experience inside the current level.
    pub progress: u32,
    /// Experience the current level needs.
    pub need: u32,
    /// Experience gained and not yet applied.
    pub gained: u32,
}

/// Experience the completed levels needed: `table[l]` is the sum of what
/// levels 1 to `l - 1` needed, so `table[1]` is 0. Built once from the game's
/// own need per level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Levels {
    cumulative: Vec<u64>,
}

impl Levels {
    /// The table for levels 1 to `cap`, where `need(l)` is the experience
    /// level `l` needs to reach `l + 1`.
    #[must_use]
    pub fn new(cap: u32, mut need: impl FnMut(u32) -> u32) -> Self {
        let mut cumulative = Vec::with_capacity(usize::try_from(cap).unwrap_or(0) + 1);
        cumulative.extend([0, 0]);
        let mut sum = 0u64;
        for level in 1..cap {
            sum += u64::from(need(level));
            cumulative.push(sum);
        }
        Self { cumulative }
    }

    /// The level cap: no experience counts past it.
    #[must_use]
    pub fn cap(&self) -> u32 {
        u32::try_from(self.cumulative.len().saturating_sub(1)).unwrap_or(u32::MAX)
    }

    /// The character's cumulative experience, gained experience included.
    #[must_use]
    pub fn total(&self, character: Character) -> u64 {
        let index = usize::try_from(character.level).unwrap_or(usize::MAX);
        let before = self
            .cumulative
            .get(index)
            .or(self.cumulative.last())
            .copied()
            .unwrap_or(0);
        before + u64::from(character.progress) + u64::from(character.gained)
    }

    /// Experience to add to the character's gained experience this tick, to
    /// bring it towards `high_water`: never past the end of its current level,
    /// as the game applies one level per tick, and nothing at the cap.
    #[must_use]
    pub fn catch_up(&self, character: Character, high_water: u64) -> u32 {
        if character.level == 0 || character.level >= self.cap() {
            return 0;
        }
        let behind = high_water.saturating_sub(self.total(character));
        let room = character
            .need
            .saturating_sub(character.progress)
            .saturating_sub(character.gained);
        u32::try_from(behind.min(u64::from(room))).unwrap_or(room)
    }
}

/// The high-water mark after a tick in which the characters online gained at
/// most `most_gained` and the furthest stands at `furthest` cumulative
/// experience: the leader's gain counts once, however many characters earned
/// it together, and a character already further ahead raises the mark to it.
#[must_use]
pub fn high_water(previous: u64, most_gained: u32, furthest: u64) -> u64 {
    previous
        .saturating_add(u64::from(most_gained))
        .max(furthest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vanilla's first levels, then a flat 6000.
    fn levels() -> Levels {
        Levels::new(50, |level| match level {
            1..=5 => [1000, 2023, 3046, 4069, 5092][level as usize - 1],
            _ => 6000,
        })
    }

    fn at(level: u32, progress: u32, gained: u32) -> Character {
        let need = match level {
            1..=5 => [1000, 2023, 3046, 4069, 5092][level as usize - 1],
            _ => 6000,
        };
        Character {
            level,
            progress,
            need,
            gained,
        }
    }

    #[test]
    fn the_total_sums_completed_levels_progress_and_gains() {
        let levels = levels();
        assert_eq!(levels.cap(), 50);
        assert_eq!(levels.total(at(1, 0, 0)), 0);
        assert_eq!(levels.total(at(1, 400, 50)), 450);
        assert_eq!(levels.total(at(3, 10, 0)), 1000 + 2023 + 10);
        assert_eq!(
            levels.total(at(50, 0, 0)),
            1000 + 2023 + 3046 + 4069 + 5092 + 44 * 6000
        );
    }

    #[test]
    fn the_leader_counts_once_and_the_furthest_raises_the_mark() {
        // Three players share one kill worth 120 each.
        assert_eq!(high_water(5000, 120, 4000), 5120);
        // A character from another world stands further ahead.
        assert_eq!(high_water(5000, 0, 9000), 9000);
        assert_eq!(high_water(u64::MAX, 10, 0), u64::MAX);
    }

    #[test]
    fn a_character_behind_catches_up_one_level_at_a_time() {
        let levels = levels();
        let behind = at(1, 200, 0);
        // Far behind: only up to the end of the current level.
        assert_eq!(levels.catch_up(behind, 100_000), 800);
        // Just behind: exactly the gap.
        assert_eq!(levels.catch_up(behind, 500), 300);
        // Its own gains fill the level first.
        assert_eq!(levels.catch_up(at(1, 200, 700), 100_000), 100);
        assert_eq!(levels.catch_up(at(1, 200, 900), 100_000), 0);
        // Level and progress after each tick reach the mark.
        let mut character = at(1, 0, 0);
        let mark = levels.total(at(4, 17, 0));
        for _ in 0..10 {
            let added = levels.catch_up(character, mark);
            character.progress += added;
            if character.progress >= character.need {
                character = at(character.level + 1, character.progress - character.need, 0);
            }
        }
        assert_eq!(character, at(4, 17, 0));
    }

    #[test]
    fn nothing_moves_at_the_cap_ahead_or_before_spawn() {
        let levels = levels();
        assert_eq!(levels.catch_up(at(50, 0, 0), u64::MAX), 0);
        assert_eq!(levels.catch_up(at(60, 0, 0), u64::MAX), 0);
        assert_eq!(levels.catch_up(Character::default(), u64::MAX), 0);
        assert_eq!(levels.catch_up(at(3, 10, 0), 0), 0);
    }
}
