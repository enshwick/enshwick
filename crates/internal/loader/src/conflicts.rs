//! Where mods meet: game functions several mods hook, features several mods
//! filter or one mod holds, resources several mods patch. The report names
//! who, in which order, and whether they chain or one holds it; the status
//! summary and `enshwick conflicts` show it.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::sync::{Mutex, PoisonError};

use enshwick::abi::Status;

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
    pub owner: usize,
    /// How the feature is shared.
    pub mode: Mode,
    /// Whether the loader granted it.
    pub granted: bool,
}

static CLAIMS: Mutex<Vec<Claim>> = Mutex::new(Vec::new());

/// Records mod `owner`'s claim on `feature` and passes the loader's answer
/// on; only grants and refusals count.
pub fn note(
    feature: impl Into<Cow<'static, str>>,
    owner: usize,
    mode: Mode,
    status: Status,
) -> Status {
    if status.is_ok() || status == Status::REFUSED {
        CLAIMS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Claim {
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
    CLAIMS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
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
    name: impl Fn(usize) -> String,
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
        let granted: Vec<String> = on
            .iter()
            .filter(|c| c.granted)
            .map(|c| name(c.owner))
            .collect();
        let refused: Vec<String> = on
            .iter()
            .filter(|c| !c.granted)
            .map(|c| name(c.owner))
            .collect();
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

    fn claim(feature: &'static str, owner: usize, mode: Mode, granted: bool) -> Claim {
        Claim {
            feature: feature.into(),
            owner,
            mode,
            granted,
        }
    }

    fn names(index: usize) -> String {
        ["safes", "survivors", "market"][index].to_owned()
    }

    #[test]
    fn shared_hooks_are_listed_in_running_order() {
        let mut framed = link("survivors", 5);
        framed.caller_frame = true;
        let hooks = [
            Hooked {
                name: "building.place".into(),
                links: vec![link("safes", 0), framed],
                refused: Vec::new(),
            },
            Hooked {
                name: "alone".into(),
                links: vec![link("safes", 0)],
                refused: Vec::new(),
            },
            Hooked {
                name: "held".into(),
                links: vec![HookLink {
                    exclusive: true,
                    ..link("market", 0)
                }],
                refused: vec![("safes".into(), Some("market".into()))],
            },
        ];
        let lines = report(&hooks, &[], &[], names);
        assert_eq!(
            lines,
            [
                "hook building.place: safes (priority 0) then survivors (priority 5, reads the caller frame), chained: each link calls on or replaces the rest",
                "hook held: market (priority 0, exclusive); safes refused, market holds it exclusively",
            ]
        );
    }

    #[test]
    fn shared_and_held_features_are_named() {
        let claims = [
            claim("interaction filter", 0, Mode::Chained, true),
            claim("interaction filter", 1, Mode::Chained, true),
            claim("altar limit", 2, Mode::Exclusive, true),
            claim("altar limit", 0, Mode::Exclusive, false),
            claim("damage filter", 1, Mode::Chained, true),
        ];
        let lines = report(&[], &claims, &[], names);
        assert_eq!(
            lines,
            [
                "interaction filter: safes, survivors, chained in load order, each may refuse or change",
                "altar limit: market holds it; refused: safes",
            ]
        );
    }

    #[test]
    fn resources_patched_by_several_mods_are_named() {
        let resources = [
            (
                "the item registry".to_owned(),
                vec!["safes".to_owned(), "survivors".to_owned()],
            ),
            ("one".to_owned(), vec!["safes".to_owned()]),
        ];
        let lines = report(&[], &[], &resources, names);
        assert_eq!(
            lines,
            ["the item registry: patched by safes then survivors, each on the one before"]
        );
    }
}
