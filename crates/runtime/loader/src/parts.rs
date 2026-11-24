//! Every part of the loader, layer by layer, joined: the start seals and
//! links them, the server step and the players' joins and leaves call them,
//! and their build table ids make the loader's feature table, so
//! `enshwick-compat` can tell which features a build lacks. Ids built at run
//! time, such as `survivors.{name}`, only reach compat through these lists.

use std::sync::OnceLock;

use crate::engine::parts::{Part, Parts};

/// The layers, lowest first: the engine, then the joins' own links, which
/// the domains' links rely on (the server step's), then the domains in the
/// order of `domains.toml`.
pub(crate) static LAYERS: &[(&str, &[&Part])] = &[
    ("engine", crate::engine::PARTS),
    ("joins", &[&crate::events::PART, &crate::start::PART]),
    ("base", crate::base::PARTS),
    ("content", crate::content::PARTS),
    ("world", crate::world::PARTS),
    ("inventory", crate::inventory::PARTS),
    ("knowledge", crate::knowledge::PARTS),
    ("actions", crate::actions::PARTS),
    ("ui", crate::ui::PARTS),
    ("packs", crate::packs::PARTS),
];

/// Every part of the loader, gathered once for the process: a table of the
/// code, which no run changes.
static PARTS: OnceLock<Parts> = OnceLock::new();

/// Every part of the loader, gathered the first time it is asked for: each
/// loader, the dry run's included, walks this one.
pub(crate) fn parts() -> &'static Parts {
    PARTS.get_or_init(|| Parts::new(LAYERS))
}

/// Each part of the loader that names build table ids, with them, by name:
/// the loader's feature table.
#[must_use]
pub fn table_ids() -> Vec<(&'static str, &'static [&'static str])> {
    parts().table_ids().to_vec()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use enshwick_gamedata::{self as tables, Table};

    use super::{LAYERS, parts, table_ids};
    use crate::engine::parts::Stage;

    /// Entries the tables pin that no part of the loader names yet.
    const UNNAMED: &[&str] = &[
        "actor.state_in_shroud",
        "attribute.experience",
        "attribute.experience_needed_slot",
        "attribute.fuel",
        "attribute.fuel_max_slot",
        "attribute.level",
        "attribute.shroud",
        "attribute.shroud_max_slot",
        "inventory.skip_entity",
        "loot.request_labels",
        "loot.request_node",
        "loot.roll",
        "loot.roll_one",
        "loot.txn_context",
        "loot.txn_entity",
        "skill.airborne_architect",
        "skill.airborne_architect_cell",
        "ui_image.content",
        "ui_image.flags",
        "ui_image.format",
        "ui_image.height",
        "ui_image.levels",
        "ui_image.width",
    ];

    /// Every symbol and constant id of the embedded tables, without the
    /// expected bytes the tables keep beside a symbol.
    fn pinned() -> BTreeSet<String> {
        tables::kept()
            .unwrap()
            .iter()
            .map(|(file, text)| Table::parse(text).unwrap_or_else(|e| panic!("{file}: {e}")))
            .flat_map(|t| {
                let symbols = t.symbols().iter().map(|s| s.id().to_owned());
                let constants = t.constants().iter().map(|c| c.id().to_owned());
                symbols.chain(constants).collect::<Vec<_>>()
            })
            .filter(|id| id.rsplit('.').next() != Some("bytes"))
            .collect()
    }

    /// Every id the loader finds on a build: by an anchor's pattern on
    /// either side, or by an engine route; the loader never reads the kept
    /// tables, so they count for nothing here.
    fn anchored_or_routed() -> BTreeSet<String> {
        let anchored = [tables::Side::Server, tables::Side::Client]
            .into_iter()
            .flat_map(|side| {
                let anchors = tables::anchors(side).unwrap();
                let symbols = anchors.symbols().iter().map(|s| s.id().to_owned());
                let constants = anchors.constants().iter().map(|c| c.id().to_owned());
                symbols.chain(constants).collect::<Vec<_>>()
            });
        let routed = tables::Routes::embedded()
            .unwrap()
            .ids()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        anchored.chain(routed).collect()
    }

    fn listed() -> BTreeSet<&'static str> {
        table_ids()
            .into_iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }

    /// The path below `src` of the module of the part named `name`.
    fn module_of(name: &str) -> String {
        let part = parts()
            .iter()
            .find(|part| part.name == name)
            .unwrap_or_else(|| panic!("no part {name}"));
        let module = part
            .module
            .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
            .unwrap_or_else(|| panic!("{name} is outside the crate: {}", part.module));
        module.replace("::", "/")
    }

    /// The code of the module of row `name` and its submodules, without test
    /// modules.
    fn sources(name: &str) -> String {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let module = module_of(name);
        let mut files = vec![src.join(format!("{module}.rs"))];
        let mut dirs = vec![src.join(module)];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        files
            .iter()
            .filter(|p| {
                let name = p.file_name().map_or_default(|n| n.to_string_lossy());
                name != "tests.rs" && !name.ends_with("_tests.rs")
            })
            .map(|p: &PathBuf| {
                let text = std::fs::read_to_string(p).unwrap();
                match text.find("#[cfg(test)]\nmod tests {") {
                    Some(at) => text[..at].to_owned(),
                    None => text,
                }
            })
            .collect()
    }

    #[test]
    fn every_listed_id_is_anchored_or_routed() {
        let found = anchored_or_routed();
        let stale: Vec<_> = listed()
            .into_iter()
            .filter(|id| !found.contains(*id))
            .collect();
        assert!(
            stale.is_empty(),
            "listed but in no anchor or route: {stale:?}"
        );
    }

    #[test]
    fn every_pinned_id_is_listed_or_known_unnamed() {
        let listed = listed();
        let unlisted: Vec<_> = pinned()
            .into_iter()
            .filter(|id| !listed.contains(id.as_str()) && !UNNAMED.contains(&id.as_str()))
            .collect();
        assert!(
            unlisted.is_empty(),
            "pinned but listed by no part of the loader: {unlisted:?}"
        );
        let named: Vec<_> = UNNAMED.iter().filter(|id| listed.contains(*id)).collect();
        assert!(named.is_empty(), "listed, so no longer unnamed: {named:?}");
    }

    /// Every typed id of `crate::engine::pinned`, by its path below it, such as
    /// `world::TIME`, read from the catalogue's sources.
    fn catalogue() -> BTreeMap<String, String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/engine/pinned");
        let mut ids = BTreeMap::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let part = path.file_stem().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            for item in text.split("pub(crate) const ").skip(1) {
                let (name, rest) = item.split_once(':').unwrap();
                let id = rest.split('"').nth(1).unwrap();
                ids.insert(format!("{part}::{name}"), id.to_owned());
            }
        }
        ids
    }

    #[test]
    fn every_catalogued_id_is_anchored_or_routed() {
        let found = anchored_or_routed();
        let stale: Vec<_> = catalogue()
            .into_values()
            .filter(|id| !found.contains(id))
            .collect();
        assert!(
            stale.is_empty(),
            "catalogued but in no anchor or route: {stale:?}"
        );
    }

    /// The catalogued ids `text` names through `pinned::`.
    fn named<'a>(text: &str, catalogue: &'a BTreeMap<String, String>) -> Vec<&'a str> {
        text.split("pinned::")
            .skip(1)
            .filter_map(|rest| {
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                    .unwrap_or(rest.len());
                catalogue.get(&rest[..end]).map(String::as_str)
            })
            .collect()
    }

    #[test]
    fn every_id_a_module_names_is_in_its_list() {
        let catalogue = catalogue();
        for (name, ids) in table_ids() {
            let missing: Vec<_> = named(&sources(name), &catalogue)
                .into_iter()
                .filter(|id| !ids.contains(id))
                .collect();
            assert!(missing.is_empty(), "{name} names {missing:?} unlisted");
        }
    }

    #[test]
    fn every_id_any_part_names_is_listed_by_some_part() {
        let (catalogue, listed) = (catalogue(), listed());
        // Parts without a list count too: one whose list went missing would pass the test above.
        for part in parts().iter() {
            let missing: Vec<_> = named(&sources(part.name), &catalogue)
                .into_iter()
                .filter(|id| !listed.contains(id))
                .collect();
            assert!(
                missing.is_empty(),
                "{} names {missing:?} no part lists",
                part.name
            );
        }
    }

    #[test]
    fn no_module_spells_a_pinned_id_as_text() {
        let pinned = pinned();
        for (name, _) in table_ids() {
            let text = sources(name);
            let spelled: Vec<_> = text
                .split('"')
                .skip(1)
                .step_by(2)
                .filter(|literal| pinned.contains(*literal))
                .collect();
            assert!(
                spelled.is_empty(),
                "{name} spells {spelled:?}; name them through crate::pinned"
            );
        }
    }

    /// The status lists a fuse once by its name, so a part's own fuse has a
    /// name no fuse of its module's links has.
    #[test]
    fn no_part_fuse_shares_its_name_with_a_fuse_of_its_module() {
        let mut shared = Vec::new();
        for part in parts().iter() {
            let named = format!("Fuse::new(\"{}\")", part.fuse.feature());
            if sources(part.name).contains(&named) {
                shared.push(part.name);
            }
        }
        assert!(
            shared.is_empty(),
            "part fuses named as a link's: {shared:?}"
        );
    }

    /// Every registry seals in the seal phase, before any link, so no link
    /// sees an open registry whatever the order the parts link in.
    #[test]
    fn no_part_seals_a_registry_in_its_link() {
        // shortcut: a scan of each `fn link*` body, which misses a seal in a
        // helper the link calls; a check in the registry's seal would see
        // it, once registries know the phase they are sealed in.
        let mut sealing = Vec::new();
        for part in parts().iter().filter(|part| part.link.is_some()) {
            if link_bodies(&sources(part.name)).any(|body| body.contains(".seal()")) {
                sealing.push(part.name);
            }
        }
        assert!(
            sealing.is_empty(),
            "parts that seal in their link: {sealing:?}"
        );
    }

    /// The bodies of the `fn link*` functions in `text`, each up to the
    /// closing brace at its own indent, inside an `impl` or `mod` too.
    fn link_bodies(text: &str) -> impl Iterator<Item = String> + '_ {
        let lines: Vec<&str> = text.lines().collect();
        let starts: Vec<usize> = (0..lines.len())
            .filter(|&at| {
                let line = lines[at].trim_start();
                !line.starts_with("//")
                    && (line.starts_with("fn link") || line.contains(" fn link"))
            })
            .collect();
        starts.into_iter().map(move |at| {
            let indent = lines[at].len() - lines[at].trim_start().len();
            let close = format!("{}}}", &lines[at][..indent]);
            lines[at..]
                .iter()
                .take_while(|line| line.trim_end() != close)
                .copied()
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    #[test]
    fn a_link_s_body_ends_at_its_own_closing_brace() {
        let text = "impl A {\n    fn link() {\n        open();\n    }\n    fn later() {\n        R.seal();\n    }\n}\n\
                    pub(crate) fn link_seam() {\n    if x {\n    }\n    S.seal();\n}\n";
        let bodies: Vec<_> = link_bodies(text).collect();
        assert_eq!(bodies.len(), 2);
        assert!(
            !bodies[0].contains(".seal()"),
            "a later function is not the link"
        );
        assert!(bodies[1].contains("S.seal()"), "{bodies:?}");
    }

    #[test]
    fn the_layers_follow_the_order_of_domains_toml() {
        let domains: toml::Table = include_str!("../../../internal/core/domains.toml")
            .parse()
            .unwrap();
        let names = |list: &toml::Value| -> Vec<String> {
            list.as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap().to_owned())
                .collect()
        };
        let mut wanted = names(&domains["loader"]["below"]);
        wanted.extend(names(&domains["order"]));
        let layers: Vec<_> = LAYERS
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .filter(|name| name != "joins")
            .collect();
        assert_eq!(layers, wanted);
    }

    #[test]
    fn each_part_lives_in_its_layer_and_has_a_name_of_its_own() {
        let mut names = BTreeSet::new();
        for (layer, parts) in LAYERS {
            for part in *parts {
                let path = part.module.split("::").nth(1).unwrap_or_default();
                if *layer == "joins" {
                    assert!(["events", "start"].contains(&path), "{}", part.module);
                } else {
                    assert_eq!(path, *layer, "{}", part.name);
                }
                assert!(names.insert(part.name), "{} twice", part.name);
            }
        }
    }

    /// Each phase of the joins, with what it calls in order: a fused call
    /// by its fuse's name, another by its part's.
    fn phases() -> String {
        let parts = parts();
        let named = |pick: fn(&crate::engine::parts::Part) -> bool| -> String {
            parts
                .iter()
                .filter(|part| pick(part))
                .map(|part| part.name)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let fused = |pick: fn(&crate::engine::parts::Part) -> Option<&'static str>| -> String {
            parts.iter().filter_map(pick).collect::<Vec<_>>().join(", ")
        };
        let stage = |stage: Stage| -> String {
            parts
                .steps(stage)
                .iter()
                .map(|step| step.work.fuse.feature())
                .collect::<Vec<_>>()
                .join(", ")
        };
        [
            ("seal", named(|p| p.seal.is_some())),
            ("link", named(|p| p.link.is_some())),
            ("step, follow", stage(Stage::Follow)),
            ("step, input", stage(Stage::Input)),
            ("step, output", stage(Stage::Output)),
            (
                "joined",
                fused(|p| p.joined.as_ref().map(|j| j.fuse.feature())),
            ),
            ("left", fused(|p| p.left.as_ref().map(|l| l.fuse.feature()))),
            ("hosting", named(|p| p.hosting.is_some())),
            (
                "end",
                parts
                    .iter()
                    .rev()
                    .filter(|p| p.end.is_some())
                    .map(|p| p.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            ("forget", named(|p| p.forget.is_some())),
            (
                "world event",
                fused(|p| p.world_event.as_ref().map(|w| w.fuse.feature())),
            ),
            ("follows", named(|p| p.follows.is_some())),
            ("game events", named(|p| p.game_events.is_some())),
            ("settings", named(|p| p.settings.is_some())),
            ("settings poll", named(|p| p.poll.is_some())),
            ("status", named(|p| p.status.is_some())),
            ("shared", named(|p| p.shared.is_some())),
            ("server link", named(|p| p.server_link.is_some())),
            ("handover", named(|p| p.handover.is_some())),
            ("facts", named(|p| p.facts.is_some())),
            ("turned", named(|p| p.turned.is_some())),
            ("patches", named(|p| p.patches.is_some())),
        ]
        .map(|(phase, calls)| format!("{phase}: {calls}"))
        .join("\n")
    }

    /// The parts whose end runs mods' handlers, in the order a hosted game
    /// ends them: windows tell their close handlers, which may queue a level
    /// write or an inventory request, then the level writes are told, whose
    /// handlers may queue an inventory request, and then the inventory
    /// answers what is queued.
    /// Every forget runs after them and drops what the handlers queued.
    #[test]
    fn a_hosted_game_tells_mods_top_layer_first_before_any_part_forgets() {
        let ending: Vec<_> = parts()
            .iter()
            .rev()
            .filter(|part| part.end.is_some())
            .map(|part| part.name)
            .collect();
        assert_eq!(ending, ["windows", "progress", "inventory"]);
        let both = parts()
            .iter()
            .find(|part| part.end.is_some() && part.forget.is_some());
        assert!(both.is_none(), "{both:?} both tells and forgets");
    }

    /// A step's work that reads what another's writes in the same step
    /// runs after it.
    #[test]
    fn a_step_reads_what_the_work_before_it_wrote() {
        let parts = parts();
        let at = |stage: Stage, feature: &str| {
            parts
                .steps(stage)
                .iter()
                .position(|step| step.work.fuse.feature() == feature)
                .unwrap_or_else(|| panic!("no {feature}"))
        };
        assert!(
            at(Stage::Follow, "item data steps") < at(Stage::Follow, "stat steps"),
            "stats work out the item stats item data attaches"
        );
        assert!(
            at(Stage::Input, "chat commands") < at(Stage::Input, "chat sending"),
            "the chat posts the commands' replies"
        );
    }

    /// The order each phase calls its parts in; a change to it shows here
    /// and is made with its reason.
    #[test]
    fn each_phase_calls_its_parts_in_layer_order() {
        insta::assert_snapshot!(phases());
    }
}
