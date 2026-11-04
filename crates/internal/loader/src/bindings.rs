//! One player's keys for mod controls: `mod-controls.toml` in the game's
//! folder, client only, holding only what the player changed; the clashes
//! the controls page shows; and how the page captures a new key.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use enshwick::controls::Key;

/// The file's name in the game's folder.
pub const FILE: &str = "mod-controls.toml";

/// The two key slots of a control: the key and the second key.
pub const SLOTS: [&str; 2] = ["key", "second"];

/// What the file sets for one control, slot by slot: `None` keeps the
/// mod's default, `Some(0)` unbinds.
pub type Override = [Option<u32>; 2];

/// The file's overrides by `<mod id>:<control>`, and a line for each entry
/// it could not read.
///
/// # Examples
///
/// ```
/// use enshwick_loader::bindings::parse;
///
/// let (overrides, problems) = parse("[\"you.cars:horn\"]\nkey = \"Shift+J\"\nsecond = 0\n");
/// assert!(problems.is_empty());
/// assert_eq!(overrides["you.cars:horn"][1], Some(0));
/// ```
#[must_use]
pub fn parse(text: &str) -> (BTreeMap<String, Override>, Vec<String>) {
    let mut overrides = BTreeMap::new();
    let mut problems = Vec::new();
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(why) => {
            problems.push(format!("{FILE} does not read: {}", why.message()));
            return (overrides, problems);
        }
    };
    for (control, value) in table {
        let Some(slots) = value.as_table() else {
            problems.push(format!("{control}: not a table of keys"));
            continue;
        };
        let mut set: Override = [None, None];
        for (slot, value) in slots {
            let Some(at) = SLOTS.iter().position(|s| s == slot) else {
                problems.push(format!("{control}: no slot {slot}"));
                continue;
            };
            let bits = match value {
                toml::Value::Integer(0) => Some(0),
                toml::Value::Integer(bits) => u32::try_from(*bits)
                    .ok()
                    .and_then(Key::from_bits)
                    .map(Key::bits),
                toml::Value::String(name) => Key::parse(name).ok().map(Key::bits),
                _ => None,
            };
            match bits {
                Some(bits) => set[at] = Some(bits),
                None => problems.push(format!("{control}.{slot}: {value} is no key")),
            }
        }
        overrides.insert(control, set);
    }
    (overrides, problems)
}

/// The file for `entries`, each `<mod id>:<control>` with the keys in force
/// where they differ from the mod's defaults: scan codes, with the key's US
/// name beside each. `kept` are the file's entries for controls no running
/// mod declares, written back as they were, read or not.
///
/// # Examples
///
/// ```
/// use enshwick_loader::bindings::{parse, render};
///
/// let kept: toml::Table = "[\"you.boats:horn\"]\nkey = \"Hyper+X\"\n".parse()?;
/// let text = render(&[("you.cars:horn".to_owned(), [Some(0x24), Some(0)])], &kept);
/// assert!(text.contains("key = 36 # J"));
/// assert!(text.contains("Hyper+X"));
/// assert_eq!(parse(&text).0["you.cars:horn"], [Some(0x24), Some(0)]);
/// # Ok::<(), toml::de::Error>(())
/// ```
#[must_use]
pub fn render(entries: &[(String, Override)], kept: &toml::Table) -> String {
    let mut text = String::from(
        "# The keys of mod controls, written by the Mods screen. Each is a scan code\n\
         # with its US key name beside it; a name such as \"Shift+E\" works too, and\n\
         # 0 unbinds. A control not listed keeps its mod's keys.\n",
    );
    if !kept.is_empty() {
        let _ = write!(text, "\n{kept}");
    }
    for (control, slots) in entries {
        if slots.iter().all(Option::is_none) {
            continue;
        }
        let name = toml::Value::String(control.clone());
        let _ = write!(text, "\n[{name}]\n");
        for (slot, bits) in SLOTS.iter().zip(slots) {
            let Some(bits) = bits else {
                continue;
            };
            let shown = Key::from_bits(*bits).map_or_else(|| "none".to_owned(), |k| k.to_string());
            let _ = writeln!(text, "{slot} = {bits} # {shown}");
        }
    }
    text
}

/// For each control's keys, the other controls that share one of them: the
/// same key with the same modifiers.
///
/// # Examples
///
/// ```
/// use enshwick_loader::bindings::clashes;
///
/// assert_eq!(clashes(&[[0x23, 0], [0x24, 0x23], [0x25, 0]]), [vec![1], vec![0], vec![]]);
/// ```
#[must_use]
pub fn clashes(keys: &[[u32; 2]]) -> Vec<Vec<usize>> {
    keys.iter()
        .enumerate()
        .map(|(i, mine)| {
            keys.iter()
                .enumerate()
                .filter(|&(j, theirs)| {
                    j != i && mine.iter().any(|&bits| bits != 0 && theirs.contains(&bits))
                })
                .map(|(j, _)| j)
                .collect()
        })
        .collect()
}

/// The left mouse button's code, which a capture passes over.
const LEFT_MOUSE: u32 = 0x200;

/// What a capture ended with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Captured {
    /// The player pressed this key, with the modifiers held.
    Key(Key),
    /// The player pressed Escape.
    Cancelled,
}

/// The controls page waiting for a key: it counts only a key that goes
/// down after the capture began, so the click that began it does not bind
/// the mouse button.
#[derive(Clone, Debug)]
pub struct Capture {
    was: Vec<bool>,
    escape_was: bool,
}

impl Capture {
    /// A capture that begins with the keys `down`, in the order of the
    /// keys a control may take, and Escape as `escape`.
    #[must_use]
    pub fn new(down: &[bool], escape: bool) -> Self {
        Self {
            was: down.to_vec(),
            escape_was: escape,
        }
    }

    /// One look at the keys: `down` in the order the capture began with,
    /// their `keys`, the modifiers held and whether Escape is down. A key
    /// that went down ends the capture with it; Escape cancels; a
    /// combination nobody may bind, such as Alt+F4, is passed over, and so
    /// is the left mouse button, which works the page itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::Key;
    /// use enshwick_loader::bindings::{Capture, Captured};
    ///
    /// let keys = [Key::parse("Mouse1")?, Key::parse("G")?];
    /// let mut capture = Capture::new(&[true, false], false);
    /// assert_eq!(capture.look(&[true, false], &keys, 0, false), None);
    /// let shift = Key::parse("Shift+G")?.modifiers();
    /// assert_eq!(
    ///     capture.look(&[true, true], &keys, shift, false),
    ///     Some(Captured::Key(Key::parse("Shift+G")?))
    /// );
    /// # Ok::<(), enshwick::controls::ParseKeyError>(())
    /// ```
    pub fn look(
        &mut self,
        down: &[bool],
        keys: &[Key],
        modifiers: u32,
        escape: bool,
    ) -> Option<Captured> {
        let escaped = escape && !self.escape_was;
        self.escape_was = escape;
        if escaped {
            return Some(Captured::Cancelled);
        }
        let pressed = down
            .iter()
            .zip(&self.was)
            .zip(keys)
            .filter(|((now, was), _)| **now && !**was)
            .filter(|(_, key)| key.code() != LEFT_MOUSE)
            .find_map(|(_, key)| Key::from_bits(key.code() | modifiers));
        self.was.clear();
        self.was.extend_from_slice(down);
        pressed.map(Captured::Key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> Key {
        Key::parse(name).unwrap()
    }

    #[test]
    fn unknown_slots_and_values_are_named_and_the_rest_reads() {
        let (overrides, problems) = parse(
            "[\"you.cars:horn\"]\nkey = 35\nthird = 1\n[\"you.cars:boost\"]\nsecond = \"Escape\"\n",
        );
        assert_eq!(overrides["you.cars:horn"], [Some(0x23), None]);
        assert_eq!(overrides["you.cars:boost"], [None, None]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        let (none, broken) = parse("not toml [");
        assert!(none.is_empty());
        assert_eq!(broken.len(), 1);
    }

    #[test]
    fn a_rendered_file_reads_back_and_leaves_out_untouched_controls() {
        let entries = [
            (
                "you.cars:horn".to_owned(),
                [Some(key("Ctrl+H").bits()), None],
            ),
            ("you.cars:boost".to_owned(), [None, None]),
            (
                "you.lights:on".to_owned(),
                [Some(0), Some(key("Mouse4").bits())],
            ),
        ];
        let text = render(&entries, &toml::Table::new());
        assert!(!text.contains("boost"), "{text}");
        assert!(text.contains("# Ctrl+H"), "{text}");
        let (read, problems) = parse(&text);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(read.len(), 2);
        assert_eq!(read["you.lights:on"], entries[2].1);
        assert_eq!(read["you.cars:horn"], entries[0].1);
    }

    #[test]
    fn another_mod_s_entries_survive_a_write_even_when_they_do_not_read() {
        let file = "stray = 1\n[\"you.boats:horn\"]\nkey = \"Hyper+X\"\nsecond = 37\n\
                    [\"you.lights:on\"]\nkey = 35\n";
        let kept: toml::Table = file.parse().unwrap();
        let mine = [("you.cars:horn".to_owned(), [Some(key("J").bits()), None])];
        let text = render(&mine, &kept);
        let written: toml::Table = text.parse().unwrap();
        assert_eq!(written["you.boats:horn"], kept["you.boats:horn"], "{text}");
        assert_eq!(written["you.lights:on"], kept["you.lights:on"]);
        assert_eq!(written["stray"], kept["stray"]);
        assert_eq!(parse(&text).0["you.cars:horn"], mine[0].1);
    }

    #[test]
    fn controls_clash_on_the_same_chord_only() {
        let keys = [
            [key("E").bits(), 0],
            [key("Shift+E").bits(), 0],
            [0, key("E").bits()],
            [0, 0],
        ];
        assert_eq!(clashes(&keys), [vec![2], vec![], vec![0], vec![]]);
    }

    #[test]
    fn a_key_written_by_its_name_reads_as_its_scan_code() {
        let (overrides, problems) = parse("[\"you.cars:horn\"]\nkey = \"Ctrl+J\"\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            overrides["you.cars:horn"],
            [Some(key("Ctrl+J").bits()), None]
        );
    }

    #[test]
    fn a_capture_binds_the_new_key_with_the_modifiers_held() {
        let keys = [key("Mouse1"), key("G")];
        let mut capture = Capture::new(&[true, false], false);
        let shift = key("Shift+G").modifiers();
        assert_eq!(
            capture.look(&[true, true], &keys, shift, false),
            Some(Captured::Key(key("Shift+G")))
        );
        let mut plain = Capture::new(&[false, false], false);
        assert_eq!(
            plain.look(&[true, false], &keys, 0, false),
            None,
            "a click on the page binds nothing"
        );
        assert_eq!(
            plain.look(&[true, true], &keys, 0, false),
            Some(Captured::Key(key("G")))
        );
    }

    #[test]
    fn escape_cancels_and_a_key_held_since_the_start_never_binds() {
        let keys = [key("Mouse1"), key("F4")];
        let mut capture = Capture::new(&[true, false], true);
        assert_eq!(capture.look(&[true, false], &keys, 0, true), None);
        assert_eq!(capture.look(&[false, false], &keys, 0, false), None);
        let alt = key("Alt+F5").modifiers();
        assert_eq!(capture.look(&[false, true], &keys, alt, false), None);
        assert_eq!(
            capture.look(&[false, false], &keys, 0, true),
            Some(Captured::Cancelled)
        );
    }
}
