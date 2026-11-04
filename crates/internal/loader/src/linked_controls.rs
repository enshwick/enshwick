//! The server's look at its players' game actions for the controls that
//! follow them.
//!
//! A tracker per player and control looks during the step, on whatever
//! thread runs the game's system; what fires waits for the step to end,
//! when the world may change.

use enshwick::Entity;
use enshwick::controls::Tracker;
use rustc_hash::{FxBuildHasher, FxHashMap};

/// A control that follows a game action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Follow {
    /// The control's index among the declared.
    pub index: usize,
    /// The game action's bit in the player's digital input, below 64.
    pub bit: u8,
    /// How long the action stays on before the control holds.
    pub hold_ms: u32,
    /// The phases the control fires on.
    pub fires: u32,
}

/// A control that fired for a player during the step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Firing {
    /// The player's character entity.
    pub entity: Entity,
    /// The control's index among the declared.
    pub index: usize,
    /// The phases that fired.
    pub kind: u32,
}

/// Every player's trackers and the firings that wait for the step's end.
#[derive(Debug)]
pub struct Players {
    trackers: FxHashMap<(Entity, usize), Tracker>,
    waiting: Vec<Firing>,
}

impl Default for Players {
    fn default() -> Self {
        Self::new()
    }
}

impl Players {
    /// No player seen yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            trackers: FxHashMap::with_hasher(FxBuildHasher),
            waiting: Vec::new(),
        }
    }

    /// One step's look at `players`, whose digital input `input` reads, at
    /// `now` milliseconds: the trackers of players gone go, and what fires
    /// waits for [`Players::settle`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ControlSpec;
    /// use enshwick_loader::linked_controls::{Follow, Players};
    ///
    /// let follow = Follow { index: 0, bit: 3, hold_ms: 500, fires: ControlSpec::PRESS };
    /// let mut players = Players::new();
    /// players.step(&[follow], &[enshwick::Entity(7)], 0, |_| Some(1 << 3));
    /// assert!(players.waiting());
    /// ```
    pub fn step(
        &mut self,
        follows: &[Follow],
        players: &[Entity],
        now: u64,
        mut input: impl FnMut(Entity) -> Option<u64>,
    ) {
        self.trackers
            .retain(|(entity, _), _| players.contains(entity));
        for &entity in players {
            let Some(digital) = input(entity) else {
                continue;
            };
            for follow in follows {
                let down = digital & 1u64.checked_shl(follow.bit.into()).unwrap_or(0) != 0;
                let tracker = self.trackers.entry((entity, follow.index)).or_default();
                let kind = tracker.look(down, now, follow.hold_ms) & follow.fires;
                if kind != 0 {
                    self.waiting.push(Firing {
                        entity,
                        index: follow.index,
                        kind,
                    });
                }
            }
        }
    }

    /// Whether firings wait.
    #[must_use]
    pub fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Takes the waiting firings with the player `find` knows for each.
    /// A firing whose player it does not know is dropped and its tracker
    /// starts over, so a press fires once the player is known and no
    /// release comes without its press.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ControlSpec;
    /// use enshwick_loader::linked_controls::{Follow, Players};
    ///
    /// let follow = Follow { index: 0, bit: 0, hold_ms: 500, fires: ControlSpec::PRESS };
    /// let mut players = Players::new();
    /// players.step(&[follow], &[enshwick::Entity(7)], 0, |_| Some(1));
    /// let ready = players.settle(|entity| Some(entity));
    /// assert_eq!(ready[0].1, enshwick::Entity(7));
    /// ```
    pub fn settle<P>(&mut self, find: impl Fn(Entity) -> Option<P>) -> Vec<(Firing, P)> {
        let mut ready = Vec::with_capacity(self.waiting.len());
        for firing in self.waiting.drain(..) {
            match find(firing.entity) {
                Some(player) => ready.push((firing, player)),
                None => {
                    self.trackers.remove(&(firing.entity, firing.index));
                }
            }
        }
        ready
    }

    /// Forgets every player and firing, as when the world goes away.
    pub fn forget(&mut self) {
        self.trackers.clear();
        self.waiting.clear();
    }
}

#[cfg(test)]
mod tests {
    use enshwick::abi::ControlSpec;

    use super::*;

    const ALL: u32 = ControlSpec::PRESS | ControlSpec::RELEASE | ControlSpec::HOLD;
    const ANN: Entity = Entity(7);
    const BOB: Entity = Entity(8);

    fn follow(bit: u8) -> Follow {
        Follow {
            index: 0,
            bit,
            hold_ms: 500,
            fires: ALL,
        }
    }

    fn kinds(players: &mut Players) -> Vec<u32> {
        players
            .settle(Some)
            .into_iter()
            .map(|(f, _)| f.kind)
            .collect()
    }

    #[test]
    fn what_fires_during_the_step_waits_for_its_end() {
        let mut players = Players::new();
        players.step(&[follow(2)], &[ANN], 0, |_| Some(0b100));
        assert!(players.waiting(), "nothing fires inside the step");
        assert_eq!(kinds(&mut players), [ControlSpec::PRESS]);
        assert!(!players.waiting());
        players.step(&[follow(2)], &[ANN], 600, |_| Some(0b100));
        players.step(&[follow(2)], &[ANN], 650, |_| Some(0));
        assert_eq!(
            kinds(&mut players),
            [ControlSpec::HOLD, ControlSpec::RELEASE]
        );
    }

    #[test]
    fn a_player_not_found_gets_the_press_later_and_never_a_lone_release() {
        let mut players = Players::new();
        players.step(&[follow(0)], &[ANN], 0, |_| Some(1));
        assert!(players.settle(|_| None::<u32>).is_empty());
        players.step(&[follow(0)], &[ANN], 16, |_| Some(0));
        assert!(!players.waiting(), "no release without its press");
        players.step(&[follow(0)], &[ANN], 32, |_| Some(1));
        assert!(players.settle(|_| None::<u32>).is_empty());
        players.step(&[follow(0)], &[ANN], 48, |_| Some(1));
        assert_eq!(kinds(&mut players), [ControlSpec::PRESS], "pressed again");
    }

    #[test]
    fn a_player_who_left_or_a_forgotten_world_starts_up() {
        let mut players = Players::new();
        players.step(&[follow(0)], &[ANN, BOB], 0, |_| Some(1));
        let _fired = players.settle(Some);
        players.step(&[follow(0)], &[BOB], 16, |_| Some(1));
        players.step(&[follow(0)], &[ANN, BOB], 32, |_| Some(1));
        let pressed: Vec<u32> = players
            .settle(Some)
            .into_iter()
            .map(|(f, _)| f.entity)
            .collect();
        assert_eq!(pressed, [ANN], "Ann left and came back");
        players.forget();
        players.step(&[follow(0)], &[BOB], 48, |_| Some(1));
        assert_eq!(kinds(&mut players), [ControlSpec::PRESS]);
    }

    #[test]
    fn only_the_followed_bit_and_the_control_s_phases_fire() {
        let mut players = Players::new();
        let press_only = Follow {
            fires: ControlSpec::PRESS,
            ..follow(5)
        };
        players.step(&[press_only], &[ANN], 0, |_| Some(1 << 4));
        assert!(!players.waiting());
        players.step(&[press_only], &[ANN], 16, |_| Some(1 << 5));
        players.step(&[press_only], &[ANN], 32, |_| Some(0));
        assert_eq!(kinds(&mut players), [ControlSpec::PRESS]);
        players.step(&[follow(64)], &[ANN], 48, |_| Some(u64::MAX));
        assert!(!players.waiting(), "a bit past the word never fires");
    }
}
