//! Engine facts by an id of enshwick's own: the API asks for a type or a
//! field offset by id, and the loader answers it from the game's reflection,
//! so a game update that renames or moves something changes this table, not
//! the mods.

use enshwick_reflect::Reflection;

/// How the loader finds one engine fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fact {
    /// A type, by its reflected name: answers type lookups by the id.
    Type(&'static str),
    /// The offset of `field` in `ty`, plus `plus` bytes into the field.
    Offset {
        ty: &'static str,
        field: &'static str,
        plus: u64,
    },
}

/// Every fact the API asks for, by id.
pub(crate) const FACTS: &[(&str, Fact)] = &[
    (
        "world_transform.position",
        offset("keen::WorldTransform", "position"),
    ),
    (
        "world_transform.orientation",
        offset("keen::WorldTransform", "orientation"),
    ),
    (
        "world_transform.scale",
        offset("keen::WorldTransform", "scale"),
    ),
    (
        "npc_command_event",
        Fact::Type("keen::ecs::NpcCommandEvent"),
    ),
    (
        "npc_command_event.kind",
        offset("keen::ecs::NpcCommandEvent", "type"),
    ),
    (
        "npc_command_event.npc",
        offset("keen::ecs::NpcCommandEvent", "npcId"),
    ),
    (
        "npc_command_event.target",
        offset("keen::ecs::NpcCommandEvent", "targetTransform"),
    ),
    (
        "set_toggle_state_event",
        Fact::Type("keen::ecs::SetToggleStateEvent"),
    ),
    (
        "set_toggle_state_event.sender",
        offset("keen::ecs::SetToggleStateEvent", "senderId"),
    ),
    (
        "set_toggle_state_event.signal",
        offset("keen::ecs::SetToggleStateEvent", "signalId"),
    ),
    (
        "set_toggle_state_event.state",
        offset("keen::ecs::SetToggleStateEvent", "newState"),
    ),
    (
        "add_threat_target_event",
        Fact::Type("keen::ecs::AddThreatTargetEvent"),
    ),
    (
        "add_threat_target_event.attacker",
        offset("keen::ecs::AddThreatTargetEvent", "attackerId"),
    ),
    (
        "add_threat_target_event.target",
        offset("keen::ecs::AddThreatTargetEvent", "targetId"),
    ),
    (
        "add_threat_target_event.threat",
        offset("keen::ecs::AddThreatTargetEvent", "threatValue"),
    ),
    ("ui_ping_event", Fact::Type("keen::ecs::UiPingEvent")),
    (
        "ui_ping_event.sender",
        offset("keen::ecs::UiPingEvent", "senderEntityId"),
    ),
    (
        "ui_ping_event.position",
        offset("keen::ecs::UiPingEvent", "position"),
    ),
];

const fn offset(ty: &'static str, field: &'static str) -> Fact {
    Fact::Offset { ty, field, plus: 0 }
}

/// The fact named `id`, if the catalogue has one.
pub(crate) fn fact(id: &str) -> Option<Fact> {
    FACTS
        .iter()
        .find_map(|&(name, fact)| (name == id).then_some(fact))
}

/// The engine's name for the type `name` stands for: the reflected name of
/// a type id, or `name` itself, which a mod may pass at the low level.
pub(crate) fn type_name(name: &str) -> &str {
    match fact(name) {
        Some(Fact::Type(engine)) => engine,
        _ => name,
    }
}

/// The value of the offset named `id` in `reflection`; `None` when the id
/// names no offset or reflection lacks its type or field.
pub(crate) fn constant(id: &str, reflection: &Reflection<'_>) -> Option<u64> {
    match fact(id)? {
        Fact::Offset { ty, field, plus } => {
            reflection.field_offset(ty, field).ok()?.checked_add(plus)
        }
        Fact::Type(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_listed_once() {
        let mut ids: Vec<&str> = FACTS.iter().map(|&(id, _)| id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn the_catalogue_holds_exactly_the_ids_the_api_asks_for() {
        let mut api: Vec<&str> = enshwick::__private::ENGINE_IDS.to_vec();
        api.sort_unstable();
        let mut ours: Vec<&str> = FACTS.iter().map(|&(id, _)| id).collect();
        ours.sort_unstable();
        assert_eq!(api, ours);
    }

    #[test]
    fn no_id_shadows_a_build_table_id() {
        for table in crate::builds::load() {
            let ids = table.symbols().iter().map(enshwick_gamedata::Symbol::id);
            let mut ids = ids.chain(
                table
                    .constants()
                    .iter()
                    .map(enshwick_gamedata::Constant::id),
            );
            assert!(!ids.any(|id| fact(id).is_some()), "{}", table.file_name());
        }
    }

    #[test]
    fn a_type_id_answers_its_engine_name_and_other_names_pass_through() {
        assert_eq!(type_name("ui_ping_event"), "keen::ecs::UiPingEvent");
        assert_eq!(
            type_name("keen::ecs::UiPingEvent"),
            "keen::ecs::UiPingEvent"
        );
        assert_eq!(
            type_name("world_transform.position"),
            "world_transform.position"
        );
    }

    /// On the server `ENSH_SERVER_EXE` names (skipped without it), every
    /// type and offset of the catalogue resolves in the game's reflection.
    #[test]
    fn every_fact_resolves_on_the_server() {
        let Ok(path) = std::env::var("ENSH_SERVER_EXE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image =
            enshwick_reflect::Image::parse(&bytes, enshwick_reflect::Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        for &(id, fact) in FACTS {
            match fact {
                Fact::Type(engine) => assert!(reflection.type_info(engine).is_ok(), "{id}"),
                Fact::Offset { .. } => assert!(constant(id, &reflection).is_some(), "{id}"),
            }
        }
    }

    #[test]
    fn every_offset_names_a_type_the_catalogue_or_the_engine_spells_out() {
        for &(id, fact) in FACTS {
            if let Fact::Offset { ty, .. } = fact {
                assert!(ty.starts_with("keen::"), "{id}");
            }
        }
    }
}
