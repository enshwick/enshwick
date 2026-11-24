//! Sounds client mods play through the game's sound system: a mod queues a
//! start or a change from any thread into a bounded lock-free queue of its
//! own, and the interface frame, on the game's main thread where the game's
//! own `ui.sound` starts sounds, runs every mod's queue, moves the sounds
//! that follow entities and ends the sounds the game no longer plays.
//!
//! The game's start links its instance into the sound system's list without
//! a lock (client 26430625, `0x106ee50`), so no call of the game's sound
//! system runs on another thread.

mod drain;
mod follow;
#[cfg(windows)]
mod game;
mod kept;
mod remote;

#[cfg(all(windows, feature = "test"))]
pub use drain::bench_sound_frame;
#[cfg(windows)]
pub(crate) use remote::{RelayError, play_for, received, steer_for};
#[cfg(windows)]
pub(crate) use windows::*;

#[cfg(windows)]
mod windows {
    use core::cell::RefCell;
    use std::sync::OnceLock;

    use enshwick_core::abi::{Side, SoundInfo, SoundPlay, SoundSteer};

    use super::drain::{Drain, Frame};
    use super::follow::Followed;
    use super::game::{Calls, GameMixer};
    use super::kept::{ModSounds, Op, QueueError, Slot};
    use crate::engine::fuse::Fuse;
    use crate::engine::loader::Loader;
    use crate::engine::mod_index::ModIndex;

    pub(crate) use super::game::TABLE_IDS;

    /// The mods' sounds: off on a build that lacks the sound system's calls
    /// or after a fault in the frame's work.
    pub(crate) static FUSE: Fuse = Fuse::new("mods' sounds");
    /// The reads of where followed entities stand, in the client's step.
    pub(crate) static FOLLOW_FUSE: Fuse = Fuse::new("sounds following entities");
    /// The one pass that stops the mods' sounds once [`FUSE`] is off.
    static CLOSE_FUSE: Fuse = Fuse::new("stopping the mods' sounds after a fault");

    /// What the mods' sounds keep while the game runs.
    struct State {
        /// The sound system's calls, once linked on a build that has them.
        calls: OnceLock<Calls>,
        /// Each mod's sounds by mod index, made at its first sound.
        mods: OnceLock<Box<[OnceLock<ModSounds>]>>,
        followed: Followed,
    }

    static STATE: State = State {
        calls: OnceLock::new(),
        mods: OnceLock::new(),
        followed: Followed::new(),
    };

    thread_local! {
        /// What the frames keep between them, on the interface thread.
        static DRAIN: RefCell<Drain> = RefCell::new(Drain::default());
        /// The mods' sounds as a frame hands them on, refilled in place.
        static KEPT: RefCell<Vec<Option<&'static ModSounds>>> = const { RefCell::new(Vec::new()) };
    }

    /// Why a mod's sound call is refused.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
    pub(crate) enum SoundError {
        /// Only a client plays sounds.
        #[error("sounds play on a client")]
        NotClient,
        /// The place, volume, fade or position is out of bounds.
        #[error("the sound's place, volume, fade or position is out of bounds")]
        BadArgs,
        /// Neither the game's archive nor a mod holds the container.
        #[error("no such sound container")]
        Missing,
        /// The mod's starts for this frame, its sounds or its queue are full.
        #[error("the mod's sounds or starts for this frame are full")]
        Full,
        /// The loader's sound playback is off on this build.
        #[error("the loader's sound playback is off")]
        Off,
    }

    impl From<QueueError> for SoundError {
        fn from(QueueError::Full: QueueError) -> Self {
            Self::Full
        }
    }

    /// Resolves the sound system's calls on a client; without them the
    /// mods' sounds stay off, with the missing entry in the log and on the
    /// Mods screen.
    pub(crate) fn link(loader: &Loader) {
        if loader.side != Side::CLIENT {
            return;
        }
        crate::base::net::on_control(crate::base::wire::SOUND, super::received);
        match Calls::resolve(loader) {
            Ok(calls) => {
                let _first = STATE.calls.set(calls);
                crate::actions::local_held::read_positions(follows, follow_step);
            }
            Err(missing) => {
                let _off =
                    loader.fuse_off(&FUSE, missing, "the sound system's calls are not found");
            }
        }
    }

    /// The sounds of the mod at `owner`, made at its first ask.
    fn sounds(loader: &Loader, owner: ModIndex) -> Option<&'static ModSounds> {
        let mods = STATE
            .mods
            .get_or_init(|| (0..loader.mods.len()).map(|_| OnceLock::new()).collect());
        Some(mods.get(owner.get())?.get_or_init(ModSounds::new))
    }

    /// Whether the client holds a container with GUID `guid`: the game's
    /// archive, or a mod that serves it.
    pub(crate) fn holds_sound(guid: [u8; 16]) -> bool {
        use enshwick_gamedata::catalogue;
        let Some(type_hash) = catalogue::type_hash(catalogue::ids::SOUND_CONTAINER) else {
            return false;
        };
        let key = enshwick_core::abi::ResourceKey {
            guid: enshwick_core::content::Guid(guid),
            type_hash,
            part: 0,
        };
        crate::engine::archive::game().is_some_and(|archive| archive.contains(&key))
            || crate::content::served::mods_serve(&key)
    }

    /// Refuses a call off a client or while the sounds are off.
    fn on(loader: &Loader) -> Result<(), SoundError> {
        if loader.side != Side::CLIENT {
            Err(SoundError::NotClient)
        } else if FUSE.is_off() {
            Err(SoundError::Off)
        } else {
            Ok(())
        }
    }

    /// Queues mod `owner`'s start of `spec`; the sound's id.
    ///
    /// # Errors
    ///
    /// [`SoundError`] for a refused start, as [`SoundError`]'s variants say.
    pub(crate) fn play(
        loader: &Loader,
        owner: ModIndex,
        spec: &SoundPlay,
    ) -> Result<u64, SoundError> {
        on(loader)?;
        if !spec.in_bounds() {
            return Err(SoundError::BadArgs);
        }
        if !holds_sound(spec.container.0) {
            return Err(SoundError::Missing);
        }
        // The world's session tells the frames when a world ends.
        crate::actions::client_snapshot::ask();
        let sounds = sounds(loader, owner).ok_or(SoundError::Off)?;
        Ok(sounds.start(*spec).map(Slot::id)?)
    }

    /// Queues mod `owner`'s `steer` of its sound `id`.
    ///
    /// # Errors
    ///
    /// [`SoundError::BadArgs`] out of bounds, [`SoundError::Full`] with the
    /// mod's queue full, [`SoundError::NotClient`] off a client.
    pub(crate) fn steer(
        loader: &Loader,
        owner: ModIndex,
        id: u64,
        steer: &SoundSteer,
    ) -> Result<(), SoundError> {
        on(loader)?;
        if !steer.in_bounds() {
            return Err(SoundError::BadArgs);
        }
        let sounds = sounds(loader, owner).ok_or(SoundError::Off)?;
        let op = if steer.kind == SoundSteer::STOP_ALL {
            Op::StopAll(steer.fade_ms)
        } else {
            match Slot::of(id) {
                Some(slot) if sounds.open(slot) => Op::Steer(slot, *steer),
                _ => return Ok(()),
            }
        };
        Ok(sounds.queue(op)?)
    }

    /// Where mod `owner`'s sound `id` stands.
    ///
    /// # Errors
    ///
    /// [`SoundError::NotClient`] off a client.
    pub(crate) fn state(
        loader: &Loader,
        owner: ModIndex,
        id: u64,
    ) -> Result<SoundInfo, SoundError> {
        if loader.side != Side::CLIENT {
            return Err(SoundError::NotClient);
        }
        Ok(sounds(loader, owner).map_or(
            SoundInfo {
                state: SoundInfo::ENDED,
                ..SoundInfo::EMPTY
            },
            |sounds| sounds.info(id),
        ))
    }

    /// Runs the mods' sounds for one interface frame.
    ///
    /// # Safety
    ///
    /// - Called inside the interface frame on its thread, with `context`
    ///   the frame's own live context, or 0 outside a game's interface.
    pub(crate) unsafe fn frame(context: usize) {
        let (Some(mods), Some(calls)) = (STATE.mods.get(), STATE.calls.get()) else {
            return;
        };
        if context == 0 {
            return;
        }
        // After a fault one pass stops what the mods' sounds still play.
        let closing = FUSE.is_off();
        if closing && DRAIN.with_borrow(Drain::closed) {
            return;
        }
        let (fuse, site) = if closing {
            (&CLOSE_FUSE, "stopping the mods' sounds")
        } else {
            (&FUSE, "the mods' sounds")
        };
        let _ran = fuse.run(site, || {
            // SAFETY: the caller's frame on the interface thread, with its
            // live, non-zero context.
            let Some(mut mixer) = (unsafe { GameMixer::of(calls, context) }) else {
                // No sound system yet: the queues wait for a frame with one.
                return;
            };
            KEPT.with_borrow_mut(|kept| {
                kept.clear();
                kept.extend(mods.iter().map(OnceLock::get));
                let off = |index: usize| crate::engine::loader::is_off(ModIndex::new(index));
                let frame = Frame {
                    mods: kept,
                    off: &off,
                    session: crate::actions::client_snapshot::session(),
                };
                DRAIN.with_borrow_mut(|drain| {
                    if closing {
                        drain.close(Some(&mut mixer), &frame);
                    } else {
                        drain.frame(Some(&mut mixer), &frame, &STATE.followed);
                    }
                });
            });
        });
    }

    /// Whether a step should read where followed entities stand.
    fn follows() -> bool {
        STATE.followed.any() && !FOLLOW_FUSE.is_off()
    }

    /// Runs on the client's step: reads where each followed entity stands
    /// with `read`, `None` for one the client's world lacks.
    fn follow_step(read: &dyn Fn(u32) -> Option<[i64; 3]>) {
        let _read = FOLLOW_FUSE.run("the followed entities' read", || STATE.followed.step(read));
    }
}
