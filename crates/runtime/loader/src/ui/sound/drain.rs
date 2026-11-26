//! One frame of the mods' sounds on the client's game thread: their queued
//! starts and changes, the entities sounds follow, the sounds that ended,
//! and the sounds of mods turned off or of a world that ended.
//!
//! The game's sound system is behind [`Mixer`], so the rules run without
//! the game.

use enshwick_core::abi::{SoundInfo, SoundPlay, SoundSteer};

use super::follow::{Followed, Seen};
use super::kept::{ModSounds, Op, Slot};

/// The game's scenes a mod's sound plays in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scene {
    /// The interface's, for [`SoundPlay::SCREEN`].
    Ui,
    /// The world's, for [`SoundPlay::AT`] and [`SoundPlay::ON`].
    Game,
    /// The music's, for [`SoundPlay::MUSIC`].
    Music,
}

impl Scene {
    /// The scene of `place`.
    fn of(place: u32) -> Self {
        match place {
            SoundPlay::AT | SoundPlay::ON => Self::Game,
            SoundPlay::MUSIC => Self::Music,
            _ => Self::Ui,
        }
    }
}

/// The game's sound system as the drain uses it; ids are the game's
/// instance ids.
pub(crate) trait Mixer {
    /// Whether this build reaches `scene`.
    fn reaches(&self, scene: Scene) -> bool;
    /// Starts `container` in `scene` at `volume`, placed at `at` or from no
    /// place; its id, `None` when the game did not start it.
    fn start(
        &mut self,
        scene: Scene,
        container: [u8; 16],
        at: Option<[i64; 3]>,
        volume: f32,
    ) -> Option<u32>;
    /// Stops `id` over `fade_ms`.
    fn stop(&mut self, id: u32, fade_ms: u32);
    /// Changes `id`'s volume over `fade_ms`.
    fn volume(&mut self, id: u32, volume: f32, fade_ms: u32);
    /// Moves `id` to `at`.
    fn position(&mut self, id: u32, at: [i64; 3]);
    /// Whether `id` still plays.
    fn playing(&mut self, id: u32) -> bool;
}

/// How long a sound fades out once its entity is gone or its world ended.
pub(crate) const END_FADE_MS: u32 = 200;
/// How long a mod's earlier music fades out under its new one that has no
/// fade of its own.
pub(crate) const MUSIC_HANDOFF_MS: u32 = 1000;
/// How many frames a start on an entity waits for a step to read where the
/// entity stands.
pub(crate) const FOLLOW_WAIT_FRAMES: u32 = 120;

/// `at`, metres, as the game's fixed point: 32.32 metres, rounded as the
/// game rounds (client 26430625: times 2^32, plus 0.5, truncated).
pub(crate) fn fixed(at: [f64; 3]) -> [i64; 3] {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the game's own rounding; `as` saturates the few metres past its range"
    )]
    at.map(|axis| (axis * 4_294_967_296.0 + 0.5) as i64)
}

/// A sound the game plays for a mod.
#[derive(Clone, Copy, Debug)]
struct Live {
    owner: usize,
    slot: Slot,
    game: u32,
    place: u32,
    /// The entity it follows, 0 for none.
    follows: u32,
    at: [i64; 3],
}

/// A start on an entity waiting for a step to read where it stands.
#[derive(Clone, Copy, Debug)]
struct Waiting {
    owner: usize,
    slot: Slot,
    play: SoundPlay,
    frames: u32,
}

/// What a frame hands the drain besides the mixer.
pub(crate) struct Frame<'a> {
    /// Each mod's sounds by mod index, `None` for a mod that never played.
    pub(crate) mods: &'a [Option<&'a ModSounds>],
    /// Whether the mod at an index is off.
    pub(crate) off: &'a dyn Fn(usize) -> bool,
    /// The client's world session, which changes when its world does.
    pub(crate) session: u64,
}

/// What the drain keeps between frames, on the client's game thread.
#[derive(Debug, Default)]
pub(crate) struct Drain {
    live: Vec<Live>,
    waiting: Vec<Waiting>,
    /// The waiting list's other buffer, so a frame keeps both capacities.
    spare: Vec<Waiting>,
    session: u64,
    closed: bool,
}

/// Settles each start in `sounds`' queue as `state` for `reason`, and drops
/// its changes.
fn settle_queued(sounds: &ModSounds, state: u32, reason: u32) {
    for op in sounds.batch() {
        if let Op::Start(slot, _) = op {
            sounds.settle(slot, state, reason);
        }
    }
}

impl Drain {
    /// Runs one frame: `mixer` is `None` where the loader's playback is off,
    /// which fails every start.
    pub(crate) fn frame<M: Mixer>(
        &mut self,
        mut mixer: Option<&mut M>,
        frame: &Frame<'_>,
        followed: &Followed,
    ) {
        if frame.session != self.session {
            // The first session counted is the client's first world, no change.
            let changed = self.session != 0;
            self.session = frame.session;
            if changed {
                followed.clear();
                self.end_where(frame, mixer.as_deref_mut(), |place| {
                    matches!(place, SoundPlay::AT | SoundPlay::ON)
                });
            }
        }
        for (owner, sounds) in frame.mods.iter().enumerate() {
            let Some(sounds) = sounds else {
                continue;
            };
            if (frame.off)(owner) {
                self.end_owner(owner, sounds, mixer.as_deref_mut());
                settle_queued(sounds, SoundInfo::ENDED, 0);
                continue;
            }
            for op in sounds.batch() {
                self.run(owner, sounds, op, mixer.as_deref_mut(), followed);
            }
        }
        self.wait(frame, mixer.as_deref_mut(), followed);
        self.follow(frame, mixer.as_deref_mut(), followed);
        self.reap(frame, mixer);
        followed.want(self.followed());
    }

    /// Ends every sound once a fault turned the mods' sounds off: stops
    /// what the game plays and ends its handle, and fails each start still
    /// queued or waiting; later calls do nothing.
    pub(crate) fn close<M: Mixer>(&mut self, mut mixer: Option<&mut M>, frame: &Frame<'_>) {
        if self.closed {
            return;
        }
        self.closed = true;
        for waits in self.waiting.drain(..) {
            if let Some(Some(sounds)) = frame.mods.get(waits.owner) {
                sounds.settle(waits.slot, SoundInfo::FAILED, SoundInfo::OFF);
            }
        }
        for (owner, sounds) in frame.mods.iter().enumerate() {
            if let Some(sounds) = sounds {
                self.end_owner(owner, sounds, mixer.as_deref_mut());
                settle_queued(sounds, SoundInfo::FAILED, SoundInfo::OFF);
            }
        }
    }

    /// Whether [`Drain::close`] ran.
    pub(crate) fn closed(&self) -> bool {
        self.closed
    }

    /// The entities the live and waiting sounds follow, 0 for none.
    fn followed(&self) -> impl Iterator<Item = u32> + '_ {
        self.live
            .iter()
            .map(|live| live.follows)
            .chain(self.waiting.iter().map(|waiting| waiting.play.entity.get()))
    }

    /// Runs `op` of the mod at `owner`.
    fn run<M: Mixer>(
        &mut self,
        owner: usize,
        sounds: &ModSounds,
        op: Op,
        mixer: Option<&mut M>,
        followed: &Followed,
    ) {
        let Some(mixer) = mixer else {
            if let Op::Start(slot, _) = op {
                sounds.settle(slot, SoundInfo::FAILED, SoundInfo::OFF);
            }
            return;
        };
        match op {
            Op::Start(slot, play) if play.place == SoundPlay::ON => {
                match followed.seen(play.entity.get()) {
                    Seen::At(at) => self.start(owner, sounds, slot, play, Some(at), mixer),
                    Seen::Gone => {
                        sounds.settle(slot, SoundInfo::FAILED, SoundInfo::NO_ENTITY);
                    }
                    Seen::Unread if !Followed::fits(play.entity.get(), self.followed()) => {
                        sounds.settle(slot, SoundInfo::FAILED, SoundInfo::FOLLOW_FULL);
                    }
                    Seen::Unread => self.waiting.push(Waiting {
                        owner,
                        slot,
                        play,
                        frames: 0,
                    }),
                }
            }
            Op::Start(slot, play) => {
                let at = (play.place == SoundPlay::AT).then(|| fixed(play.at));
                self.start(owner, sounds, slot, play, at, mixer);
            }
            Op::Steer(slot, steer) => self.steer(owner, sounds, slot, &steer, mixer),
            Op::StopAll(fade_ms) => {
                for live in self.live.iter().filter(|live| live.owner == owner) {
                    mixer.stop(live.game, fade_ms);
                    sounds.settle(live.slot, SoundInfo::ENDED, 0);
                }
                self.live.retain(|live| live.owner != owner);
                for waiting in self.waiting.iter().filter(|waiting| waiting.owner == owner) {
                    sounds.settle(waiting.slot, SoundInfo::ENDED, 0);
                }
                self.waiting.retain(|waiting| waiting.owner != owner);
            }
        }
    }

    /// Starts `play` for the slot, which a stop before its start ended
    /// already, at `at` in the world or from no place.
    fn start<M: Mixer>(
        &mut self,
        owner: usize,
        sounds: &ModSounds,
        slot: Slot,
        play: SoundPlay,
        at: Option<[i64; 3]>,
        mixer: &mut M,
    ) {
        if !sounds.open(slot) {
            return;
        }
        let scene = Scene::of(play.place);
        if !mixer.reaches(scene) {
            sounds.settle(slot, SoundInfo::FAILED, SoundInfo::NO_SCENE);
            return;
        }
        let silent = if play.fade_in_ms > 0 {
            0.0
        } else {
            play.volume
        };
        let Some(game) = mixer.start(scene, play.container.0, at, silent) else {
            sounds.settle(slot, SoundInfo::FAILED, SoundInfo::GAME_REFUSED);
            return;
        };
        if play.fade_in_ms > 0 {
            mixer.volume(game, play.volume, play.fade_in_ms);
        }
        if play.place == SoundPlay::MUSIC {
            let handoff = if play.fade_in_ms > 0 {
                play.fade_in_ms
            } else {
                MUSIC_HANDOFF_MS
            };
            for earlier in self
                .live
                .iter()
                .filter(|live| live.owner == owner && live.place == SoundPlay::MUSIC)
            {
                mixer.stop(earlier.game, handoff);
                sounds.settle(earlier.slot, SoundInfo::ENDED, 0);
            }
            self.live
                .retain(|live| !(live.owner == owner && live.place == SoundPlay::MUSIC));
        }
        sounds.settle(slot, SoundInfo::PLAYING, 0);
        self.live.push(Live {
            owner,
            slot,
            game,
            place: play.place,
            follows: if play.place == SoundPlay::ON {
                play.entity.get()
            } else {
                0
            },
            at: at.unwrap_or_default(),
        });
    }

    /// Changes the mod's sound of `slot` by `steer`, if it still plays or
    /// waits.
    fn steer<M: Mixer>(
        &mut self,
        owner: usize,
        sounds: &ModSounds,
        slot: Slot,
        steer: &SoundSteer,
        mixer: &mut M,
    ) {
        if let Some(at) = self
            .waiting
            .iter()
            .position(|waiting| waiting.owner == owner && waiting.slot == slot)
        {
            match steer.kind {
                SoundSteer::STOP => {
                    self.waiting.swap_remove(at);
                    sounds.settle(slot, SoundInfo::ENDED, 0);
                }
                SoundSteer::VOLUME => self.waiting[at].play.volume = steer.volume,
                // Placed before its entity was read: it starts there and
                // follows nothing.
                _ => {
                    let mut play = self.waiting.swap_remove(at).play;
                    play.place = SoundPlay::AT;
                    play.at = steer.at;
                    self.start(owner, sounds, slot, play, Some(fixed(steer.at)), mixer);
                }
            }
            return;
        }
        let Some(at) = self
            .live
            .iter()
            .position(|live| live.owner == owner && live.slot == slot)
        else {
            // A sound not live has ended or failed already, keeping its reason.
            return;
        };
        let live = &mut self.live[at];
        match steer.kind {
            SoundSteer::STOP => {
                mixer.stop(live.game, steer.fade_ms);
                sounds.settle(slot, SoundInfo::ENDED, 0);
                self.live.swap_remove(at);
            }
            SoundSteer::VOLUME => mixer.volume(live.game, steer.volume, steer.fade_ms),
            // A sound on the screen or of music has no place to move.
            SoundSteer::POSITION if matches!(live.place, SoundPlay::AT | SoundPlay::ON) => {
                live.follows = 0;
                live.at = fixed(steer.at);
                mixer.position(live.game, live.at);
            }
            _ => {}
        }
    }

    /// Starts the waiting sounds whose entity a step read, and fails those
    /// that waited too long.
    fn wait<M: Mixer>(
        &mut self,
        frame: &Frame<'_>,
        mut mixer: Option<&mut M>,
        followed: &Followed,
    ) {
        let spare = core::mem::take(&mut self.spare);
        let mut waiting = core::mem::replace(&mut self.waiting, spare);
        for mut waits in waiting.drain(..) {
            let Some(Some(sounds)) = frame.mods.get(waits.owner) else {
                continue;
            };
            match (followed.seen(waits.play.entity.get()), mixer.as_deref_mut()) {
                (Seen::At(at), Some(mixer)) => {
                    self.start(waits.owner, sounds, waits.slot, waits.play, Some(at), mixer);
                }
                (_, None) => {
                    sounds.settle(waits.slot, SoundInfo::FAILED, SoundInfo::OFF);
                }
                (Seen::Gone, _) => {
                    sounds.settle(waits.slot, SoundInfo::FAILED, SoundInfo::NO_ENTITY);
                }
                (Seen::Unread, _) if waits.frames >= FOLLOW_WAIT_FRAMES => {
                    sounds.settle(waits.slot, SoundInfo::FAILED, SoundInfo::NO_ENTITY);
                }
                (Seen::Unread, _) => {
                    waits.frames += 1;
                    self.waiting.push(waits);
                }
            }
        }
        self.spare = waiting;
    }

    /// Moves each following sound to where its entity stands, and ends one
    /// whose entity is gone.
    fn follow<M: Mixer>(&mut self, frame: &Frame<'_>, mixer: Option<&mut M>, followed: &Followed) {
        let Some(mixer) = mixer else {
            return;
        };
        self.live.retain_mut(|live| {
            if live.follows == 0 {
                return true;
            }
            match followed.seen(live.follows) {
                Seen::At(at) if at != live.at => {
                    live.at = at;
                    mixer.position(live.game, at);
                    true
                }
                Seen::Gone => {
                    mixer.stop(live.game, END_FADE_MS);
                    if let Some(Some(sounds)) = frame.mods.get(live.owner) {
                        sounds.settle(live.slot, SoundInfo::ENDED, 0);
                    }
                    false
                }
                _ => true,
            }
        });
    }

    /// Ends the sounds the game no longer plays.
    fn reap<M: Mixer>(&mut self, frame: &Frame<'_>, mixer: Option<&mut M>) {
        let Some(mixer) = mixer else {
            return;
        };
        self.live.retain(|live| {
            let playing = mixer.playing(live.game);
            if !playing && let Some(Some(sounds)) = frame.mods.get(live.owner) {
                sounds.settle(live.slot, SoundInfo::ENDED, 0);
            }
            playing
        });
    }

    /// Stops and ends every sound of the mod at `owner`.
    fn end_owner<M: Mixer>(&mut self, owner: usize, sounds: &ModSounds, mut mixer: Option<&mut M>) {
        self.live.retain(|live| {
            if live.owner != owner {
                return true;
            }
            if let Some(mixer) = mixer.as_deref_mut() {
                mixer.stop(live.game, END_FADE_MS);
            }
            sounds.settle(live.slot, SoundInfo::ENDED, 0);
            false
        });
        self.waiting.retain(|waiting| {
            if waiting.owner != owner {
                return true;
            }
            sounds.settle(waiting.slot, SoundInfo::ENDED, 0);
            false
        });
    }

    /// Stops and ends every sound whose place `ends` names.
    fn end_where<M: Mixer>(
        &mut self,
        frame: &Frame<'_>,
        mut mixer: Option<&mut M>,
        ends: impl Fn(u32) -> bool,
    ) {
        let settle = |owner: usize, slot: Slot| {
            if let Some(Some(sounds)) = frame.mods.get(owner) {
                sounds.settle(slot, SoundInfo::ENDED, 0);
            }
        };
        self.live.retain(|live| {
            if !ends(live.place) {
                return true;
            }
            if let Some(mixer) = mixer.as_deref_mut() {
                mixer.stop(live.game, END_FADE_MS);
            }
            settle(live.owner, live.slot);
            false
        });
        self.waiting.retain(|waiting| {
            if !ends(waiting.play.place) {
                return true;
            }
            settle(waiting.owner, waiting.slot);
            false
        });
    }
}

/// A frame of the drain over a mixer that starts every sound and keeps it
/// playing: `live` sounds of one mod already play, and each run queues
/// `starts` more, which the frame starts, then stops them again so the next
/// run starts from the same state.
#[cfg(all(windows, feature = "test"))]
#[doc(hidden)]
pub fn bench_sound_frame(live: usize, starts: usize) -> impl FnMut() {
    struct Silent(u32);
    impl Mixer for Silent {
        fn reaches(&self, _: Scene) -> bool {
            true
        }
        fn start(&mut self, _: Scene, _: [u8; 16], _: Option<[i64; 3]>, _: f32) -> Option<u32> {
            self.0 += 1;
            Some(self.0)
        }
        fn stop(&mut self, _: u32, _: u32) {}
        fn volume(&mut self, _: u32, _: f32, _: u32) {}
        fn position(&mut self, _: u32, _: [i64; 3]) {}
        fn playing(&mut self, id: u32) -> bool {
            core::hint::black_box(id) != 0
        }
    }
    let sounds: &'static ModSounds = Box::leak(Box::new(ModSounds::new()));
    let followed: &'static Followed = Box::leak(Box::new(Followed::new()));
    let (mut drain, mut mixer) = (Drain::default(), Silent(0));
    let run = move |drain: &mut Drain, mixer: &mut Silent| {
        let frame = Frame {
            mods: &[Some(sounds)],
            off: &|_| false,
            session: 1,
        };
        drain.frame(Some(mixer), &frame, followed);
    };
    for _ in 0..live {
        let _taken = sounds.start(SoundPlay::EMPTY);
    }
    run(&mut drain, &mut mixer);
    move || {
        let slots: Vec<Slot> = (0..starts)
            .filter_map(|_| sounds.start(SoundPlay::EMPTY).ok())
            .collect();
        run(&mut drain, &mut mixer);
        for slot in slots {
            let _queued = sounds.queue(Op::Steer(slot, SoundSteer::EMPTY));
        }
        run(&mut drain, &mut mixer);
    }
}

#[cfg(test)]
mod tests;
