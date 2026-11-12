//! What each part of the loader hands the joins: its build table ids and the
//! calls the start, the server step and a player's join and leave make into
//! it. Each layer lists its parts in one list; the joins walk the layers in
//! the order of `domains.toml` and each layer's list in its own order.

use std::time::Instant;

use enshwick_core::abi::{Event, PlayerData};

use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

/// One part of a layer: a module, with what the joins call in it.
#[derive(Debug)]
pub(crate) struct Part {
    /// The part's feature name in the build's feature table and in compat
    /// reports.
    pub(crate) name: &'static str,
    /// The module the part is, as `module_path!` spells it.
    pub(crate) module: &'static str,
    /// The build table ids the part names; a part with none has no row in
    /// the feature table.
    pub(crate) table_ids: &'static [&'static str],
    /// Ends what mods registered during init; runs before any link.
    pub(crate) seal: Option<fn(&'static Loader)>,
    /// Links the part's hooks, once at the start.
    pub(crate) link: Option<fn(&'static Loader)>,
    /// The part's work after each server step.
    pub(crate) steps: &'static [Step],
    /// A player reached the game.
    pub(crate) joined: Option<Fused<fn(&'static Loader, &PlayerData)>>,
    /// A player leaves, before the game removes the character.
    pub(crate) left: Option<Fused<fn(&'static Loader, &PlayerData)>>,
    /// A client starts hosting a game.
    pub(crate) hosting: Option<fn(&'static Loader)>,
    /// A client's hosted game ended: the part forgets its server.
    pub(crate) forget: Option<fn(&'static Loader)>,
    /// Follows the server's world through its loaded, saved and shutdown
    /// events, which [`Loader::emit`] hands it before the mods.
    pub(crate) world_event: Option<fn(&Loader, Event)>,
    /// Whether the part follows the world with no mod subscribed to those
    /// events, so their links are made.
    pub(crate) follows: Option<fn(&Loader) -> bool>,
}

impl Part {
    /// A part named `name` that the module `module` is, with nothing to call.
    pub(crate) const fn new(name: &'static str, module: &'static str) -> Self {
        Self {
            name,
            module,
            table_ids: &[],
            seal: None,
            link: None,
            steps: &[],
            joined: None,
            left: None,
            hosting: None,
            forget: None,
            world_event: None,
            follows: None,
        }
    }
}

/// A call the game's threads make into a part, under a fuse of its own: a
/// panic in it turns off that call alone, and the game keeps running.
#[derive(Debug)]
pub(crate) struct Fused<F> {
    pub(crate) fuse: &'static Fuse,
    pub(crate) run: F,
}

/// When in the server step a part's work runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Following the game: the parts' own state after the game's step,
    /// before the tick events.
    Follow,
    /// Taking what players and admins asked for, after the tick events.
    Input,
    /// Writing out what the step made: chat, saves, messages and world edits.
    Output,
}

/// A part's work after each server step.
#[derive(Debug)]
pub(crate) struct Step {
    pub(crate) stage: Stage,
    pub(crate) work: Fused<fn(&Stepping)>,
}

/// What a step's work is given.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stepping {
    pub(crate) loader: &'static Loader,
    /// The step's one clock read.
    pub(crate) now: Instant,
    /// The step's number, from 1.
    pub(crate) step: u64,
}

/// A call with a fuse of its own, which the log and the Mods screen name
/// `$feature` when it turns off.
macro_rules! fused {
    ($feature:literal, $run:expr) => {{
        static FUSE: $crate::engine::fuse::Fuse = $crate::engine::fuse::Fuse::new($feature);
        $crate::engine::parts::Fused {
            fuse: &FUSE,
            run: $run,
        }
    }};
}
pub(crate) use fused;

/// A part's work at `$stage` of each server step, under a fuse named
/// `$feature`.
macro_rules! step {
    ($stage:ident, $feature:literal, $run:expr) => {
        $crate::engine::parts::Step {
            stage: $crate::engine::parts::Stage::$stage,
            work: $crate::engine::parts::fused!($feature, $run),
        }
    };
}
pub(crate) use step;

/// The part a layer's module `$module` is, named `$name`; without a module,
/// the module that writes it.
macro_rules! part {
    ($name:literal, $($module:ident)::+) => {
        $crate::engine::parts::Part::new(
            $name,
            concat!(module_path!() $(, "::", stringify!($module))+),
        )
    };
    ($name:literal) => {
        $crate::engine::parts::Part::new($name, module_path!())
    };
}
pub(crate) use part;

/// Every part of the loader, layer by layer, and the feature table rows they
/// name.
#[derive(Debug)]
pub(crate) struct Parts {
    layers: &'static [(&'static str, &'static [&'static Part])],
    rows: Box<[(&'static str, &'static [&'static str])]>,
}

impl Parts {
    /// The parts of `layers`, given lowest first.
    pub(crate) fn new(layers: &'static [(&'static str, &'static [&'static Part])]) -> Self {
        let mut rows: Vec<_> = layers
            .iter()
            .flat_map(|(_, parts)| parts.iter())
            .filter(|part| !part.table_ids.is_empty())
            .map(|part| (part.name, part.table_ids))
            .collect();
        rows.sort_unstable_by_key(|&(name, _)| name);
        Self {
            layers,
            rows: rows.into_boxed_slice(),
        }
    }

    /// Every part, lowest layer first, each layer in its own order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &'static Part> + use<> {
        self.layers
            .iter()
            .flat_map(|(_, parts)| parts.iter().copied())
    }

    /// The layers, lowest first, by name.
    #[cfg(test)]
    pub(crate) fn layers(&self) -> &'static [(&'static str, &'static [&'static Part])] {
        self.layers
    }

    /// Each part that names build table ids, with them, by name: the
    /// loader's feature table.
    pub(crate) fn table_ids(&self) -> &[(&'static str, &'static [&'static str])] {
        &self.rows
    }

    /// The work of every part at `stage` of a step, in order.
    pub(crate) fn steps(&self, stage: Stage) -> impl Iterator<Item = &'static Step> + use<> {
        self.iter()
            .flat_map(|part| part.steps)
            .filter(move |step| step.stage == stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first() {}

    static LOW: &[&Part] = &[&Part {
        table_ids: &["b.one"],
        steps: &[
            step!(Output, "test low output", |_| first()),
            step!(Follow, "test low follow", |_| first()),
        ],
        ..part!("zeta", low)
    }];
    static HIGH: &[&Part] = &[
        &Part {
            steps: &[step!(Follow, "test high follow", |_| first())],
            ..part!("high", high)
        },
        &Part {
            table_ids: &["a.one", "a.two"],
            ..part!("alpha", other)
        },
    ];
    static LAYERS: &[(&str, &[&Part])] = &[("low", LOW), ("high", HIGH)];

    #[test]
    fn the_feature_table_lists_the_parts_that_name_ids_by_name() {
        let parts = Parts::new(LAYERS);
        let names: Vec<_> = parts.table_ids().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["alpha", "zeta"]);
        assert_eq!(parts.table_ids()[0].1, ["a.one", "a.two"]);
    }

    #[test]
    fn a_stage_runs_its_steps_lowest_layer_first() {
        let parts = Parts::new(LAYERS);
        let follow: Vec<_> = parts
            .steps(Stage::Follow)
            .map(|s| s.work.fuse.feature())
            .collect();
        assert_eq!(follow, ["test low follow", "test high follow"]);
        assert_eq!(parts.steps(Stage::Input).count(), 0);
    }

    #[test]
    fn a_part_knows_its_module() {
        assert_eq!(LOW[0].module, concat!(module_path!(), "::low"));
        assert_eq!(part!("own").module, module_path!());
    }
}
