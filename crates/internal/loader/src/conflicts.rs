//! Where mods meet, and game code a mod changed outside the API.
//!
//! Mods meet on game functions several hook, code patches that overlap,
//! features several filter or one holds, resources several patch, and
//! vanilla values and components several write.
//!
//! The report names who, in which order, and whether they chain or one holds
//! it; the status summary and `enshwick conflicts` show it.

use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt::Write as _;
use std::sync::Mutex;

use enshwick::abi::Status;
use rustc_hash::FxHashSet;

use crate::code_watch::CodeChange;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// How the mods on one feature share it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Every mod's callback runs, in load order; each may refuse or change.
    Chained,
    /// Callbacks run in load order until one answers.
    FirstAnswers,
    /// One mod holds it; others are refused.
    Exclusive,
}

impl Mode {
    /// How the mods share it, in words.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Chained => "chained in load order, each may refuse or change",
            Self::FirstAnswers => "asked in load order, the first answer decides",
            Self::Exclusive => "one mod holds it",
        }
    }
}

/// A mod's claim on a feature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    /// The feature, such as `interaction filter` or `rename filter`.
    pub feature: Cow<'static, str>,
    /// The mod's index.
    pub owner: ModIndex,
    /// How the feature is shared.
    pub mode: Mode,
    /// Whether the loader granted it.
    pub granted: bool,
}

/// What the loader found where mods meet.
#[derive(Debug)]
struct Found {
    claims: Vec<Claim>,
    /// Where `.text` starts from the image base, and the code changed
    /// outside the API.
    code: (usize, Vec<CodeChange>),
    /// The vanilla values one mod's set overrides another's, in words.
    fields: Vec<String>,
    /// The components each mod wrote, by type hash.
    writes: Vec<(u32, ModIndex)>,
}

static FOUND: Mutex<Found> = Mutex::new(Found {
    claims: Vec::new(),
    code: (0, Vec::new()),
    fields: Vec::new(),
    writes: Vec::new(),
});

thread_local! {
    /// The component writes this thread has already reported, so a repeated
    /// write takes no lock.
    static WRITTEN: RefCell<FxHashSet<(u32, ModIndex)>> = RefCell::default();
}

/// Records mod `owner`'s claim on `feature` and passes the loader's answer
/// on; only grants and refusals count.
pub fn note(
    feature: impl Into<Cow<'static, str>>,
    owner: ModIndex,
    mode: Mode,
    status: Status,
) -> Status {
    if status.is_ok() || status == Status::REFUSED {
        FOUND.relock().claims.push(Claim {
            feature: feature.into(),
            owner,
            mode,
            granted: status.is_ok(),
        });
    }
    status
}

/// Every claim, in order.
#[must_use]
pub fn claims() -> Vec<Claim> {
    FOUND.relock().claims.clone()
}

/// Keeps the game code changed outside the API, found while the loader
/// started, with where `.text` starts from the image base.
pub fn set_code_changes(text_rva: usize, changes: Vec<CodeChange>) {
    FOUND.relock().code = (text_rva, changes);
}

/// The game code changed outside the API, as [`set_code_changes`] kept it.
#[must_use]
pub fn code_changes() -> (usize, Vec<CodeChange>) {
    FOUND.relock().code.clone()
}

/// Replaces the lines on vanilla values one mod's set overrides another's,
/// after each merge of the declared changes.
pub fn set_fields(lines: Vec<String>) {
    FOUND.relock().fields = lines;
}

/// The lines on vanilla values one mod's set overrides another's.
#[must_use]
pub fn fields() -> Vec<String> {
    FOUND.relock().fields.clone()
}

/// Notes that mod `owner` wrote a component of type `hash`; a write this
/// thread noted before costs a lookup and no lock, and a thread shutting
/// down notes nothing.
pub fn note_write(hash: u32, owner: ModIndex) {
    let new = WRITTEN.try_with(|seen| seen.borrow_mut().insert((hash, owner)));
    if new == Ok(true) {
        let mut found = FOUND.relock();
        if !found.writes.contains(&(hash, owner)) {
            found.writes.push((hash, owner));
        }
    }
}

/// Every component type several mods write, with the writers in the order
/// they first wrote it.
#[must_use]
pub fn shared_writes() -> Vec<(u32, Vec<ModIndex>)> {
    let found = FOUND.relock();
    let mut shared: Vec<(u32, Vec<ModIndex>)> = Vec::new();
    for &(hash, owner) in &found.writes {
        match shared.iter_mut().find(|(h, _)| *h == hash) {
            Some((_, owners)) => owners.push(owner),
            None => shared.push((hash, vec![owner])),
        }
    }
    shared.retain(|(_, owners)| owners.len() >= 2);
    shared
}

/// The report's lines on components several mods write, with each type
/// named by `type_name` and each mod by `name`.
#[must_use]
pub fn write_report(
    shared: &[(u32, Vec<ModIndex>)],
    type_name: impl Fn(u32) -> String,
    name: impl Fn(ModIndex) -> String,
) -> Vec<String> {
    shared
        .iter()
        .map(|(hash, owners)| {
            let owners: Vec<String> = owners.iter().map(|o| name(*o)).collect();
            format!(
                "component {}: written by {}, the last write wins",
                type_name(*hash),
                owners.join(", ")
            )
        })
        .collect()
}

/// One link on a hooked function, for the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookLink {
    /// Who linked it: a mod id, or `enshwick`.
    pub owner: String,
    /// Lower runs earlier.
    pub priority: i32,
    /// Holds the function alone.
    pub exclusive: bool,
    /// Reads the game caller's frame.
    pub caller_frame: bool,
}

/// A hooked function as the report sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hooked {
    /// Its name.
    pub name: String,
    /// Its links in running order.
    pub links: Vec<HookLink>,
    /// Who was refused a link, and who held it exclusively, if anyone.
    pub refused: Vec<(String, Option<String>)>,
}

/// The conflict report, a line each: hooks first, then features, then
/// resources.
#[must_use]
pub fn report(
    hooks: &[Hooked],
    claims: &[Claim],
    resources: &[(String, Vec<String>)],
    name: impl Fn(ModIndex) -> String,
) -> Vec<String> {
    let mut lines = Vec::new();
    for hook in hooks {
        let owners: Vec<&str> = hook.links.iter().map(|l| l.owner.as_str()).collect();
        let distinct = {
            let mut o = owners.clone();
            o.sort_unstable();
            o.dedup();
            o.len()
        };
        if distinct < 2 && hook.refused.is_empty() {
            continue;
        }
        let order: Vec<String> = hook
            .links
            .iter()
            .map(|l| {
                let mut notes = vec![format!("priority {}", l.priority)];
                if l.exclusive {
                    notes.push("exclusive".to_owned());
                }
                if l.caller_frame {
                    notes.push("reads the caller frame".to_owned());
                }
                format!("{} ({})", l.owner, notes.join(", "))
            })
            .collect();
        let mut line = format!("hook {}: {}", hook.name, order.join(" then "));
        if distinct >= 2 {
            line.push_str(", chained: each link calls on or replaces the rest");
        }
        for (asked, holder) in &hook.refused {
            match holder {
                Some(holder) => {
                    let _w = write!(line, "; {asked} refused, {holder} holds it exclusively");
                }
                None => {
                    let _w = write!(line, "; {asked} refused a second link");
                }
            }
        }
        lines.push(line);
    }
    let mut features: Vec<&str> = claims.iter().map(|c| c.feature.as_ref()).collect();
    features.dedup();
    let mut seen: Vec<&str> = Vec::new();
    for feature in features {
        if seen.contains(&feature) {
            continue;
        }
        seen.push(feature);
        let on: Vec<&Claim> = claims.iter().filter(|c| c.feature == feature).collect();
        let owners = |granted: bool| {
            let mut owners: Vec<String> = Vec::new();
            for claim in on.iter().filter(|c| c.granted == granted) {
                let owner = name(claim.owner);
                if !owners.contains(&owner) {
                    owners.push(owner);
                }
            }
            owners
        };
        let granted = owners(true);
        let refused = owners(false);
        if granted.len() < 2 && refused.is_empty() {
            continue;
        }
        let mode = on.first().map_or(Mode::Chained, |c| c.mode);
        let mut line = match mode {
            Mode::Exclusive => format!(
                "{feature}: {} holds it",
                granted.first().map_or("no mod", String::as_str)
            ),
            _ => format!("{feature}: {}, {}", granted.join(", "), mode.describe()),
        };
        if !refused.is_empty() {
            let _w = write!(line, "; refused: {}", refused.join(", "));
        }
        lines.push(line);
    }
    for (key, owners) in resources {
        if owners.len() >= 2 {
            lines.push(format!(
                "{key}: patched by {}, each on the one before",
                owners.join(" then ")
            ));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(owner: &str, priority: i32) -> HookLink {
        HookLink {
            owner: owner.to_owned(),
            priority,
            exclusive: false,
            caller_frame: false,
        }
    }

    fn claim(feature: &'static str, owner: ModIndex, mode: Mode, granted: bool) -> Claim {
        Claim {
            feature: feature.into(),
            owner,
            mode,
            granted,
        }
    }

    fn names(index: ModIndex) -> String {
        ["you.trading-post", "you.market-stalls", "market"][index.get()].to_owned()
    }

    #[test]
    fn shared_hooks_are_listed_in_running_order() {
        let mut framed = link("you.market-stalls", 5);
        framed.caller_frame = true;
        let hooks = [
            Hooked {
                name: "building.place".into(),
                links: vec![link("you.trading-post", 0), framed],
                refused: Vec::new(),
            },
            Hooked {
                name: "alone".into(),
                links: vec![link("you.trading-post", 0)],
                refused: Vec::new(),
            },
            Hooked {
                name: "held".into(),
                links: vec![HookLink {
                    exclusive: true,
                    ..link("market", 0)
                }],
                refused: vec![("you.trading-post".into(), Some("market".into()))],
            },
        ];
        let lines = report(&hooks, &[], &[], names);
        assert_eq!(
            lines,
            [
                "hook building.place: you.trading-post (priority 0) then you.market-stalls (priority 5, reads the caller frame), chained: each link calls on or replaces the rest",
                "hook held: market (priority 0, exclusive); you.trading-post refused, market holds it exclusively",
            ]
        );
    }

    #[test]
    fn shared_and_held_features_are_named() {
        let claims = [
            claim("interaction filter", ModIndex::new(0), Mode::Chained, true),
            claim("interaction filter", ModIndex::new(1), Mode::Chained, true),
            claim("altar limit", ModIndex::new(2), Mode::Exclusive, true),
            claim("altar limit", ModIndex::new(0), Mode::Exclusive, false),
            claim("damage filter", ModIndex::new(1), Mode::Chained, true),
        ];
        let lines = report(&[], &claims, &[], names);
        assert_eq!(
            lines,
            [
                "interaction filter: you.trading-post, you.market-stalls, chained in load order, each may refuse or change",
                "altar limit: market holds it; refused: you.trading-post",
            ]
        );
    }

    #[test]
    fn resources_patched_by_several_mods_are_named() {
        let resources = [
            (
                "the item registry".to_owned(),
                vec![
                    "you.trading-post".to_owned(),
                    "you.market-stalls".to_owned(),
                ],
            ),
            ("one".to_owned(), vec!["you.trading-post".to_owned()]),
        ];
        let lines = report(&[], &[], &resources, names);
        assert_eq!(
            lines,
            [
                "the item registry: patched by you.trading-post then you.market-stalls, each on the one before"
            ]
        );
    }

    #[test]
    fn components_several_mods_write_are_named_once_per_writer() {
        let hash = 0x5eed_c0de;
        for owner in [0, 1, 0, 1, 2] {
            note_write(hash, ModIndex::new(owner));
        }
        note_write(0x0a10_0e00, ModIndex::new(0));
        let shared: Vec<_> = shared_writes()
            .into_iter()
            .filter(|(h, _)| *h == hash || *h == 0x0a10_0e00)
            .collect();
        assert_eq!(
            write_report(&shared, |h| format!("{h:#x}"), names),
            [
                "component 0x5eedc0de: written by you.trading-post, you.market-stalls, market, the last write wins"
            ]
        );
    }

    #[test]
    fn a_mod_refused_twice_on_one_patch_is_named_once() {
        let claims = [(2, true), (0, false), (2, true), (0, false)].map(|(owner, granted)| {
            claim(
                "code patch at 0x10",
                ModIndex::new(owner),
                Mode::Exclusive,
                granted,
            )
        });
        assert_eq!(
            report(&[], &claims, &[], names),
            ["code patch at 0x10: market holds it; refused: you.trading-post"]
        );
    }
}
