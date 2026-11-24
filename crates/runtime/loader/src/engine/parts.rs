//! What each part of the loader hands the joins: its build table ids and the
//! calls the start, the server step and a player's join and leave make into
//! it. Each layer lists its parts in one list; the joins walk the layers in
//! the order of `domains.toml` and each layer's list in its own order.

use std::time::Instant;

use enshwick_core::abi::{Event, PlayerData};

use crate::engine::code::{CodePatch, Verdict};
use crate::engine::diag::Summary;
use crate::engine::facts::{Fact, Facts};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, ModRecord};
use crate::engine::mod_index::ModIndex;
use crate::engine::part_order;

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
    /// A player leaves, before the game removes the character; after it, as
    /// recorded at join, when the game cleared or reused the slot unseen.
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
    pub(crate) end: Option<fn(&'static Loader)>,
    /// A client's hosted game ended: the part forgets its server, after
    /// every part's `end`.
    pub(crate) forget: Option<fn(&'static Loader)>,
    /// Follows the server's world through its loaded, saved and shutdown
    /// events, which [`Loader::emit`] hands it before the mods.
    pub(crate) world_event: Option<WorldCall>,
    /// Whether the part follows the world with no mod subscribed to those
    /// events, so their links are made.
    pub(crate) follows: Option<fn(&Loader) -> bool>,
    /// What the part reads of the server's game events as the engine moves
    /// them.
    pub(crate) game_events: Option<EventFollower>,
    /// A world's settings were applied, at its load, or dropped, at its
    /// stop: the part reads what it keeps of them, before the mods hear.
    pub(crate) settings: Option<fn(&Loader)>,
    /// The part's look at its own files on the settings thread, once a
    /// second, after the mods' settings files.
    pub(crate) poll: Option<fn(&Loader)>,
    /// Writes the part's section of the status summary, under a fuse of its
    /// own, so a panic there leaves the part's game work on.
    pub(crate) status: Option<Fused<fn(&mut Summary)>>,
    /// What two or more mods change of the part's, by name with those mods,
    /// for the conflict lines, under a fuse of its own.
    pub(crate) shared: Option<Fused<fn() -> Shared>>,
    /// What the part keeps of a client's server follows its link to it.
    pub(crate) server_link: Option<fn(ServerLink)>,
    /// Sends the client of the player with this account key, which just
    /// said hello, the part's join data; false when some of it did not fit
    /// the link, so the whole goes again.
    pub(crate) handover: Option<fn(u64) -> bool>,
    /// What the part keeps of each mod, made with the mod's record at the
    /// start and read through [`ModRecord::fact`].
    pub(crate) facts: Option<MakeFact>,
    /// A mod turned off (`true`) or back on (`false`): the part follows
    /// what it keeps of it.
    pub(crate) turned: Option<fn(&Loader, &ModRecord, bool)>,
    /// What the part rests on code patches for, settled at each arming.
    pub(crate) patches: Option<Patches>,
    /// The part as a whole, named after it: its hosting, end and forget run
    /// under it, and once a panic in one of them turns it off, the part's
    /// steps, joins, leaves, hellos and world events stop too, since they
    /// would run on a game's state it no longer keeps.
    pub(crate) fuse: &'static Fuse,
}

impl Part {
    /// A part named `name` that the module `module` is, under its own `fuse`,
    /// with nothing to call.
    pub(crate) const fn new(name: &'static str, module: &'static str, fuse: &'static Fuse) -> Self {
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
            game_events: None,
            settings: None,
            poll: None,
            status: None,
            shared: None,
            server_link: None,
            handover: None,
            facts: None,
            turned: None,
            patches: None,
            fuse,
        }
    }

    /// Whether the part's own fuse is on, so its calls run.
    pub(crate) fn runs(&self) -> bool {
        !self.fuse.is_off()
    }

    /// Runs `call`, one of the part's hosting, end or forget, under the
    /// part's fuse at `site`.
    fn whole(&self, call: Option<fn(&'static Loader)>, site: &str, loader: &'static Loader) {
        if let Some(run) = call {
            let _ran = self.fuse.run(site, || run(loader));
        }
    }

    /// Runs the part's `forget` at a hosted game's end, as [`Part::clearing`]
    /// does.
    fn clear(&self, loader: &'static Loader) {
        if let Some(forget) = self.forget {
            self.clearing("forgetting its hosted game", || forget(loader));
        }
    }

    /// Runs `clear`, which drops state the part keeps, under the part's fuse
    /// at `site`, and still once the fuse is off, a panic then swallowed,
    /// since the part's links stay armed and read what `clear` drops.
    fn clearing(&self, site: &str, clear: impl FnOnce()) {
        if self.runs() {
            let _ran = self.fuse.run(site, clear);
        } else {
            let _cleared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(clear));
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

impl<F: Copy> Fused<F> {
    /// Hands the call to `call` under its fuse, as `site`: `None` while the
    /// fuse is off or when the call panicked, which turns it off.
    #[inline]
    pub(crate) fn call<R>(&self, site: &str, call: impl FnOnce(F) -> R) -> Option<R> {
        self.fuse.run(site, || call(self.run))
    }
}

/// Makes what a part keeps of one mod from its record.
pub(crate) type MakeFact = fn(&ModRecord) -> Box<dyn Fact>;

/// What a part settles of the code patches it rests on, as
/// [`crate::engine::code::arming`] and [`crate::engine::code::disarm`] do.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Patches {
    /// Settles, before arming writes anything, whether the part's features
    /// run: the patches, their verdicts and the targets hooked.
    pub(crate) arming: fn(&[CodePatch], &[Verdict], &[u64]),
    /// Turns the part's patch features off with plain stores, while the
    /// other threads stand still.
    pub(crate) disarm: fn(),
}

/// A call at a player's join or leave.
pub(crate) type PlayerCall = Fused<fn(&'static Loader, &PlayerData)>;

/// A call about one player's client, by its account key.
pub(crate) type KeyCall = Fused<fn(&'static Loader, u64)>;

/// What becomes of a client's link to the server it plays on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerLink {
    /// The client joins a server: what the last one sent goes.
    Joining,
    /// The server answered the client's hello and takes its messages.
    Answered,
    /// The link to the server went: the windows it showed go.
    Lost,
    /// The client forgets its server: it changes servers or its hosted
    /// game ends.
    Forgotten,
}

/// What two or more mods change of a part's, each by its name with
/// those mods.
pub(crate) type Shared = Vec<(String, Vec<ModIndex>)>;

/// A call at the world's loaded, saved and shutdown events.
pub(crate) type WorldCall = Fused<fn(&Loader, Event)>;

/// A part of the loader that reads the server's game events in the event
/// move, beside the mods' handlers: once the move links, it screens the
/// follower's types and copies those it follows, and it lists the
/// follower's switch off when the move cannot run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EventFollower {
    /// The event types it reads on this run, asked once as the move links,
    /// after every seal; `None` when it reads none, so the move need not
    /// link for it.
    pub(crate) types: fn(&Loader) -> Option<Vec<u32>>,
    /// Sees a live event of one of its types in the server's streams before
    /// the engine moves them, and may change it.
    pub(crate) screen: Option<EventScreen>,
    /// Sees the copy of an event of one of its types after the move, before
    /// the mods' handlers.
    pub(crate) follow: Option<EventFollow>,
    /// Its switch, listed off by the entry the build lacks when the move or
    /// its stream layout is off while it reads.
    pub(crate) fuse: Option<&'static Fuse>,
}

/// A follower's screen of the live event of `type_hash` at `payload`,
/// `size` bytes.
///
/// # Safety
///
/// `payload` is a live event of `type_hash` in a stream of the server's
/// world, readable and writable for `size` bytes, on the thread that steps
/// that world before the engine moves its streams.
pub(crate) type EventScreen = unsafe fn(type_hash: u32, payload: *mut u8, size: usize);

/// A follower's follow of the copy `payload` of an event of `type_hash`.
pub(crate) type EventFollow = fn(type_hash: u32, payload: &[u8]);

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
    /// The fuse of the part the step belongs to, once [`Parts`] gathered it:
    /// off, the step no longer runs.
    pub(crate) part: Option<&'static Fuse>,
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
    ($feature:literal, $run:path) => {{
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
    ($stage:ident, $feature:literal, $($run:tt)+) => {
        $crate::engine::parts::Step {
            stage: $crate::engine::parts::Stage::$stage,
            work: $crate::engine::parts::fused!($feature, $($run)+),
            part: None,
        }
    };
}
pub(crate) use step;

/// The part a layer's module `$module` is, named `$name`; without a module,
/// the module that writes it.
macro_rules! part {
    ($name:literal, $($module:ident)::+) => {{
        static FUSE: $crate::engine::fuse::Fuse = $crate::engine::fuse::Fuse::new(concat!($name, " part"));
        $crate::engine::parts::Part::new(
            $name,
            concat!(module_path!() $(, "::", stringify!($module))+),
            &FUSE,
        )
    }};
    ($name:literal) => {{
        static FUSE: $crate::engine::fuse::Fuse = $crate::engine::fuse::Fuse::new(concat!($name, " part"));
        $crate::engine::parts::Part::new($name, module_path!(), &FUSE)
    }};
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
        Self {
            layers,
            rows: part_order::feature_rows(parts().map(|part| (part.name, part.table_ids))),
            stages: part_order::by_stage(Stage::ALL, || {
                parts().flat_map(|part| {
                    part.steps.iter().map(|step| {
                        let step = Step {
                            part: Some(part.fuse),
                            ..*step
                        };
                        (step.stage, step)
                    })
                })
            }),
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
            |step| {
                if step.part.is_none_or(|part| !part.is_off()) {
                    (step.work.run)(stepping);
                }
            },
        );
    }

    /// Every part whose own fuse is on.
    fn running(&self) -> impl DoubleEndedIterator<Item = &'static Part> + use<> {
        self.iter().filter(|part| part.runs())
    }

    /// Hands the world's `event` to every part that follows the world, each
    /// under its own fuse.
    pub(crate) fn world_event(&self, loader: &Loader, event: Event) {
        for fused in self.running().filter_map(|part| part.world_event.as_ref()) {
            let _ran = fused.call("a world event", |run| run(loader, event));
        }
    }

    /// Tells every part that keeps something of a world's settings that they
    /// were applied or dropped, each under the part's fuse and still once it
    /// is off, as [`Part::clearing`] runs it.
    pub(crate) fn settings(&self, loader: &Loader) {
        for part in self.iter() {
            if let Some(settings) = part.settings {
                part.clearing("a world's settings", || settings(loader));
            }
        }
    }

    /// Lets every part look at its files on the settings thread, each under
    /// the part's fuse.
    pub(crate) fn poll(&self, loader: &Loader) {
        for part in self.running() {
            if let Some(poll) = part.poll {
                let _ran = part.fuse.run("its settings poll", || poll(loader));
            }
        }
    }

    /// Lets every part write its section of `summary`, an off part's too,
    /// each under the fuse of its status alone.
    pub(crate) fn status(&self, summary: &mut Summary) {
        for status in self.iter().filter_map(|part| part.status.as_ref()) {
            let _ran = status.call("its status", |write| write(summary));
        }
    }

    /// What two or more mods change of every part's, an off part's too, for
    /// the conflict lines, each read under its own fuse.
    pub(crate) fn shared(&self) -> Shared {
        self.iter()
            .filter_map(|part| part.shared.as_ref())
            .filter_map(|shared| shared.call("its shared resources", |read| read()))
            .flatten()
            .collect()
    }

    /// Tells every part that keeps something of the client's server what
    /// became of its link, each under the part's fuse; a link that drops
    /// what the last server sent reaches an off part too, as
    /// [`Part::clearing`] runs it.
    pub(crate) fn server_link(&self, link: ServerLink) {
        for part in self.iter() {
            let Some(follow) = part.server_link else {
                continue;
            };
            if link == ServerLink::Answered {
                let _ran = part.fuse.run("its server link", || follow(link));
            } else {
                part.clearing("its server link", || follow(link));
            }
        }
    }

    /// Sends the client of player `key` every part's join data, lowest
    /// layer first, each under the part's fuse; false once some did not
    /// fit, leaving the parts after it unsent. A part that panicked or is
    /// off sends none and holds nothing up.
    pub(crate) fn handover(&self, key: u64) -> bool {
        self.running()
            .filter_map(|part| Some((part, part.handover?)))
            .all(|(part, hand)| part.fuse.run("its join data", || hand(key)).unwrap_or(true))
    }

    /// Tells every part that the client of player `key` said hello, each
    /// under its own fuse.
    pub(crate) fn hello(&self, loader: &'static Loader, key: u64) {
        for fused in self.running().filter_map(|part| part.hello.as_ref()) {
            let _ran = fused.call("a client's hello", |run| run(loader, key));
        }
    }

    /// Tells every part that this client starts hosting a game, each under
    /// the part's fuse.
    pub(crate) fn hosting(&self, loader: &'static Loader) {
        for part in self.iter() {
            part.whole(part.hosting, "its hosted game's start", loader);
        }
    }

    /// Ends a client's hosted game in the parts: every `end`, top layer
    /// first, so a window's close handler may still queue a request the
    /// inventory then answers; then every `forget`, which drops what the
    /// handlers queued with the rest. Each runs under the part's fuse, and
    /// `forget` also once that fuse is off.
    pub(crate) fn end(&self, loader: &'static Loader) {
        for part in self.iter().rev() {
            part.whole(part.end, "its hosted game's end", loader);
        }
        for part in self.iter() {
            part.clear(loader);
        }
    }

    /// What every part keeps of the mod `record` is for, made as its record
    /// is.
    pub(crate) fn facts(&self, record: &ModRecord) -> Facts {
        Facts::new(
            self.iter()
                .filter_map(|part| part.facts)
                .map(|make| make(record))
                .collect(),
        )
    }

    /// Tells every part that `record`'s mod turned `off` or back on, lowest
    /// layer first, each under the part's fuse and still once it is off, as
    /// [`Part::clearing`] runs it: an off mod's runtime and links go even
    /// when its part panicked before.
    pub(crate) fn turned(&self, loader: &Loader, record: &ModRecord, off: bool) {
        for part in self.iter() {
            if let Some(turned) = part.turned {
                part.clearing("a mod turned off or on", || turned(loader, record, off));
            }
        }
    }

    /// Lets every part that rests on code patches settle its features
    /// before arming writes anything.
    pub(crate) fn arming(&self, patches: &[CodePatch], verdicts: &[Verdict], hooked: &[u64]) {
        for settle in self.iter().filter_map(|part| part.patches) {
            (settle.arming)(patches, verdicts, hooked);
        }
    }

    /// Turns off every part's patch features with plain stores: it neither
    /// allocates, locks nor logs, so it runs while the other threads stand
    /// still.
    pub(crate) fn disarm(&self) {
        for settle in self.iter().filter_map(|part| part.patches) {
            (settle.disarm)();
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
    fn a_part_whose_end_panics_stops_whole_still_forgets_and_leaves_the_other_parts_running() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static RAN: AtomicU32 = AtomicU32::new(0);
        static BROKEN_STEPS: AtomicU32 = AtomicU32::new(0);
        static BROKEN_HELLOS: AtomicU32 = AtomicU32::new(0);
        static BROKEN_FORGETS: AtomicU32 = AtomicU32::new(0);
        static HOSTS: &[&Part] = &[
            &Part {
                steps: &[step!(Follow, "test steps of the broken part", |_| {
                    BROKEN_STEPS.fetch_add(1, Ordering::Relaxed);
                })],
                hello: Some(fused!("test hello of the broken part", |_, _| {
                    BROKEN_HELLOS.fetch_add(1, Ordering::Relaxed);
                })),
                end: Some(|_| panic!("broken")),
                forget: Some(|_| {
                    BROKEN_FORGETS.fetch_add(1, Ordering::Relaxed);
                    panic!("broken");
                }),
                ..part!("breaks", breaks)
            },
            &Part {
                hosting: Some(|_| {
                    RAN.fetch_add(1, Ordering::Relaxed);
                }),
                forget: Some(|_| {
                    RAN.fetch_add(10, Ordering::Relaxed);
                }),
                ..part!("after", after)
            },
        ];
        static WITH_HOSTS: &[(&str, &[&Part])] = &[("hosts", HOSTS)];
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
        let parts = Parts::new(WITH_HOSTS);
        let stepping = Stepping {
            loader,
            now: Instant::now(),
            step: 1,
        };
        parts.run(Stage::Follow, &stepping);
        parts.hello(loader, 7);
        for _ in 0..2 {
            parts.hosting(loader);
            parts.end(loader);
        }
        parts.run(Stage::Follow, &stepping);
        parts.hello(loader, 7);
        assert_eq!(
            RAN.load(Ordering::Relaxed),
            22,
            "the other part runs every time"
        );
        let broken = HOSTS[0];
        assert!(!broken.runs(), "the part's own fuse is off");
        assert_eq!(
            (
                BROKEN_STEPS.load(Ordering::Relaxed),
                BROKEN_HELLOS.load(Ordering::Relaxed)
            ),
            (1, 1),
            "its step and hello ran before, and not after"
        );
        assert!(
            !broken.steps[0].work.fuse.is_off(),
            "the step itself never panicked"
        );
        assert_eq!(
            BROKEN_FORGETS.load(Ordering::Relaxed),
            2,
            "its links stay armed, so it still forgets each game"
        );
    }

    #[test]
    fn settings_polls_status_and_shared_reach_each_part_until_one_panics_alone() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static SETTINGS: AtomicU32 = AtomicU32::new(0);
        static POLLS: AtomicU32 = AtomicU32::new(0);
        fn shared_file() -> Shared {
            vec![("a.file".to_owned(), vec![ModIndex::new(1)])]
        }
        fn shared_badly() -> Shared {
            panic!("broken")
        }
        static READERS: &[&Part] = &[
            &Part {
                settings: Some(|_| {
                    SETTINGS.fetch_add(1, Ordering::Relaxed);
                }),
                poll: Some(|_| {
                    POLLS.fetch_add(1, Ordering::Relaxed);
                }),
                status: Some(fused!("test status that reads", |summary| {
                    summary.knowledge_guard = Some("read".to_owned());
                })),
                shared: Some(fused!("test shared file", shared_file)),
                ..part!("reads", reads)
            },
            &Part {
                poll: Some(|_| panic!("broken")),
                status: Some(fused!(
                    "test status of the part that polls badly",
                    |summary| {
                        summary.served.texts = 3;
                    }
                )),
                ..part!("polls badly", polls_badly)
            },
            &Part {
                poll: Some(|_| {
                    POLLS.fetch_add(10, Ordering::Relaxed);
                }),
                status: Some(fused!("test status that panics", |_| panic!("broken"))),
                shared: Some(fused!("test shared that panics", shared_badly)),
                ..part!("reports badly", reports_badly)
            },
        ];
        static WITH_READERS: &[(&str, &[&Part])] = &[("readers", READERS)];
        let loader = crate::start::tests::bare_loader();
        let parts = Parts::new(WITH_READERS);
        parts.settings(&loader);
        parts.poll(&loader);
        let mut summary = Summary::default();
        parts.status(&mut summary);
        assert_eq!(
            (summary.knowledge_guard.as_deref(), summary.served.texts),
            (Some("read"), 3),
            "the part whose poll panicked still writes its section"
        );
        assert_eq!(parts.shared(), shared_file());
        parts.poll(&loader);
        assert_eq!(
            (
                SETTINGS.load(Ordering::Relaxed),
                POLLS.load(Ordering::Relaxed)
            ),
            (1, 22)
        );
        assert!(!READERS[1].runs());
        let reports_badly = READERS[2];
        assert!(
            reports_badly.runs(),
            "a panic in its status or shared read leaves its game work on"
        );
        assert!(
            reports_badly
                .status
                .as_ref()
                .is_some_and(|status| status.fuse.is_off())
        );
        assert!(
            reports_badly
                .shared
                .as_ref()
                .is_some_and(|shared| shared.fuse.is_off())
        );
    }

    #[test]
    fn a_part_that_is_off_still_drops_what_a_world_s_settings_or_its_last_server_left() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static DROPPED: AtomicU32 = AtomicU32::new(0);
        static ANSWERED: AtomicU32 = AtomicU32::new(0);
        static OFF: &[&Part] = &[&Part {
            poll: Some(|_| panic!("broken")),
            settings: Some(|_| {
                DROPPED.fetch_add(1, Ordering::Relaxed);
            }),
            server_link: Some(|link| {
                if link == ServerLink::Answered {
                    ANSWERED.fetch_add(1, Ordering::Relaxed);
                } else {
                    DROPPED.fetch_add(10, Ordering::Relaxed);
                    panic!("broken again");
                }
            }),
            ..part!("off", off)
        }];
        static WITH_OFF: &[(&str, &[&Part])] = &[("off", OFF)];
        let loader = crate::start::tests::bare_loader();
        let parts = Parts::new(WITH_OFF);
        parts.poll(&loader);
        assert!(!OFF[0].runs());
        parts.settings(&loader);
        for link in [
            ServerLink::Joining,
            ServerLink::Answered,
            ServerLink::Lost,
            ServerLink::Forgotten,
        ] {
            parts.server_link(link);
        }
        assert_eq!(
            DROPPED.load(Ordering::Relaxed),
            31,
            "its armed links read what it keeps, so it still drops it"
        );
        assert_eq!(
            ANSWERED.load(Ordering::Relaxed),
            0,
            "an answer is no drop, so the off part does not hear it"
        );
    }

    #[test]
    fn a_part_hears_each_turn_off_and_on_with_its_direction_even_after_it_panicked() {
        use std::sync::Mutex;

        use enshwick_base::Relock as _;

        static HEARD: Mutex<Vec<bool>> = Mutex::new(Vec::new());
        static TURNS: &[&Part] = &[&Part {
            turned: Some(|_, _, off| {
                let first = {
                    let mut heard = HEARD.relock();
                    heard.push(off);
                    heard.len() == 1
                };
                assert!(!first, "broken once");
            }),
            ..part!("turns", turns)
        }];
        static WITH_TURNS: &[(&str, &[&Part])] = &[("turns", TURNS)];
        let mut loader = crate::start::tests::loader_running(&["you.lanterns"]);
        loader.parts = Box::leak(Box::new(Parts::new(WITH_TURNS)));
        let index = ModIndex::new(0);
        loader.turn_off(index, "turned off by the test");
        assert!(!TURNS[0].runs(), "its first call panicked");
        loader.turn_back_on(index);
        loader.turn_off(index, "turned off again");
        assert_eq!(*HEARD.relock(), [true, false, true]);
    }

    #[test]
    fn a_client_s_join_data_goes_lowest_layer_first_and_stops_at_the_first_that_does_not_fit() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static HANDED: AtomicU64 = AtomicU64::new(0);
        static LINKS: AtomicU64 = AtomicU64::new(0);
        static LOW: &[&Part] = &[&Part {
            handover: Some(|key| HANDED.fetch_add(key, Ordering::Relaxed) == 0),
            server_link: Some(|link| {
                if link == ServerLink::Forgotten {
                    LINKS.fetch_add(1, Ordering::Relaxed);
                }
            }),
            ..part!("low", low)
        }];
        static HIGH: &[&Part] = &[
            &Part {
                handover: Some(|_| panic!("broken")),
                ..part!("breaks", breaks)
            },
            &Part {
                handover: Some(|key| {
                    HANDED.fetch_add(10 * key, Ordering::Relaxed);
                    true
                }),
                ..part!("high", high)
            },
        ];
        static HANDS: &[(&str, &[&Part])] = &[("low", LOW), ("high", HIGH)];
        let parts = Parts::new(HANDS);
        assert!(parts.handover(1), "a panic holds nothing up");
        assert_eq!(HANDED.load(Ordering::Relaxed), 11);
        assert!(!parts.handover(1), "the low part's data no longer fits");
        assert_eq!(
            HANDED.load(Ordering::Relaxed),
            12,
            "the high part sends none"
        );
        parts.server_link(ServerLink::Joining);
        parts.server_link(ServerLink::Forgotten);
        assert_eq!(LINKS.load(Ordering::Relaxed), 1);
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
