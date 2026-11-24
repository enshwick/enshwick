//! The fake's sound playback: the loader's refusals, a queue the next frame
//! starts, and the handles' states.

use crate::test::table::base::args;
use crate::test::table::entries;
use enshwick_core::PlayerKey;
use enshwick_core::abi::{
    ModHandle, Side, SoundAudience, SoundInfo, SoundPlay, SoundSteer, Status,
};

use super::holds_sound;
use crate::test::table::{State, base::registered, get_headed, on_game_thread, put_headed, with};

/// One sound a mod started, as the loader keeps it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Kept {
    pub(crate) play: SoundPlay,
    pub(crate) info: SoundInfo,
}

/// What the installed fake keeps of the mod's sounds.
#[derive(Default)]
pub(crate) struct SoundState {
    /// Every sound the mod started, its id its index plus one.
    pub(crate) sounds: Vec<Kept>,
    /// The starts waiting for the next frame.
    pub(crate) waiting: u32,
    /// Every sound the mod sent clients, its id its index plus one.
    pub(crate) sent: Vec<Sent>,
    /// The server step of the last sound sent and how many went in it.
    pub(crate) sent_in_step: (u64, u32),
}

/// One sound a mod sent clients, as the loader sends it: to whom, and the
/// play as they get it, changed by the steers that followed.
#[derive(Clone, Debug)]
pub(crate) struct Sent {
    #[cfg_attr(
        not(feature = "unstable"),
        expect(dead_code, reason = "only the unstable test readers read it")
    )]
    pub(crate) to: Vec<PlayerKey>,
    pub(crate) kept: Kept,
}

impl SoundState {
    /// Starts the waiting sounds as the loader's next frame does: a music
    /// sound fades out the mod's earlier one.
    pub(crate) fn frame(&mut self) {
        self.waiting = 0;
        let mut music = None;
        for (at, kept) in self.sounds.iter_mut().enumerate() {
            if kept.info.state == SoundInfo::PENDING {
                kept.info.state = SoundInfo::PLAYING;
                if kept.play.place == SoundPlay::MUSIC {
                    music = Some(at);
                }
            }
        }
        if let Some(newest) = music {
            for (at, kept) in self.sounds.iter_mut().enumerate() {
                let earlier = at < newest && kept.play.place == SoundPlay::MUSIC;
                if earlier && kept.info.state == SoundInfo::PLAYING {
                    kept.info.state = SoundInfo::ENDED;
                }
            }
        }
    }
}

/// Ends `kept` unless it already ended or failed.
fn end(kept: &mut Kept) {
    if matches!(kept.info.state, SoundInfo::PENDING | SoundInfo::PLAYING) {
        kept.info.state = SoundInfo::ENDED;
    }
}

entries! {
    /// Queues a sound as the loader does: unavailable on a server, a bad
    /// argument out of bounds, missing for an unknown container, busy past the
    /// frame's starts.
    pub(crate) fn sound_play(
        me as Mod,
        spec: *const SoundPlay as args::Headed,
        id: *mut u64 as args::Out,
    ) -> Status {
        let Some(spec) = get_headed(spec) else {
            return Status::BAD_ARGS;
        };
        if id.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| match start(s, spec) {
            Ok(started) => {
                // SAFETY: the safe layer passes a writable `u64`, non-null above.
                unsafe { id.write(started) };
                Status::OK
            }
            Err(status) => status,
        })
    }
}

/// Queues `spec` on the client as the loader's start does; its id.
fn start(s: &mut State, spec: SoundPlay) -> Result<u64, Status> {
    if s.host.base.side == Side::SERVER {
        return Err(Status::UNAVAILABLE);
    }
    if !spec.in_bounds() {
        return Err(Status::BAD_ARGS);
    }
    if !holds_sound(s, spec.container) {
        return Err(Status::MISSING);
    }
    let sounds = &mut s.ui.sound;
    if sounds.waiting >= SoundPlay::MAX_STARTS {
        return Err(Status::BUSY);
    }
    sounds.waiting += 1;
    sounds.sounds.push(Kept {
        play: spec,
        info: SoundInfo::EMPTY,
    });
    Ok(sounds.sounds.len() as u64)
}

/// Takes a sound the mod's server part sent this client, as the loader
/// does: queued as the mod's own start, or dropped for a container the
/// client lacks, a play out of bounds or a full frame. Whether it queued.
#[cfg_attr(
    not(feature = "unstable"),
    expect(dead_code, reason = "only the unstable test readers hear a sent sound")
)]
pub(crate) fn hear(spec: SoundPlay) -> bool {
    with(false, |s| start(s, spec).is_ok())
}

/// Changes `kept` by `steer`, a stop, a volume or a position; a sound on
/// the screen or of music has no place to move.
fn apply(kept: &mut Kept, steer: &SoundSteer) {
    match steer.kind {
        SoundSteer::STOP => end(kept),
        SoundSteer::VOLUME => kept.play.volume = steer.volume,
        SoundSteer::POSITION if matches!(kept.play.place, SoundPlay::AT | SoundPlay::ON) => {
            kept.play.place = SoundPlay::AT;
            kept.play.at = steer.at;
        }
        _ => {}
    }
}

entries! {
    /// Sends a sound to clients as the loader does, refusing what it refuses
    /// in its order; the clients are the players whose client runs Enshwick
    /// ([`FakeHost::net_peer`](crate::test::FakeHost::net_peer)) or who host,
    /// picked by the audience from where their characters stand
    /// ([`FakeHost::placed`](crate::test::FakeHost::placed)).
    pub(crate) fn sound_play_for(
        me as Mod,
        audience: *const SoundAudience as args::Headed,
        spec: *const SoundPlay as args::Headed,
        id: *mut u64 as args::Out,
    ) -> Status {
        let (Some(audience), Some(spec)) = (get_headed(audience), get_headed(spec)) else {
            return Status::BAD_ARGS;
        };
        if id.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| match send(s, me, &audience, spec) {
            Ok(sent) => {
                // SAFETY: the safe layer passes a writable `u64`, non-null above.
                unsafe { id.write(sent) };
                Status::OK
            }
            Err(status) => status,
        })
    }
}

/// Where entity `entity` stands, as the test placed it.
fn position(s: &State, entity: enshwick_core::Entity) -> Option<[f64; 3]> {
    s.host
        .world
        .placed
        .iter()
        .find(|(_, place)| place.entity == entity)
        .map(|(_, place)| place.position)
}

/// Refuses a server sound call as the loader does, in its order: where no
/// server runs, then for a mod not on both sides.
fn serving(s: &State, me: ModHandle) -> Result<(), Status> {
    let base = &s.host.base;
    if base.side == Side::CLIENT && !base.hosting {
        return Err(Status::UNAVAILABLE);
    }
    if !super::super::base::mods::joins(s, me) {
        return Err(Status::REFUSED);
    }
    Ok(())
}

/// [`sound_play_for`] past its pointer checks; the sound's id.
fn send(
    s: &mut State,
    me: ModHandle,
    audience: &SoundAudience,
    mut spec: SoundPlay,
) -> Result<u64, Status> {
    serving(s, me)?;
    if !audience.in_bounds() || !spec.in_bounds() {
        return Err(Status::BAD_ARGS);
    }
    let reads = audience.kind == SoundAudience::NEAR || spec.place == SoundPlay::ON;
    if reads && !on_game_thread() {
        return Err(Status::WRONG_THREAD);
    }
    if spec.place == SoundPlay::ON {
        spec.at = position(s, spec.entity).ok_or(Status::MISSING)?;
        spec.place = SoundPlay::AT;
        spec.entity = enshwick_core::Entity::NONE;
    }
    let step = s.base.ticks.step;
    let (last, count) = s.ui.sound.sent_in_step;
    let count = if last == step { count } else { 0 };
    if count >= SoundAudience::MAX_PLAYS {
        return Err(Status::BUSY);
    }
    let base = &s.host.base;
    let host = base.host_key.filter(|_| base.hosting);
    let to = base
        .players
        .iter()
        .filter(|p| base.net_peers.contains(&p.key.get()) || Some(p.key) == host)
        .filter(|p| audience.hears(p.key, position(s, p.entity)))
        .map(|p| p.key)
        .collect();
    let sounds = &mut s.ui.sound;
    sounds.sent_in_step = (step, count + 1);
    sounds.sent.push(Sent {
        to,
        kept: Kept {
            play: spec,
            info: SoundInfo {
                state: SoundInfo::PLAYING,
                ..SoundInfo::EMPTY
            },
        },
    });
    Ok(sounds.sent.len() as u64)
}

entries! {
    /// Changes a sound the mod sent clients, as the loader does; an id of no
    /// such sound changes nothing.
    pub(crate) fn sound_steer_for(
        me as Mod,
        id: u64,
        steer: *const SoundSteer as args::Headed,
    ) -> Status {
        let Some(steer) = get_headed(steer) else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            if let Err(status) = serving(s, me) {
                return status;
            }
            if !steer.in_bounds() || steer.kind == SoundSteer::STOP_ALL {
                return Status::BAD_ARGS;
            }
            let sent = usize::try_from(id)
                .ok()
                .and_then(|id| id.checked_sub(1))
                .and_then(|at| s.ui.sound.sent.get_mut(at));
            if let Some(sent) = sent {
                apply(&mut sent.kept, &steer);
            }
            Status::OK
        })
    }

    /// Changes the mod's sound `id`, or stops all of them, as the loader does;
    /// an id of no sound changes nothing.
    pub(crate) fn sound_steer(
        me as Mod,
        id: u64,
        steer: *const SoundSteer as args::Headed,
    ) -> Status {
        let Some(steer) = get_headed(steer) else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            if !steer.in_bounds() {
                return Status::BAD_ARGS;
            }
            let sounds = &mut s.ui.sound.sounds;
            if steer.kind == SoundSteer::STOP_ALL {
                sounds.iter_mut().for_each(end);
                return Status::OK;
            }
            let kept = usize::try_from(id)
                .ok()
                .and_then(|id| id.checked_sub(1))
                .and_then(|at| sounds.get_mut(at))
                .filter(|kept| matches!(kept.info.state, SoundInfo::PENDING | SoundInfo::PLAYING));
            if let Some(kept) = kept {
                apply(kept, &steer);
            }
            Status::OK
        })
    }

    /// Where the mod's sound `id` stands; an unknown id, and every sound of a
    /// mod turned off, reads as ended.
    pub(crate) fn sound_state(
        me as Mod,
        id: u64,
        out: *mut SoundInfo as args::HeadedOut,
    ) -> Status {
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        let off = registered::is_off(me);
        with(Status::MISSING, |s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            let kept = usize::try_from(id)
                .ok()
                .and_then(|id| id.checked_sub(1))
                .and_then(|at| s.ui.sound.sounds.get_mut(at));
            let info = match kept {
                Some(kept) => {
                    if off {
                        end(kept);
                    }
                    kept.info
                }
                None => SoundInfo {
                    state: SoundInfo::ENDED,
                    ..SoundInfo::EMPTY
                },
            };
            put_headed(out, info)
        })
    }
}
