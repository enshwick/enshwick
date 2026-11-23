//! The fake's sound playback: the loader's refusals, a queue the next frame
//! starts, and the handles' states.

use enshwick_core::abi::{ModHandle, Side, SoundInfo, SoundPlay, SoundSteer, Status};

use super::holds_sound;
use crate::test::table::{base::registered, get_headed, put_headed, with};

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

/// Queues a sound as the loader does: unavailable on a server, a bad
/// argument out of bounds, missing for an unknown container, busy past the
/// frame's starts.
pub(crate) extern "system" fn sound_play(
    _me: ModHandle,
    spec: *const SoundPlay,
    id: *mut u64,
) -> Status {
    let Some(spec) = get_headed(spec) else {
        return Status::BAD_ARGS;
    };
    if id.is_null() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.host.base.side == Side::SERVER {
            return Status::UNAVAILABLE;
        }
        if !spec.in_bounds() {
            return Status::BAD_ARGS;
        }
        if !holds_sound(s, spec.container) {
            return Status::MISSING;
        }
        let sounds = &mut s.ui.sound;
        if sounds.waiting >= SoundPlay::MAX_STARTS {
            return Status::BUSY;
        }
        sounds.waiting += 1;
        sounds.sounds.push(Kept {
            play: spec,
            info: SoundInfo::EMPTY,
        });
        // SAFETY: the safe layer passes a writable `u64`, non-null above.
        unsafe { id.write(sounds.sounds.len() as u64) };
        Status::OK
    })
}

/// Changes the mod's sound `id`, or stops all of them, as the loader does;
/// an id of no sound changes nothing.
pub(crate) extern "system" fn sound_steer(
    _me: ModHandle,
    id: u64,
    steer: *const SoundSteer,
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
            match steer.kind {
                SoundSteer::STOP => end(kept),
                SoundSteer::VOLUME => kept.play.volume = steer.volume,
                _ => {
                    kept.play.place = SoundPlay::AT;
                    kept.play.at = steer.at;
                }
            }
        }
        Status::OK
    })
}

/// Where the mod's sound `id` stands; an unknown id, and every sound of a
/// mod turned off, reads as ended.
pub(crate) extern "system" fn sound_state(me: ModHandle, id: u64, out: *mut SoundInfo) -> Status {
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
