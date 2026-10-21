//! A background thread solves the clusters whenever the altars' vanilla
//! zones change; the game thread never waits for it. What was solved
//! belongs to one game: the loader drops the sizes when a hosted game stops,
//! so the next game, the same world again included, is solved afresh.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

use enshwick_loader_api::altars::{ZoneControl, ZoneSize};
use enshwick_loader_api::ecs::Entity;

use crate::geometry::{Altar, Box3, clusters};

/// Per altar: its level and its vanilla zone at each level up to it.
type Input = BTreeMap<u32, Vec<Box3>>;

#[derive(Debug)]
struct State {
    solved: Option<Input>,
    solving: bool,
    /// Counts the games run, so a solve of one never lands in the next.
    game: u64,
}

impl State {
    const fn new() -> Self {
        Self {
            solved: None,
            solving: false,
            game: 0,
        }
    }

    /// Whether `input` still needs solving: it differs from what this game
    /// solved last.
    fn wants(&self, input: &Input) -> bool {
        self.solved.as_ref() != Some(input)
    }

    /// Forgets what was solved, for the next game.
    fn forget(&mut self) {
        self.solved = None;
        self.game = self.game.wrapping_add(1);
    }

    /// Notes the end of a solve of `input` started in `game`; whether its
    /// sizes still apply, as they do unless another game began meanwhile.
    fn finish(&mut self, game: u64, input: Input) -> bool {
        self.solving = false;
        if game != self.game {
            return false;
        }
        self.solved = Some(input);
        true
    }
}

static STATE: Mutex<State> = Mutex::new(State::new());

pub(crate) fn request(control: ZoneControl) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    maybe_solve(&mut state, control);
}

/// Forgets the game that stopped: the next one solves its zones again.
pub(crate) fn forget() {
    if let Ok(mut state) = STATE.lock() {
        state.forget();
    }
}

fn input(control: ZoneControl) -> Input {
    control
        .altars()
        .list()
        .into_iter()
        .filter(|altar| {
            if altar.tilted {
                enshwick_loader_api::warn!(
                    "altar {} is tilted; it keeps its vanilla zone",
                    altar.entity.0
                );
            }
            !altar.tilted && !altar.levels.is_empty()
        })
        .map(|altar| {
            let levels = altar
                .levels
                .iter()
                .map(|zone| Box3::new(zone.min, zone.max))
                .collect();
            (altar.entity.0, levels)
        })
        .collect()
}

fn maybe_solve(state: &mut State, control: ZoneControl) {
    if state.solving {
        return;
    }
    let input = input(control);
    if !state.wants(&input) {
        return;
    }
    let levels: Vec<(u32, usize)> = input.iter().map(|(id, l)| (*id, l.len())).collect();
    enshwick_loader_api::info!("{} altars placed, levelled {levels:?}", input.len());
    state.solving = true;
    let game = state.game;
    let spawned = std::thread::Builder::new()
        .name("altar-zones-solver".into())
        .spawn(move || {
            let solved = std::panic::catch_unwind(|| sizes(&input));
            let Ok(mut state) = STATE.lock() else {
                return;
            };
            let Ok(sizes) = solved else {
                state.solving = false;
                enshwick_loader_api::warn!("the solver panicked; vanilla zones stay");
                return;
            };
            if state.finish(game, input)
                && let Err(error) = control.set(&sizes)
            {
                enshwick_loader_api::warn!("{error}");
            }
            // The altars may have moved on while this ran.
            maybe_solve(&mut state, control);
        });
    if spawned.is_err() {
        state.solving = false;
    }
}

#[must_use]
pub(crate) fn sizes(input: &Input) -> Vec<ZoneSize> {
    let started = Instant::now();
    let altars: Vec<Altar> = input
        .iter()
        .map(|(&id, levels)| Altar {
            key: u64::from(id),
            levels: levels.clone(),
        })
        .collect();
    let placements = match clusters(&altars) {
        Ok(placements) => placements,
        Err(error) => {
            enshwick_loader_api::warn!(
                "cluster input rejected: {error}; altars keep vanilla zones"
            );
            return Vec::new();
        }
    };
    let mut sizes = Vec::new();
    for placement in &placements {
        let Ok(id) = u32::try_from(placement.key) else {
            continue;
        };
        let Some(level) = input.get(&id).and_then(|l| u8::try_from(l.len()).ok()) else {
            continue;
        };
        let height = placement.zone.max[1] - placement.zone.min[1];
        enshwick_loader_api::info!(
            "altar {id} level {level}: cluster {}, growth {} cells, height {height}",
            placement.cluster,
            placement.growth
        );
        sizes.push(ZoneSize {
            altar: Entity(id),
            level,
            grow: placement.growth,
            height,
        });
    }
    enshwick_loader_api::info!(
        "solved {} altars in {:?}",
        placements.len(),
        started.elapsed()
    );
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_game_solves_the_same_altars_again() {
        let mut input = Input::new();
        input.insert(1, vec![Box3::new([0, 0, 0], [80, 80, 80])]);
        let mut state = State::new();
        assert!(state.wants(&input));
        state.solving = true;
        assert!(state.finish(state.game, input.clone()));
        assert!(!state.solving);
        assert!(!state.wants(&input), "solved in this game");
        // The host quits and loads the same world again.
        state.forget();
        assert!(state.wants(&input), "the loader dropped the sizes");
        // A solve still running when its game stopped lands nowhere.
        let started = state.game;
        state.solving = true;
        state.forget();
        assert!(!state.finish(started, input.clone()));
        assert!(!state.solving);
        assert!(state.wants(&input));
    }

    #[test]
    fn two_touching_altars_grow_alike_and_a_lone_one_keeps_its_zone() {
        let mut input = Input::new();
        input.insert(1, vec![Box3::new([0, 0, 0], [80, 80, 80])]);
        input.insert(2, vec![Box3::new([80, 0, 0], [160, 80, 80])]);
        input.insert(3, vec![Box3::new([1000, 0, 0], [1080, 80, 80])]);
        let sizes = sizes(&input);
        assert_eq!(sizes.len(), 3);
        assert_eq!(sizes[0].grow, sizes[1].grow);
        assert_eq!(sizes[2].grow, 0);
        assert!(sizes.iter().all(|s| s.level == 1 && s.height == 80));
    }
}
