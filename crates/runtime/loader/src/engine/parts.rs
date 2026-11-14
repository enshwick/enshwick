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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the table id tests read each part's code by it")
    )]
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
    pub(crate) joined: Option<PlayerCall>,
    /// A player leaves, before the game removes the character.
    pub(crate) left: Option<PlayerCall>,
    /// The client of the player with this account key said hello and the
    /// server welcomed it, on a link that holds nothing yet: it takes the
    /// server's messages from now on.
    pub(crate) hello: Option<KeyCall>,
    /// A client starts hosting a game.
    pub(crate) hosting: Option<fn(&'static Loader)>,
    /// A client's hosted game ended: the part tells mods what goes with it
    /// (closed windows, unanswered requests), top layer first, before any
    /// part forgets, so what their handlers queue is forgotten too.
    pub(crate) end: Option<Fused<fn(&'static Loader)>>,
    /// A client's hosted game ended: the part forgets its server, after
    /// every part's `end`.
    pub(crate) forget: Option<fn(&'static Loader)>,
    /// Follows the server's world through its loaded, saved and shutdown
    /// events, which [`Loader::emit`] hands it before the mods.
    pub(crate) world_event: Option<WorldCall>,
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
            hello: None,
            hosting: None,
            end: None,
            forget: None,
            world_event: None,
            follows: None,
        }
    }

    /// Runs the `forget` of the part named `name` in `parts`, as a hosted
    /// game's end does.
    #[cfg(test)]
    pub(crate) fn forget_in(parts: &[&Self], name: &str, loader: &'static Loader) {
        let Some(forget) = parts
            .iter()
            .find(|part| part.name == name)
            .and_then(|part| part.forget)
        else {
            panic!("no part {name} forgets anything");
        };
        forget(loader);
    }
}

/// A call the game's threads make into a part, under a fuse of its own: a
/// panic in it turns off that call alone, and the game keeps running.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fused<F> {
    pub(crate) fuse: &'static Fuse,
    pub(crate) run: F,
}

/// A call at a player's join or leave.
pub(crate) type PlayerCall = Fused<fn(&'static Loader, &PlayerData)>;

/// A call about one player's client, by its account key.
pub(crate) type KeyCall = Fused<fn(&'static Loader, u64)>;

/// A call at the world's loaded, saved and shutdown events.
pub(crate) type WorldCall = Fused<fn(&Loader, Event)>;

/// When in the server step a part's work runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Following the game: the parts' own state after the game's step,
    /// before the tick events.
    Follow,
    /// Taking what players and admins asked for, after the tick events,
    /// and posting the chat lines it answers with.
    Input,
    /// Writing out what the step made: saves, messages and world edits.
    Output,
}

impl Stage {
    /// Every stage, in the order of the step and of their values.
    pub(crate) const ALL: [Self; 3] = [Self::Follow, Self::Input, Self::Output];
}

/// A part's work after each server step.
#[derive(Debug, Clone, Copy)]
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
    ($feature:literal, |$($arg:pat_param),*| $run:block) => {{
        static FUSE: $crate::engine::fuse::Fuse = $crate::engine::fuse::Fuse::new($feature);
        $crate::engine::parts::Fused {
            fuse: &FUSE,
            run: |$($arg),*| $run,
        }
    }};
    ($feature:literal, |$($arg:pat_param),*| $run:expr) => {{
        static FUSE: $crate::engine::fuse::Fuse = $crate::engine::fuse::Fuse::new($feature);
        $crate::engine::parts::Fused {
            fuse: &FUSE,
            run: |$($arg),*| {
                $run;
            },
        }
    }};
}
pub(crate) use fused;

/// A part's work at `$stage` of each server step, under a fuse named
/// `$feature`.
macro_rules! step {
    ($stage:ident, $feature:literal, $($run:tt)+) => {
        $crate::engine::parts::Step {
            stage: $crate::engine::parts::Stage::$stage,
            work: $crate::engine::parts::fused!($feature, $($run)+),
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

/// Every part of the loader, layer by layer, the feature table rows they
/// name, and their step work by stage, gathered once for the step to walk.
#[derive(Debug)]
pub(crate) struct Parts {
    layers: &'static [(&'static str, &'static [&'static Part])],
    rows: Box<[(&'static str, &'static [&'static str])]>,
    /// The steps of each [`Stage`], by its index in [`Stage::ALL`], copied
    /// side by side: a step reads its fuse and work without a hop through
    /// its layer's list (61 to 44 ns for 26 parts, `benches/step_parts.rs`).
    stages: [Box<[Step]>; Stage::ALL.len()],
}

impl Parts {
    /// The parts of `layers`, given lowest first.
    pub(crate) fn new(layers: &'static [(&'static str, &'static [&'static Part])]) -> Self {
        let parts = || layers.iter().flat_map(|(_, parts)| parts.iter().copied());
        let mut rows: Vec<_> = parts()
            .filter(|part| !part.table_ids.is_empty())
            .map(|part| (part.name, part.table_ids))
            .collect();
        rows.sort_unstable_by_key(|&(name, _)| name);
        let stages = Stage::ALL.map(|stage| {
            parts()
                .flat_map(|part| part.steps)
                .filter(|step| step.stage == stage)
                .copied()
                .collect()
        });
        Self {
            layers,
            rows: rows.into_boxed_slice(),
            stages,
        }
    }

    /// Every part, lowest layer first, each layer in its own order.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &'static Part> + use<> {
        self.layers
            .iter()
            .flat_map(|(_, parts)| parts.iter().copied())
    }

    /// Runs every part's work at `stage` of a step, in order, each turned
    /// off alone by a panic.
    pub(crate) fn run(&self, stage: Stage, stepping: &Stepping) {
        Fuse::run_each(
            self.steps(stage),
            "the post-step link",
            |step| step.work.fuse,
            |step| (step.work.run)(stepping),
        );
    }

    /// Hands the world's `event` to every part that follows the world, each
    /// under its own fuse.
    pub(crate) fn world_event(&self, loader: &Loader, event: Event) {
        for fused in self.iter().filter_map(|part| part.world_event.as_ref()) {
            let _ran = fused
                .fuse
                .run("a world event", || (fused.run)(loader, event));
        }
    }

    /// Tells every part that the client of player `key` said hello, each
    /// under its own fuse.
    pub(crate) fn hello(&self, loader: &'static Loader, key: u64) {
        for fused in self.iter().filter_map(|part| part.hello.as_ref()) {
            let _ran = fused
                .fuse
                .run("a client's hello", || (fused.run)(loader, key));
        }
    }

    /// Ends a client's hosted game in the parts: every `end`, top layer
    /// first, so a window's close handler may still queue a request the
    /// inventory then answers; then every `forget`, which drops what the
    /// handlers queued with the rest.
    pub(crate) fn end(&self, loader: &'static Loader) {
        for fused in self.iter().rev().filter_map(|part| part.end.as_ref()) {
            let _ran = fused
                .fuse
                .run("a hosted game's end", || (fused.run)(loader));
        }
        for forget in self.iter().filter_map(|part| part.forget) {
            forget(loader);
        }
    }

    /// Each part that names build table ids, with them, by name: the
    /// loader's feature table.
    pub(crate) fn table_ids(&self) -> &[(&'static str, &'static [&'static str])] {
        &self.rows
    }

    /// The work of every part at `stage` of a step, in order.
    pub(crate) fn steps(&self, stage: Stage) -> &[Step] {
        &self.stages[stage as usize]
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
            .iter()
            .map(|s| s.work.fuse.feature())
            .collect();
        assert_eq!(follow, ["test low follow", "test high follow"]);
        assert!(parts.steps(Stage::Input).is_empty());
        assert_eq!(parts.steps(Stage::Output).len(), 1);
        for (index, stage) in Stage::ALL.into_iter().enumerate() {
            assert_eq!(stage as usize, index, "{stage:?}");
        }
    }

    #[test]
    fn a_part_knows_its_module() {
        assert_eq!(LOW[0].module, concat!(module_path!(), "::low"));
        assert_eq!(part!("own").module, module_path!());
    }

    #[test]
    fn a_client_s_hello_reaches_every_part_that_hears_it_with_its_key() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static HEARD: AtomicU64 = AtomicU64::new(0);
        static HEARS: &[&Part] = &[
            &Part {
                hello: Some(fused!("test hello", |_, key| HEARD
                    .fetch_add(key, Ordering::Relaxed))),
                ..part!("hears", hears)
            },
            &part!("deaf", deaf),
        ];
        static WITH_HELLO: &[(&str, &[&Part])] = &[("low", LOW), ("ears", HEARS)];
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        Parts::new(WITH_HELLO).hello(loader, 7);
        assert_eq!(HEARD.load(Ordering::Relaxed), 7);
    }
}
