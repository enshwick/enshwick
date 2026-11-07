//! `sounds.toml`: a pack's sounds read, named in its namespace and checked.

use super::super::sounds::SceneEntry;
use super::*;

#[test]
fn a_sound_takes_the_pack_s_namespace_and_its_problems_their_line() {
    let good = PackFiles::new().with(
        SOUNDS_FILE,
        "[[sound]]\nid = \"horn\"\nfile = \"sounds/horn.wav\"\nscene = \"ui\"\nvolume = 0.5\n",
    );
    let pack = Pack::parse("you.horns", &good).unwrap();
    assert_eq!(pack.sounds.len(), 1);
    assert_eq!(pack.sounds[0].id, "you.horns:horn");
    assert_eq!(pack.sounds[0].scene, Some(SceneEntry::Ui));
    assert!(
        pack.reaches_players(),
        "a pack of sounds alone runs on both sides"
    );
    let bad = PackFiles::new().with(
        SOUNDS_FILE,
        "[[sound]]\nid = \"horn\"\nfile = \"../horn.wav\"\n\n[[sound]]\nid = \"horn\"\nfile = \"b.wav\"\nvolume = 5.0\n",
    );
    let lines: Vec<String> = Pack::parse("you.horns", &bad)
        .unwrap_err()
        .list()
        .iter()
        .map(ToString::to_string)
        .collect();
    for expected in [
        "sounds.toml:1:1: sound horn: \"../horn.wav\" is no path inside",
        "sounds.toml:5:1: sound id \"horn\" is taken",
        "sounds.toml:5:1: sound horn: volume 5 is outside 0 to 4",
    ] {
        assert!(
            lines.iter().any(|l| l.starts_with(expected)),
            "{expected}: {lines:?}"
        );
    }
}
