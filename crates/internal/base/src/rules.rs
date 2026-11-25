//! The rules the loader and the fake host both refuse a mod's requests by.

use crate::abi::{Event, HookFlags};

/// Whether `event` is one the loader raises, so a mod may subscribe to it: an
/// event past the last is one a newer loader raises.
#[must_use]
pub const fn is_loader_event(event: Event) -> bool {
    event.0 >= Event::SETTINGS_CHANGED.0 && event.0 <= Event::QUEST_PROGRESSED.0
}

/// Whether a prop or block can stand at `position`: every axis finite, at
/// 0 or more and below [`Voxel::END`](crate::abi::Voxel::END) metres, the
/// world's voxel range, which [`in_fixed_range`](crate::abi::in_fixed_range)
/// holds.
#[must_use]
pub fn in_voxel_range(position: [f64; 3]) -> bool {
    position
        .iter()
        .all(|axis| (0.0..f64::from(crate::abi::Voxel::END)).contains(axis))
}

/// Whether `min` and `max` bound a box an edit can fill: both corners
/// [`in_fixed_range`](crate::abi::in_fixed_range) and `max` nowhere below
/// `min`.
#[must_use]
pub fn is_edit_box(min: [f64; 3], max: [f64; 3]) -> bool {
    crate::abi::in_fixed_range(min)
        && crate::abi::in_fixed_range(max)
        && min.iter().zip(&max).all(|(low, high)| low <= high)
}

/// Whether a hook names both a target and a detour: without either it is a
/// bad request.
#[must_use]
pub fn is_complete_hook(target: u64, detour: usize) -> bool {
    target != 0 && detour != 0
}

/// Whether a new hook clashes with one a target holds: one mod links a
/// target once, and an exclusive link shares it with nobody.
#[must_use]
pub fn hooks_clash(same_mod: bool, held: HookFlags, new: HookFlags) -> bool {
    same_mod || held.contains(HookFlags::EXCLUSIVE) || new.contains(HookFlags::EXCLUSIVE)
}

/// Whether a code patch over `span` (address and length) patches at least
/// one byte and lies inside `text`, the game's code section.
#[must_use]
pub fn patch_fits((address, len): (usize, usize), text: (usize, usize)) -> bool {
    len != 0 && address >= text.0 && address.saturating_add(len) <= text.0.saturating_add(text.1)
}

/// Whether two spans of bytes, each an address and a length, share a byte.
#[must_use]
pub fn spans_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.0.saturating_add(b.1) && b.0 < a.0.saturating_add(a.1)
}

/// Whether a point's name is `<id>:<point>` under the defining mod's `id`.
#[must_use]
pub fn is_named_after(name: &str, id: &str) -> bool {
    name.split_once(':')
        .is_some_and(|(owner, what)| owner == id && !what.is_empty())
}

/// The first of `exports` that is not `<kind>:<local name>`, the kind one
/// of [`Kind::ALL`](crate::schema_section::Kind::ALL) and the local name 1
/// to 128 of `[a-z0-9_./-]`, with no mod id; `None` when every one is.
///
/// The loader refuses a manifest with such an export, and the fake does too.
#[must_use]
pub fn bad_export<S: AsRef<str>>(exports: &[S]) -> Option<&str> {
    exports.iter().map(AsRef::as_ref).find(|export| {
        crate::schema_section::Kind::of_export(export)
            .is_none_or(|(_, local)| !is_local_name(local))
    })
}

/// Whether `local` is a local name: 1 to 128 of `[a-z0-9_./-]`, with no mod
/// id.
#[must_use]
pub fn is_local_name(local: &str) -> bool {
    (1..=128).contains(&local.len())
        && local.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-')
        })
}

/// Why a mod may not use the `kind` `name`, `<owner>:<what>`, whose owner
/// exports `exports`: the loader's and the fake's one message, naming the
/// missing export with its kind.
#[must_use]
pub fn unexported_reason<S: AsRef<str>>(
    kind: crate::schema_section::Kind,
    name: &str,
    exports: &[S],
) -> String {
    let (owner, what) = name.split_once(':').unwrap_or((name, ""));
    let offered = if exports.is_empty() {
        "it exports nothing".to_owned()
    } else {
        let names: Vec<&str> = exports.iter().map(AsRef::as_ref).collect();
        format!("it exports {}", names.join(", "))
    };
    format!(
        "{} {name}: {owner} does not export {}:{what}; {offered}",
        kind.label(),
        kind.word()
    )
}

/// Whether a mod's `exports`, each `<kind>:<local name>`, let other mods use
/// its `kind` `name`.
///
/// `name` is a local name, exported when `kind:name` is, or, for `content`
/// and `stat`, the name it is a part of (`content:locker` for
/// `locker.recipe`).
#[must_use]
pub fn exports_name<S: AsRef<str>>(
    exports: &[S],
    kind: crate::schema_section::Kind,
    name: &str,
) -> bool {
    let exported = |local: &str| {
        exports
            .iter()
            .any(|e| crate::schema_section::Kind::of_export(e.as_ref()) == Some((kind, local)))
    };
    exported(name)
        || (matches!(
            kind,
            crate::schema_section::Kind::Content | crate::schema_section::Kind::Stat
        ) && name.match_indices('.').any(|(at, _)| exported(&name[..at])))
}

/// The screens a client command opens, by their `HudScreenType` names: the
/// ones the game opens from a key in the world (client 26430625).
pub const CLIENT_OPENS: [&str; 10] = [
    "SystemMenu",
    "InGameMenu",
    "Map",
    "Journal",
    "Character",
    "PlayerCrafting",
    "Backpack",
    "SkillTree",
    "TextChat",
    "Social",
];

/// The in-game menu's tabs a client command chooses, in the menu's index
/// order (client 26430625, seen live).
pub const MENU_TABS: [&str; 7] = [
    "Social",
    "Map",
    "Journal",
    "PlayerCrafting",
    "Backpack",
    "Character",
    "SkillTree",
];

/// How many presses of game actions one mod holds at once, so no mod takes
/// every press the loader holds for all of them.
pub const PRESSES_PER_MOD: usize = 4;

/// The world's own screen: the one a close returns to and the only one a
/// look, a move or a press may expect.
pub const WORLD_SCREEN: &str = "InGame";

/// The longest action id.
pub const MAX_ACTION_ID: usize = 32;

/// Whether `id` is an action id: 1 to [`MAX_ACTION_ID`] bytes of
/// `[a-z0-9-]`.
#[must_use]
pub fn is_action_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ACTION_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The id of a carried window's Close button.
pub const WINDOW_CLOSE: &str = "@close";
/// The id of a carried window's form's Cancel button.
pub const WINDOW_CANCEL: &str = "@cancel";
/// The id of a carried window's map button, which opens the game's map.
pub const WINDOW_MAP: &str = "@map";

/// An item of a carried window as its id names it, from the action ids
/// its mod declared.
///
/// Action ids hold only `[a-z0-9-]`, so the window's own buttons, which
/// start with `@`, and the separators `/`, `@` and `#` never clash with
/// one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowItem {
    /// `<action>`: the button that runs the action, drawn whole or in its
    /// form.
    Run(String),
    /// `<action>/<arg>`: the field of the action's argument, by its place
    /// from 0.
    Field(String, usize),
    /// `<action>@<key>`: a list's switch for the player of this account.
    Switch(String, crate::PlayerKey),
    /// `<action>#<n>`: the window's `n`th row button that runs the action,
    /// from 0.
    RowButton(String, usize),
    /// The window's Close, [`WINDOW_CLOSE`].
    Close,
    /// A form's Cancel, [`WINDOW_CANCEL`].
    Cancel,
    /// A map's button, [`WINDOW_MAP`].
    Map,
}

impl WindowItem {
    /// The item `id` names; `None` for one no window can hold.
    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        let action = |action: &str| is_action_id(action).then(|| action.to_owned());
        let number = |text: &str| -> Option<u64> {
            (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
                .then(|| text.parse().ok())
                .flatten()
        };
        let place = |text: &str| number(text).and_then(|n| usize::try_from(n).ok());
        match id {
            WINDOW_CLOSE => return Some(Self::Close),
            WINDOW_CANCEL => return Some(Self::Cancel),
            WINDOW_MAP => return Some(Self::Map),
            _ => {}
        }
        if let Some((name, arg)) = id.split_once('/') {
            return Some(Self::Field(action(name)?, place(arg)?));
        }
        if let Some((name, key)) = id.split_once('@') {
            return Some(Self::Switch(
                action(name)?,
                crate::PlayerKey::new(number(key)?),
            ));
        }
        if let Some((name, n)) = id.split_once('#') {
            return Some(Self::RowButton(action(name)?, place(n)?));
        }
        action(id).map(Self::Run)
    }

    /// The id that names this item.
    #[must_use]
    pub fn id(&self) -> String {
        match self {
            Self::Run(action) => action.clone(),
            Self::Field(action, arg) => format!("{action}/{arg}"),
            Self::Switch(action, key) => format!("{action}@{}", key.0),
            Self::RowButton(action, n) => format!("{action}#{n}"),
            Self::Close => WINDOW_CLOSE.to_owned(),
            Self::Cancel => WINDOW_CANCEL.to_owned(),
            Self::Map => WINDOW_MAP.to_owned(),
        }
    }
}

/// A carried window's `mod:window` name split into the mod's id and the
/// window's; `None` without a mod or with a window id that is no action
/// id's shape.
#[must_use]
pub fn window_name(window: &str) -> Option<(&str, &str)> {
    let (mod_id, id) = window.rsplit_once(':')?;
    (!mod_id.is_empty() && is_action_id(id)).then_some((mod_id, id))
}

/// Whether a window command of raw kind `kind` names a window, an item and
/// a `text` the loader takes: a pick names its option.
#[must_use]
pub fn window_command_names(kind: u32, window: &str, item: &str, text: &str) -> bool {
    use crate::abi::ClientCommand as Spec;
    window_name(window).is_some()
        && WindowItem::parse(item).is_some()
        && (kind != Spec::WINDOW_PICK || !text.is_empty())
}

/// Whether a client command of raw kind `kind` names a `target` and an
/// `expected` screen the loader takes.
///
/// An open takes a screen in [`CLIENT_OPENS`], a tab one in [`MENU_TABS`],
/// and a look, move or press expects only [`WORLD_SCREEN`] or nothing.
#[must_use]
pub fn client_command_names(kind: u32, target: &str, expected: &str) -> bool {
    use crate::abi::ClientCommand as Spec;
    match kind {
        Spec::OPEN_SCREEN => CLIENT_OPENS.contains(&target),
        Spec::CHOOSE_TAB => MENU_TABS.contains(&target),
        Spec::LOOK | Spec::MOVE | Spec::PRESS => expected.is_empty() || expected == WORLD_SCREEN,
        Spec::JOIN => join_target(target).is_some() && expected.is_empty(),
        _ => true,
    }
}

/// A join's server `target` as its host and query port: `host:port`, the
/// host a name or an address without spaces, the port above 0; `None` for
/// anything else.
#[must_use]
pub fn join_target(target: &str) -> Option<(&str, u16)> {
    let (host, port) = target.rsplit_once(':')?;
    let port = port.parse::<u16>().ok().filter(|&p| p != 0)?;
    (!host.is_empty() && !host.contains(char::is_whitespace)).then_some((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_command_names_only_the_screens_and_tabs_the_loader_takes() {
        use crate::abi::ClientCommand as Spec;
        assert!(client_command_names(Spec::OPEN_SCREEN, "Backpack", ""));
        assert!(!client_command_names(Spec::OPEN_SCREEN, "StorageChest", ""));
        assert!(client_command_names(Spec::CHOOSE_TAB, "SkillTree", ""));
        assert!(!client_command_names(Spec::CHOOSE_TAB, "Inventory", ""));
        assert!(client_command_names(Spec::LOOK, "", WORLD_SCREEN));
        assert!(!client_command_names(Spec::MOVE, "", "Backpack"));
        assert!(client_command_names(Spec::PRESS, "Jump", ""));
        assert!(!client_command_names(Spec::PRESS, "Jump", "Backpack"));
        assert!(client_command_names(Spec::JOIN, "127.0.0.1:15637", ""));
        assert!(!client_command_names(
            Spec::JOIN,
            "127.0.0.1:15637",
            "InGame"
        ));
    }

    #[test]
    fn every_window_item_id_reads_back_as_the_item_it_names() {
        let items = [
            WindowItem::Run("ban".into()),
            WindowItem::Field("ban".into(), 1),
            WindowItem::Switch("friends".into(), crate::PlayerKey::new(76_561_198)),
            WindowItem::RowButton("ban".into(), 2),
            WindowItem::Close,
            WindowItem::Cancel,
            WindowItem::Map,
        ];
        for item in items {
            assert_eq!(WindowItem::parse(&item.id()), Some(item));
        }
    }

    #[test]
    fn an_id_no_action_can_have_names_no_window_item() {
        for id in [
            "", "Ban", "ban/", "ban/x", "ban#-1", "@open", "ban@", "a b", "ban/1/2",
        ] {
            assert_eq!(WindowItem::parse(id), None, "{id:?}");
        }
        assert_eq!(
            WindowItem::parse(&"a".repeat(33)),
            None,
            "past MAX_ACTION_ID"
        );
    }

    #[test]
    fn a_window_is_named_by_its_mod_and_its_own_id() {
        use crate::abi::ClientCommand as Spec;
        assert_eq!(
            window_name("you.windows-check:check"),
            Some(("you.windows-check", "check"))
        );
        for bad in ["check", ":check", "m:Check", "m:"] {
            assert_eq!(window_name(bad), None, "{bad}");
        }
        assert!(window_command_names(
            Spec::WINDOW_PRESS,
            "m:w",
            "@close",
            ""
        ));
        assert!(!window_command_names(Spec::WINDOW_PICK, "m:w", "ban/0", ""));
        assert!(window_command_names(
            Spec::WINDOW_PICK,
            "m:w",
            "ban/0",
            "Bob"
        ));
        assert!(!window_command_names(Spec::WINDOW_TEXT, "m:w", "Ban", "x"));
    }

    #[test]
    fn a_join_target_is_a_host_and_a_port() {
        assert_eq!(join_target("127.0.0.1:15637"), Some(("127.0.0.1", 15637)));
        assert_eq!(
            join_target("play.example.net:15637"),
            Some(("play.example.net", 15637))
        );
        for bad in ["127.0.0.1", ":15637", "a:0", "a:b", "a b:1", "a:70000"] {
            assert_eq!(join_target(bad), None, "{bad}");
        }
    }
    use crate::schema_section::Kind;

    #[test]
    fn a_hook_needs_a_target_and_a_detour() {
        assert!(is_complete_hook(1, 1));
        assert!(!is_complete_hook(0, 1));
        assert!(!is_complete_hook(1, 0));
    }

    #[test]
    fn a_target_takes_one_link_per_mod_and_none_beside_an_exclusive_one() {
        let none = HookFlags::NONE;
        assert!(!hooks_clash(false, none, none));
        assert!(hooks_clash(true, none, none));
        assert!(hooks_clash(false, HookFlags::EXCLUSIVE, none));
        assert!(hooks_clash(false, none, HookFlags::EXCLUSIVE));
    }

    #[test]
    fn a_patch_fits_only_inside_the_code_and_with_bytes() {
        let text = (100, 10);
        assert!(patch_fits((100, 10), text));
        assert!(patch_fits((104, 2), text));
        assert!(!patch_fits((104, 0), text));
        assert!(!patch_fits((99, 2), text));
        assert!(!patch_fits((105, 6), text));
        assert!(!patch_fits((usize::MAX, 2), text));
    }

    #[test]
    fn spans_overlap_only_when_they_share_a_byte() {
        assert!(spans_overlap((0, 4), (2, 4)));
        assert!(spans_overlap((2, 4), (0, 4)));
        assert!(!spans_overlap((0, 4), (4, 4)));
        assert!(!spans_overlap((0, 0), (0, 4)));
    }

    #[test]
    fn a_name_is_under_its_mod_s_id() {
        assert!(is_named_after("you.mod:limit", "you.mod"));
        assert!(!is_named_after("you.mod:", "you.mod"));
        assert!(!is_named_after("other.mod:limit", "you.mod"));
        assert!(!is_named_after("limit", "you.mod"));
    }

    #[test]
    fn an_exported_content_name_exports_its_parts_too() {
        let exports = ["content:locker", "content:bench.recipe"];
        let content = |name| exports_name(&exports, Kind::Content, name);
        assert!(content("locker"));
        assert!(content("locker.recipe"));
        assert!(content("locker.icon.png"));
        assert!(content("bench.recipe"));
        assert!(!content("bench"));
        assert!(!content("bench.name"));
        assert!(!content("lockers"));
        assert!(!content("locker-key"));
        assert!(!exports_name::<&str>(&[], Kind::Content, "locker"));
    }

    #[test]
    fn an_export_without_a_known_kind_or_a_local_name_is_bad() {
        assert_eq!(bad_export(&["point:price", "content:a.b/c-d_e"]), None);
        for bad in [
            "price",
            "greeting:hi",
            "point:",
            "point:you.a:price",
            "content:Big",
        ] {
            assert_eq!(bad_export(&["event:sold", bad]), Some(bad), "{bad}");
        }
        let long = format!("content:{}", "a".repeat(129));
        assert_eq!(bad_export(&[long.as_str()]), Some(long.as_str()));
    }

    #[test]
    fn a_prop_or_block_stands_from_0_to_below_the_voxel_end_on_each_axis() {
        let end = f64::from(crate::abi::Voxel::END);
        assert!(in_voxel_range([0.0, end - 0.5, 12.25]));
        for out in [
            [-0.5, 0.0, 0.0],
            [0.0, end, 0.0],
            [0.0, 0.0, 1e300],
            [f64::NAN, 0.0, 0.0],
            [f64::INFINITY, 0.0, 0.0],
        ] {
            assert!(!in_voxel_range(out), "{out:?}");
        }
        assert!(
            crate::abi::in_fixed_range([end; 3]),
            "inside the fixed point"
        );
    }

    #[test]
    fn an_edit_box_has_both_corners_in_range_and_max_nowhere_below_min() {
        assert!(is_edit_box([0.0; 3], [0.0; 3]));
        assert!(is_edit_box([-2e9, 0.0, 1.0], [2e9, 0.0, 2.0]));
        for (min, max) in [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 1.0]),
            ([f64::NAN, 0.0, 0.0], [1.0; 3]),
            ([0.0; 3], [1e300, 1.0, 1.0]),
            ([-2_147_483_648.0, 0.0, 0.0], [1.0; 3]),
        ] {
            assert!(!is_edit_box(min, max), "{min:?} {max:?}");
        }
    }

    #[test]
    fn an_export_opens_only_its_own_kind() {
        let exports = ["point:price", "event:sold"];
        assert!(exports_name(&exports, Kind::Point, "price"));
        assert!(!exports_name(&exports, Kind::Content, "price"));
        assert!(!exports_name(&exports, Kind::Stat, "price"));
        assert!(!exports_name(&exports, Kind::Event, "price"));
        assert!(!exports_name(&exports, Kind::Call, "sold"));
        assert!(!exports_name(&exports, Kind::Point, "price.part"));
    }
}
