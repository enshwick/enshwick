//! `sounds.toml`: a pack's sounds, changes and replaced recordings read,
//! named in its namespace and checked.

use proptest::prelude::*;

use super::*;
use crate::sound::{SoundContainerScene, SoundId, Take, Takes};

const CREAK: &str = "4f1d6a3e-90b2-4c7e-8d15-2a6f0c93b7e1";

/// The problems of `sounds` as `you.horns`'s `sounds.toml`.
fn problems(sounds: &str) -> Vec<String> {
    Pack::parse("you.horns", &PackFiles::new().with(SOUNDS_FILE, sounds))
        .unwrap_err()
        .list()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn has(lines: &[String], expected: &str) {
    assert!(
        lines.iter().any(|l| l.starts_with(expected)),
        "{expected}: {lines:?}"
    );
}

#[test]
fn a_sound_takes_the_pack_s_namespace_and_its_problems_their_line() {
    let good = PackFiles::new().with(
        SOUNDS_FILE,
        "[[sound]]\nid = \"horn\"\nfile = \"sounds/horn.wav\"\nscene = \"ui\"\nvolume = 0.5\nsleep = 0.25\n",
    );
    let pack = Pack::parse("you.horns", &good).unwrap();
    let [sound] = pack.sounds.sounds.as_slice() else {
        panic!("one sound");
    };
    assert_eq!(sound.id, "you.horns:horn");
    assert_eq!(sound.container.scene, Some(SoundContainerScene::Ui));
    assert_eq!(
        sound.container.sleep,
        Some(std::time::Duration::from_millis(250))
    );
    assert!(
        pack.reaches_players(),
        "a pack of sounds alone runs on both sides"
    );
    let lines = problems(
        "[[sound]]\nid = \"horn\"\nfile = \"../horn.wav\"\n\n[[sound]]\nid = \"horn\"\nfile = \"b.wav\"\nvolume = 5.0\n",
    );
    has(
        &lines,
        "sounds.toml:1:1: sound horn: \"../horn.wav\" is no path inside",
    );
    has(&lines, "sounds.toml:5:1: sound id \"horn\" is taken");
    has(&lines, "sounds.toml:5:1: sound horn: volume 5 is outside");
}

#[test]
fn a_sound_of_several_takes_keeps_their_chances_channels_gain_and_trim() {
    let files = PackFiles::new().with(
        SOUNDS_FILE,
        "[[sound]]\nid = \"steps\"\ntakes = [{ file = \"a.ogg\" }, { file = \"b.flac\", chance = 0.5 }]\nchannels = \"mono\"\ngain_db = -6.0\ntrim = { from = 0.5, to = 2.0 }\n",
    );
    let pack = Pack::parse("you.horns", &files).unwrap();
    let [sound] = pack.sounds.sounds.as_slice() else {
        panic!("one sound");
    };
    assert_eq!(
        sound.takes,
        vec![("a.ogg".to_owned(), None), ("b.flac".to_owned(), Some(0.5))]
    );
    assert_eq!(
        sound.channels,
        Some(super::super::sounds::ChannelsEntry::Mono)
    );
    assert!((sound.gain_db + 6.0).abs() < f32::EPSILON);
    assert_eq!(
        sound.trim,
        (
            std::time::Duration::from_millis(500),
            Some(std::time::Duration::from_secs(2))
        )
    );
    let lines = problems(
        "[[sound]]\nid = \"a\"\nfile = \"a.wav\"\ntakes = [{ file = \"b.wav\" }]\n\n\
         [[sound]]\nid = \"b\"\nfile = \"b.wav\"\ngain_db = 30.0\ntrim = { from = 2.0, to = 1.0 }\n\n\
         [[sound]]\nid = \"c\"\ntakes = []\n\n\
         [[sound]]\nid = \"d\"\ntakes = [{ file = \"d.wav\", chance = 2.0 }]\n",
    );
    has(
        &lines,
        "sounds.toml:1:1: sound a: give either `file` or `takes`",
    );
    has(&lines, "sounds.toml:6:1: sound b: gain 30 dB is outside");
    has(
        &lines,
        "sounds.toml:6:1: sound b: the trim ends before it starts",
    );
    has(
        &lines,
        "sounds.toml:12:1: sound c: 0 takes: a sound has 1 to 32",
    );
    has(
        &lines,
        "sounds.toml:16:1: sound d: d.wav: chance 2 is outside 0 to 1",
    );
}

#[test]
fn a_change_names_any_container_and_takes_the_pack_s_own_recordings() {
    let files = PackFiles::new().with(
        SOUNDS_FILE,
        format!(
            "[[change]]\nsound = \"{CREAK}\"\nmax_count = 2\ntakes = {{ add = [{{ sound = \"horn\", chance = 0.5 }}] }}\n\n\
             [[change]]\nsound = \"horn\"\nloop = true\n\n\
             [[sound]]\nid = \"horn\"\nfile = \"horn.wav\"\n"
        ),
    );
    let pack = Pack::parse("you.horns", &files).unwrap();
    let [creak, horn] = pack.sounds.changes.as_slice() else {
        panic!("two changes");
    };
    assert_eq!(creak.container, Guid::parse(CREAK));
    assert_eq!(creak.change.max_count, Some(2));
    assert_eq!(
        creak.change.takes,
        Some(Takes::Add(vec![{
            let mut value = Take::new(SoundId::from_guid(Guid::named("you.horns:horn.sound")));
            value.chance = 0.5;
            value
        }]))
    );
    assert_eq!(horn.container, Guid::named("you.horns:horn"));
    assert_eq!(horn.change.looped, Some(true));
}

#[test]
fn a_replaced_recording_is_the_game_s_by_its_guid() {
    let files = PackFiles::new().with(
        SOUNDS_FILE,
        format!("[[replace]]\nsound = \"{CREAK}\"\nfile = \"creak.wav\"\n"),
    );
    let pack = Pack::parse("you.horns", &files).unwrap();
    assert_eq!(pack.sounds.replaces[0].sound.guid(), Guid::parse(CREAK));
    assert!(pack.reaches_players());
    let lines = problems("[[replace]]\nsound = \"horn\"\nfile = \"/creak.wav\"\n");
    has(
        &lines,
        "sounds.toml:1:1: replacement of horn: \"/creak.wav\" is no path",
    );
    has(
        &lines,
        "sounds.toml:1:1: replacement of horn: a replaced recording is the game's",
    );
}

#[test]
fn a_change_of_an_unknown_sound_or_a_take_past_its_bound_is_named() {
    let lines = problems(
        "[[change]]\nsound = \"bell\"\n\n[[change]]\nsound = \"4f1d6a3e-90b2-4c7e-8d15-2a6f0c93b7e1\"\ntakes = { replace = [{ sound = \"you.bells:bell.sound\", chance = 2.0 }] }\n\n[[change]]\nsound = \"4f1d6a3e-90b2-4c7e-8d15-2a6f0c93b7e1\"\ntakes = { replace = [] }\n",
    );
    has(&lines, "sounds.toml:1:1: no sound \"bell\" in this pack");
    has(
        &lines,
        "sounds.toml:4:1: change of 4f1d6a3e-90b2-4c7e-8d15-2a6f0c93b7e1: chance 2 is outside",
    );
    has(
        &lines,
        "sounds.toml:8:1: change of 4f1d6a3e-90b2-4c7e-8d15-2a6f0c93b7e1: takes replaced by none",
    );
}

#[test]
fn a_voiced_line_and_music_take_the_pack_s_namespace_and_name_their_problems() {
    let good = PackFiles::new().with(
        SOUNDS_FILE,
        format!(
            "[[voiceover]]\nid = \"welcome\"\nlines = [{{ language = \"english\", file = \"en.ogg\" }}]\nfallback = \"english\"\nhas_position = false\n\n[[voiceover]]\nid = \"reading\"\nvoiceover = \"{CREAK}\"\nlines = [{{ language = \"german\", file = \"de.ogg\" }}]\n\n[[music]]\nid = \"night\"\nfiles = [\"a.ogg\", \"b.ogg\"]\nenvironment = \"Grassland\"\nstart_hour = 22.0\nend_hour = 6.5\nsleep = 30.0\n"
        ),
    );
    let pack = Pack::parse("you.horns", &good).unwrap();
    let [welcome, reading] = pack.sounds.voiceovers.as_slice() else {
        panic!("two lines");
    };
    assert_eq!(welcome.id, "you.horns:welcome");
    assert_eq!(welcome.voiceover, None);
    assert_eq!(
        reading.voiceover.map(|g| g.to_string()).as_deref(),
        Some(CREAK)
    );
    let [night] = pack.sounds.music.as_slice() else {
        panic!("one music");
    };
    assert_eq!(night.id, "you.horns:night");
    assert_eq!(night.start, Some(std::time::Duration::from_hours(22)));
    assert_eq!(night.end, Some(std::time::Duration::from_mins(390)));
    assert_eq!(night.sleep, std::time::Duration::from_secs(30));
    assert!(pack.reaches_players());
    let lines = problems(
        "[[voiceover]]\nid = \"a\"\nvoiceover = \"bell\"\nlines = [{ language = \"english\", file = \"../a.ogg\" }]\n\n[[voiceover]]\nid = \"b\"\nlines = []\n\n[[music]]\nid = \"c\"\nfiles = []\nend_hour = 25.0\nvolume = 2.0\n",
    );
    has(
        &lines,
        "sounds.toml:1:1: voiceover a: \"../a.ogg\" is no path inside",
    );
    has(&lines, "sounds.toml:1:1: voiceover a: \"bell\" is no GUID");
    has(
        &lines,
        "sounds.toml:6:1: voiceover b: give a file for a language",
    );
    has(&lines, "sounds.toml:10:1: music c: 0 files");
    has(&lines, "sounds.toml:10:1: music c: volume 2 is outside");
    has(&lines, "sounds.toml:10:1: music c: end_hour 25 is outside");
}

proptest! {
    /// Any volume, sleep and count read as a change or name their bound,
    /// as the sound module's own check words it.
    #[test]
    fn any_values_of_a_change_read_or_name_their_bound(
        volume in prop::num::f32::ANY,
        sleep in prop::num::f32::ANY,
        max_count in any::<u32>(),
    ) {
        let text = format!(
            "[[change]]\nsound = \"{CREAK}\"\nvolume = {volume:?}\nsleep = {sleep:?}\nmax_count = {max_count}\n"
        );
        let text = text.replace("NaN", "nan");
        let parsed = Pack::parse("you.horns", &PackFiles::new().with(SOUNDS_FILE, text));
        let within = (0.0..=crate::sound::MAX_VOLUME).contains(&volume)
            && (0.0..=600.0).contains(&sleep)
            && max_count <= crate::sound::MAX_COUNT;
        prop_assert_eq!(parsed.is_ok(), within, "{:?}", parsed.err().map(|e| e.list().iter().map(ToString::to_string).collect::<Vec<_>>()));
    }
}
