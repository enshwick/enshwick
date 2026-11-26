use enshwick_core::Entity;
use enshwick_core::abi::{SoundInfo, SoundPlay, SoundSteer};
use enshwick_core::content::Guid;

use super::*;
use crate::ui::sound::follow::{Followed, MAX_FOLLOWED};
use crate::ui::sound::kept::{ModSounds, Op, Slot};

/// A mixer that records its calls; every scene but the ones in `missing`
/// is reached, and ids it stops or `ends` no longer play.
#[derive(Default)]
struct Recorder {
    missing: Vec<Scene>,
    refuse: bool,
    next: u32,
    started: Vec<(u32, Scene, Option<[i64; 3]>, f32)>,
    stopped: Vec<(u32, u32)>,
    volumes: Vec<(u32, f32, u32)>,
    moved: Vec<(u32, [i64; 3])>,
    ended: Vec<u32>,
}

impl Mixer for Recorder {
    fn reaches(&self, scene: Scene) -> bool {
        !self.missing.contains(&scene)
    }

    fn start(
        &mut self,
        scene: Scene,
        _: [u8; 16],
        at: Option<[i64; 3]>,
        volume: f32,
    ) -> Option<u32> {
        if self.refuse {
            return None;
        }
        self.next += 1;
        self.started.push((self.next, scene, at, volume));
        Some(self.next)
    }

    fn stop(&mut self, id: u32, fade_ms: u32) {
        self.stopped.push((id, fade_ms));
    }

    fn volume(&mut self, id: u32, volume: f32, fade_ms: u32) {
        self.volumes.push((id, volume, fade_ms));
    }

    fn position(&mut self, id: u32, at: [i64; 3]) {
        self.moved.push((id, at));
    }

    fn playing(&mut self, id: u32) -> bool {
        !self.ended.contains(&id) && !self.stopped.iter().any(|&(stopped, _)| stopped == id)
    }
}

const BELL: Guid = Guid([7; 16]);

fn play(place: u32) -> SoundPlay {
    enshwick_abi::headed!(SoundPlay {
        container: BELL,
        place,
        entity: if place == SoundPlay::ON {
            Entity::from_engine(9)
        } else {
            Entity::NONE
        },
        ..SoundPlay::EMPTY
    })
}

/// One frame over `mods`, none off, in session 1.
fn run(drain: &mut Drain, mixer: &mut Recorder, mods: &[&ModSounds], followed: &Followed) {
    run_with(drain, Some(mixer), mods, followed, &|_| false, 1);
}

fn run_with(
    drain: &mut Drain,
    mixer: Option<&mut Recorder>,
    mods: &[&ModSounds],
    followed: &Followed,
    off: &dyn Fn(usize) -> bool,
    session: u64,
) {
    let mods: Vec<Option<&ModSounds>> = mods.iter().copied().map(Some).collect();
    let frame = Frame {
        mods: &mods,
        off,
        session,
    };
    drain.frame(mixer, &frame, followed);
}

fn state(sounds: &ModSounds, slot: Slot) -> (u32, u32) {
    let info = sounds.info(slot.id());
    (info.state, info.reason)
}

#[test]
fn a_started_sound_plays_until_the_game_ends_it() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let slot = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(state(&sounds, slot).0, SoundInfo::PLAYING);
    assert_eq!(mixer.started, [(1, Scene::Ui, None, 1.0)]);
    mixer.ended.push(1);
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(state(&sounds, slot).0, SoundInfo::ENDED);
}

#[test]
fn a_start_fails_with_its_reason_when_the_scene_the_game_or_the_playback_is_missing() {
    let (mut drain, followed, sounds) = (Drain::default(), Followed::new(), ModSounds::new());
    let mut unreached = Recorder {
        missing: vec![Scene::Music],
        ..Recorder::default()
    };
    let music = sounds.start(play(SoundPlay::MUSIC)).unwrap();
    run(&mut drain, &mut unreached, &[&sounds], &followed);
    assert_eq!(
        state(&sounds, music),
        (SoundInfo::FAILED, SoundInfo::NO_SCENE)
    );

    let mut refusing = Recorder {
        refuse: true,
        ..Recorder::default()
    };
    let refused = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    run(&mut drain, &mut refusing, &[&sounds], &followed);
    assert_eq!(
        state(&sounds, refused),
        (SoundInfo::FAILED, SoundInfo::GAME_REFUSED)
    );

    let off = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    run_with(&mut drain, None, &[&sounds], &followed, &|_| false, 1);
    assert_eq!(state(&sounds, off), (SoundInfo::FAILED, SoundInfo::OFF));
}

#[test]
fn a_place_in_the_world_reaches_the_game_in_fixed_point() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let _at = sounds
        .start(enshwick_abi::headed!(SoundPlay {
            at: [1.5, -2.0, 0.0],
            ..play(SoundPlay::AT)
        }))
        .unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    // The game truncates after adding 0.5, so -2 m lands one step above.
    let expected = [3 << 31, -(2_i64 << 32) + 1, 0];
    assert_eq!(mixer.started, [(1, Scene::Game, Some(expected), 1.0)]);
}

#[test]
fn a_fade_in_starts_silent_and_rises_to_its_volume() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let _faded = sounds
        .start(enshwick_abi::headed!(SoundPlay {
            volume: 0.5,
            fade_in_ms: 300,
            ..play(SoundPlay::SCREEN)
        }))
        .unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(mixer.started[0].3, 0.0);
    assert_eq!(mixer.volumes, [(1, 0.5, 300)]);
}

#[test]
fn stop_all_reaches_only_the_calling_mod_s_sounds() {
    let (mut drain, mut mixer, followed) = (Drain::default(), Recorder::default(), Followed::new());
    let (mine, theirs) = (ModSounds::new(), ModSounds::new());
    let my_sound = mine.start(play(SoundPlay::SCREEN)).unwrap();
    let their_sound = theirs.start(play(SoundPlay::SCREEN)).unwrap();
    run(&mut drain, &mut mixer, &[&mine, &theirs], &followed);
    mine.queue(Op::StopAll(100)).unwrap();
    run(&mut drain, &mut mixer, &[&mine, &theirs], &followed);
    assert_eq!(state(&mine, my_sound).0, SoundInfo::ENDED);
    assert_eq!(state(&theirs, their_sound).0, SoundInfo::PLAYING);
    assert_eq!(mixer.stopped, [(1, 100)]);
}

#[test]
fn a_stale_id_changes_nothing_of_the_sound_that_took_its_slot() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let old = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    sounds
        .queue(Op::Steer(
            old,
            enshwick_abi::headed!(SoundSteer {
                fade_ms: 0,
                ..SoundSteer::EMPTY
            }),
        ))
        .unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    let new = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    assert_eq!(new.index, old.index);
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    sounds
        .queue(Op::Steer(
            old,
            enshwick_abi::headed!(SoundSteer {
                kind: SoundSteer::VOLUME,
                volume: 0.1,
                ..SoundSteer::EMPTY
            }),
        ))
        .unwrap();
    sounds.queue(Op::Steer(old, SoundSteer::EMPTY)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(state(&sounds, new).0, SoundInfo::PLAYING);
    assert!(mixer.volumes.is_empty());
    assert_eq!(mixer.stopped, [(1, 0)]);
}

#[test]
fn a_mod_turned_off_ends_every_sound_it_plays_music_and_interface_included() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let screen = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    let music = sounds.start(play(SoundPlay::MUSIC)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    let queued = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    run_with(
        &mut drain,
        Some(&mut mixer),
        &[&sounds],
        &followed,
        &|_| true,
        1,
    );
    for slot in [screen, music, queued] {
        assert_eq!(state(&sounds, slot).0, SoundInfo::ENDED);
    }
    assert_eq!(mixer.started.len(), 2);
}

#[test]
fn a_new_world_ends_the_sounds_of_the_old_one_but_not_music_or_interface_sounds() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let world = sounds.start(play(SoundPlay::AT)).unwrap();
    let screen = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    let music = sounds.start(play(SoundPlay::MUSIC)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    run_with(
        &mut drain,
        Some(&mut mixer),
        &[&sounds],
        &followed,
        &|_| false,
        2,
    );
    assert_eq!(state(&sounds, world).0, SoundInfo::ENDED);
    assert_eq!(state(&sounds, screen).0, SoundInfo::PLAYING);
    assert_eq!(state(&sounds, music).0, SoundInfo::PLAYING);
}

#[test]
fn a_mod_s_new_music_fades_out_its_earlier_music_and_no_other_mod_s() {
    let (mut drain, mut mixer, followed) = (Drain::default(), Recorder::default(), Followed::new());
    let (mine, theirs) = (ModSounds::new(), ModSounds::new());
    let first = mine.start(play(SoundPlay::MUSIC)).unwrap();
    let their_music = theirs.start(play(SoundPlay::MUSIC)).unwrap();
    run(&mut drain, &mut mixer, &[&mine, &theirs], &followed);
    let second = mine.start(play(SoundPlay::MUSIC)).unwrap();
    run(&mut drain, &mut mixer, &[&mine, &theirs], &followed);
    assert_eq!(state(&mine, first).0, SoundInfo::ENDED);
    assert_eq!(state(&mine, second).0, SoundInfo::PLAYING);
    assert_eq!(state(&theirs, their_music).0, SoundInfo::PLAYING);
    assert_eq!(mixer.stopped, [(1, MUSIC_HANDOFF_MS)]);
}

#[test]
fn a_sound_on_an_entity_waits_for_its_read_follows_it_and_ends_when_it_is_gone() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let on = sounds.start(play(SoundPlay::ON)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(state(&sounds, on).0, SoundInfo::PENDING);
    assert!(mixer.started.is_empty());
    followed.step(|_| Some([1, 2, 3]));
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(mixer.started, [(1, Scene::Game, Some([1, 2, 3]), 1.0)]);
    followed.step(|_| Some([4, 5, 6]));
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(mixer.moved, [(1, [4, 5, 6])]);
    followed.step(|_| None);
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(state(&sounds, on).0, SoundInfo::ENDED);
    assert_eq!(mixer.stopped, [(1, END_FADE_MS)]);
}

#[test]
fn a_sound_on_an_entity_no_step_reads_fails_after_its_wait() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let on = sounds.start(play(SoundPlay::ON)).unwrap();
    for _ in 0..=FOLLOW_WAIT_FRAMES + 1 {
        run(&mut drain, &mut mixer, &[&sounds], &followed);
    }
    assert_eq!(
        state(&sounds, on),
        (SoundInfo::FAILED, SoundInfo::NO_ENTITY)
    );
}

#[test]
fn moving_a_following_sound_stops_the_following() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let on = sounds.start(play(SoundPlay::ON)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    followed.step(|_| Some([1, 2, 3]));
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    let steer = enshwick_abi::headed!(SoundSteer {
        kind: SoundSteer::POSITION,
        at: [0.0; 3],
        ..SoundSteer::EMPTY
    });
    sounds.queue(Op::Steer(on, steer)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    followed.step(|_| None);
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(state(&sounds, on).0, SoundInfo::PLAYING);
    assert_eq!(mixer.moved, [(1, [0; 3])]);
}

fn steer(kind: u32) -> SoundSteer {
    enshwick_abi::headed!(SoundSteer {
        kind,
        volume: 0.25,
        ..SoundSteer::EMPTY
    })
}

#[test]
fn the_first_world_session_counted_ends_no_sound() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let world = sounds.start(play(SoundPlay::AT)).unwrap();
    let at = |drain: &mut Drain, mixer: &mut Recorder, session| {
        run_with(
            drain,
            Some(mixer),
            &[&sounds],
            &followed,
            &|_| false,
            session,
        );
    };
    at(&mut drain, &mut mixer, 0);
    at(&mut drain, &mut mixer, 1);
    assert_eq!(state(&sounds, world).0, SoundInfo::PLAYING);
    at(&mut drain, &mut mixer, 2);
    assert_eq!(state(&sounds, world).0, SoundInfo::ENDED);
}

#[test]
fn a_change_to_a_sound_waiting_for_its_entity_holds_when_it_starts() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let quiet = sounds.start(play(SoundPlay::ON)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    sounds
        .queue(Op::Steer(quiet, steer(SoundSteer::VOLUME)))
        .unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    followed.step(|_| Some([1, 2, 3]));
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(mixer.started, [(1, Scene::Game, Some([1, 2, 3]), 0.25)]);
    let placed = sounds
        .start(enshwick_abi::headed!(SoundPlay {
            entity: Entity::from_engine(10),
            ..play(SoundPlay::ON)
        }))
        .unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    sounds
        .queue(Op::Steer(placed, steer(SoundSteer::POSITION)))
        .unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(mixer.started[1], (2, Scene::Game, Some([0; 3]), 1.0));
    followed.step(|entity| (entity == 9).then_some([1, 2, 3]));
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(
        state(&sounds, placed).0,
        SoundInfo::PLAYING,
        "it follows no entity"
    );
}

#[test]
fn a_sound_on_one_entity_more_than_the_client_follows_fails_with_its_reason() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let on = |entity| {
        enshwick_abi::headed!(SoundPlay {
            entity: Entity::from_engine(entity),
            ..play(SoundPlay::ON)
        })
    };
    let most = u32::try_from(MAX_FOLLOWED).unwrap();
    for entity in 1..=most {
        sounds.start(on(entity)).unwrap();
    }
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    let past = sounds.start(on(most + 1)).unwrap();
    let shared = sounds.start(on(1)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(
        state(&sounds, past),
        (SoundInfo::FAILED, SoundInfo::FOLLOW_FULL)
    );
    assert_eq!(state(&sounds, shared).0, SoundInfo::PENDING);
    followed.step(|entity| Some([i64::from(entity); 3]));
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert_eq!(mixer.started.len(), MAX_FOLLOWED + 1, "every entity read");
}

/// A mixer that queues another change of `slot` at each volume it sets, as
/// a mod's thread that never stops queueing would.
struct Refiller<'a> {
    sounds: &'a ModSounds,
    slot: Slot,
    volumes: u32,
}

impl Mixer for Refiller<'_> {
    fn reaches(&self, _: Scene) -> bool {
        true
    }

    fn start(&mut self, _: Scene, _: [u8; 16], _: Option<[i64; 3]>, _: f32) -> Option<u32> {
        Some(1)
    }

    fn stop(&mut self, _: u32, _: u32) {}

    fn volume(&mut self, _: u32, _: f32, _: u32) {
        self.volumes += 1;
        let _queued = self
            .sounds
            .queue(Op::Steer(self.slot, steer(SoundSteer::VOLUME)));
    }

    fn position(&mut self, _: u32, _: [i64; 3]) {}

    fn playing(&mut self, _: u32) -> bool {
        true
    }
}

#[test]
fn a_mod_that_keeps_queueing_changes_cannot_hold_the_frame() {
    let (mut drain, followed, sounds) = (Drain::default(), Followed::new(), ModSounds::new());
    let slot = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    let mut refiller = Refiller {
        sounds: &sounds,
        slot,
        volumes: 0,
    };
    let mods = [Some(&sounds)];
    let frame = Frame {
        mods: &mods,
        off: &|_| false,
        session: 1,
    };
    drain.frame(Some(&mut refiller), &frame, &followed);
    sounds
        .queue(Op::Steer(slot, steer(SoundSteer::VOLUME)))
        .unwrap();
    drain.frame(Some(&mut refiller), &frame, &followed);
    assert_eq!(refiller.volumes, 1);
    assert_eq!(
        sounds.batch().count(),
        1,
        "the change queued meanwhile waits"
    );
}

#[test]
fn a_move_of_a_sound_on_the_screen_or_of_music_changes_nothing() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let screen = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    let music = sounds.start(play(SoundPlay::MUSIC)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    for slot in [screen, music] {
        sounds
            .queue(Op::Steer(slot, steer(SoundSteer::POSITION)))
            .unwrap();
    }
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    assert!(mixer.moved.is_empty());
    assert_eq!(state(&sounds, screen).0, SoundInfo::PLAYING);
}

#[test]
fn closing_after_a_fault_stops_every_sound_and_fails_every_start_left() {
    let (mut drain, mut mixer, followed, sounds) = (
        Drain::default(),
        Recorder::default(),
        Followed::new(),
        ModSounds::new(),
    );
    let playing = sounds.start(play(SoundPlay::SCREEN)).unwrap();
    let waiting = sounds.start(play(SoundPlay::ON)).unwrap();
    run(&mut drain, &mut mixer, &[&sounds], &followed);
    let queued = sounds.start(play(SoundPlay::MUSIC)).unwrap();
    let mods = [Some(&sounds)];
    let frame = Frame {
        mods: &mods,
        off: &|_| false,
        session: 1,
    };
    drain.close(Some(&mut mixer), &frame);
    assert!(drain.closed());
    assert_eq!(state(&sounds, playing).0, SoundInfo::ENDED);
    assert_eq!(mixer.stopped, [(1, END_FADE_MS)]);
    for slot in [waiting, queued] {
        assert_eq!(state(&sounds, slot), (SoundInfo::FAILED, SoundInfo::OFF));
    }
}
