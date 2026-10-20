//! A background thread solves the clusters whenever the altars' vanilla
//! zones change; the game thread never waits for it.

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
}

static STATE: Mutex<State> = Mutex::new(State {
    solved: None,
    solving: false,
});

pub(crate) fn request(control: ZoneControl) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    maybe_solve(&mut state, control);
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
    if state.solved.as_ref() == Some(&input) {
        return;
    }
    let levels: Vec<(u32, usize)> = input.iter().map(|(id, l)| (*id, l.len())).collect();
    enshwick_loader_api::info!("{} altars placed, levelled {levels:?}", input.len());
    state.solving = true;
    let spawned = std::thread::Builder::new()
        .name("altar-zones-solver".into())
        .spawn(move || {
            let solved = std::panic::catch_unwind(|| sizes(&input));
            let Ok(mut state) = STATE.lock() else {
                return;
            };
            state.solving = false;
            match solved {
                Ok(sizes) => {
                    if let Err(off) = control.set(&sizes) {
                        enshwick_loader_api::warn!("{off}");
                    }
                    state.solved = Some(input);
                    // The altars may have moved on while this ran.
                    maybe_solve(&mut state, control);
                }
                Err(_) => enshwick_loader_api::warn!("the solver panicked; vanilla zones stay"),
            }
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
