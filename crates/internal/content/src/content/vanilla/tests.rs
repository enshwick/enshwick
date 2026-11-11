use super::fixtures::{
    HASHES, ITEMS, game, game_with_chest, game_with_container, game_with_recipes, game_with_tables,
    registry,
};
use super::*;

#[test]
fn items_are_found_by_name_and_near_names_suggested() {
    let torch = Guid([1; 16]);
    let (one, two) = (Guid([2; 16]), Guid([3; 16]));
    let data = registry(&[
        (torch, "Tool_Torch01"),
        (one, "Twin"),
        (two, "Twin"),
        (Guid([4; 16]), "Material_T2_Wood_Planks"),
        (Guid([5; 16]), "Material_T1_Wood_Logs"),
    ]);
    let names = registry_names(&ITEMS, &data).unwrap();
    assert_eq!(names.get("Tool_Torch01"), Some(&vec![torch]));
    assert_eq!(near(&names, "tool_torch01"), ["Tool_Torch01"]);
    assert_eq!(
        near(&names, "wood"),
        ["Material_T1_Wood_Logs", "Material_T2_Wood_Planks"]
    );
    assert!(near(&names, "nothing").is_empty());
}

#[test]
fn a_registry_that_does_not_read_yet_is_read_again_and_each_name_tag_once_it_does() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let (torch, tag) = (Guid([7; 16]), Guid([9; 16]));
    let mut info = vec![0u8; 0x60];
    info[0x48..0x58].copy_from_slice(&tag.0);
    let data: rustc_hash::FxHashMap<ResourceKey, Vec<u8>> = [
        (
            ITEM_REGISTRY.key_by_hash(HASHES.item_registry),
            registry(&[(torch, "Tool_Torch01")]),
        ),
        (torch.key_by_hash(HASHES.item), info),
    ]
    .into_iter()
    .collect();
    let (ready, tag_reads) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    );
    let (gate, counter) = (Arc::clone(&ready), Arc::clone(&tag_reads));
    let vanilla = Vanilla::with_parts(
        ITEMS,
        super::super::workshops::fixtures::LAYOUT,
        HASHES,
        move |key| {
            if !gate.load(Ordering::Relaxed) {
                return None;
            }
            if key.type_hash == HASHES.item {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            data.get(&key).cloned()
        },
    )
    .with_item_names(Some(0x48));
    assert_eq!(
        vanilla.items(),
        Err(VanillaError::Unreadable("item registry"))
    );
    ready.store(true, Ordering::Relaxed);
    let expected = [VanillaItem {
        guid: torch,
        name: "Tool_Torch01".into(),
        name_tag: Some(tag),
    }];
    assert_eq!(vanilla.items(), Ok(&expected[..]));
    assert_eq!(vanilla.items(), Ok(&expected[..]));
    assert_eq!(
        tag_reads.load(Ordering::Relaxed),
        1,
        "each tag is read once"
    );
}

#[test]
fn a_registry_that_reads_but_does_not_parse_is_read_only_once() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let reads = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reads);
    let vanilla = Vanilla::with_parts(
        ITEMS,
        super::super::workshops::fixtures::LAYOUT,
        HASHES,
        move |key| {
            (key.type_hash == HASHES.item_registry).then(|| {
                counter.fetch_add(1, Ordering::Relaxed);
                vec![0xff; 3]
            })
        },
    );
    for _ in 0..2 {
        assert_eq!(
            vanilla.item("Tool_Torch01"),
            Err(VanillaError::Unreadable("item registry"))
        );
    }
    assert_eq!(
        reads.load(Ordering::Relaxed),
        1,
        "a layout that does not fit is kept"
    );
}

/// A game with the backpack `backpack`, named `name`, of `slots` slots
/// in the template of its own entity.
pub(crate) fn game_with_backpack(backpack: Guid, name: &str, slots: u8) -> Vanilla {
    game_with_container(backpack, name, slots, ITEMS.pid_entity)
}

#[test]
fn a_container_s_loot_table_is_found_by_its_name_with_its_sharers() {
    let (corpses, chest) = (Guid([1; 16]), Guid([2; 16]));
    let vanilla = game_with_tables(&[
        ("LootContainer_Enemy_Corpse_Vukah", corpses),
        ("LootContainer_Enemy_Wildlife_Vuka_Melee", corpses),
        ("LootChest_Gold", chest),
    ]);
    assert_eq!(
        vanilla.loot_table("LootContainer_Enemy_Corpse_Vukah"),
        Ok(LootTable {
            table: corpses,
            containers: 2
        })
    );
    assert_eq!(vanilla.loot_table("LootChest_Gold").unwrap().containers, 1);
    assert!(
        vanilla
            .loot_table("LootChest_Gld")
            .unwrap_err()
            .to_string()
            .contains("near: LootChest_Gold")
    );
}

#[test]
fn a_template_is_found_by_its_name_or_its_near_names_or_its_twins_named() {
    use super::super::loot_tables::fixtures::FILL;

    let vanilla = game_with_tables(&[
        ("Enemy_Wolf", Guid([1; 16])),
        ("Prop_Chest", Guid([2; 16])),
        ("Prop_Chest", Guid([3; 16])),
    ])
    .with_template_names(Some(FILL.name));
    assert_eq!(vanilla.template("Enemy_Wolf"), Ok(Guid([0x40; 16])));
    assert_eq!(
        vanilla.template("Prop_Chest"),
        Err(VanillaError::AmbiguousTemplate {
            name: "Prop_Chest".into(),
            templates: vec![Guid([0x41; 16]), Guid([0x42; 16])],
        })
    );
    let unknown = vanilla.template("Enemy_Wolv").unwrap_err().to_string();
    assert!(unknown.contains("near: Enemy_Wolf"), "{unknown}");
}

#[test]
fn a_chest_s_slots_are_read_from_the_template_it_places() {
    let chest = Guid([1; 16]);
    let vanilla = game_with_chest(chest, "Prop_Storage_24_T1_Box", 24);
    assert_eq!(vanilla.container_slots(chest), Ok(Some(24)));
    assert_eq!(vanilla.container_slots(Guid([7; 16])), Ok(None));
    assert_eq!(
        game(&[]).container_slots(chest),
        Err(VanillaError::Unreadable("container layout"))
    );
}

#[test]
fn a_backpack_s_slots_are_read_from_its_own_entity() {
    let backpack = Guid([2; 16]);
    let vanilla = game_with_backpack(backpack, "Equipment_Backpack_T1", 16);
    assert_eq!(vanilla.container_slots(backpack), Ok(Some(16)));
}

#[test]
fn a_recipe_is_found_by_its_name_and_by_what_it_makes() {
    let (torch, recipe) = (Guid([1; 16]), Guid([3; 16]));
    let vanilla = game_with_recipes(
        &[(torch, "Tool_Torch01")],
        &[(recipe, "Tool_Torch01_T0_PK", torch)],
    );
    assert_eq!(vanilla.recipe("Tool_Torch01_T0_PK"), Ok(recipe));
    assert_eq!(
        vanilla
            .recipe("tool_torch01_t0_pk")
            .unwrap_err()
            .to_string(),
        "no vanilla recipe \"tool_torch01_t0_pk\"; near: Tool_Torch01_T0_PK"
    );
    assert_eq!(vanilla.recipes_making(torch), Ok(vec![recipe]));
    assert_eq!(vanilla.recipes_making(Guid([9; 16])), Ok(Vec::new()));
    assert_eq!(
        game(&[]).recipe("Tool_Torch01_T0_PK"),
        Err(VanillaError::Unreadable("recipe registry"))
    );
}

#[test]
fn a_name_finds_its_item_or_explains_why_not() {
    let torch = Guid([1; 16]);
    let vanilla = game(&[
        (torch, "Tool_Torch01"),
        (Guid([2; 16]), "Twin"),
        (Guid([3; 16]), "Twin"),
    ]);
    assert_eq!(vanilla.item("Tool_Torch01"), Ok(torch));
    let unknown = vanilla.item("torch").unwrap_err();
    assert_eq!(
        unknown.to_string(),
        "no vanilla item \"torch\"; near: Tool_Torch01"
    );
    let ambiguous = vanilla.item("Twin").unwrap_err().to_string();
    assert!(
        ambiguous.starts_with("2 vanilla items are named \"Twin\""),
        "{ambiguous}"
    );
    let empty = Vanilla::with_parts(
        ITEMS,
        super::super::workshops::fixtures::LAYOUT,
        HASHES,
        |_| None,
    );
    assert_eq!(
        empty.item("Tool_Torch01"),
        Err(VanillaError::Unreadable("item registry"))
    );
}

#[test]
fn an_item_id_finds_only_the_vanilla_item_of_that_key() {
    let torch = Guid([1; 16]);
    let vanilla = game(&[(torch, "Tool_Torch01")]);
    assert_eq!(vanilla.item_with_id(torch.fnv1a()), Some(torch));
    assert_eq!(vanilla.item_with_id(Guid([2; 16]).fnv1a()), None);
}

#[test]
fn only_a_crafting_station_of_its_own_can_be_cloned() {
    let vanilla = game(&[]);
    let station = vanilla.station(Guid([5; 16])).unwrap();
    assert_eq!(station.template, Guid([8; 16]));
    assert_eq!(
        vanilla.workshop(Guid([5; 16])).map(WorkshopId::guid),
        Ok(Guid([10; 16]))
    );
    for (item, why) in [
        (6, "a survivor's"),
        (7, "no crafting components"),
        (11, "no workshop"),
    ] {
        assert_eq!(
            vanilla.station(Guid([item; 16])),
            Err(VanillaError::NotAStation(Guid([item; 16]))),
            "{why}"
        );
    }
    assert_eq!(
        vanilla.station(stations::WORKBENCH.item),
        Ok(stations::WORKBENCH)
    );
}
