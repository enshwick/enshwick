use super::*;

const HASHES: Hashes = Hashes {
    item: 1,
    item_registry: 2,
    template: 3,
    workshop_registry: 4,
    template_collection: 5,
};

/// The item layout of build 1269579, as far as the registry goes.
pub(crate) const ITEMS: ItemLayout = ItemLayout {
    id: 0,
    object: 4,
    name: 0x48,
    description: 0x68,
    flags: 0x78,
    pid_entity: 0x80,
    ui_values: 0x90,
    icon_image: 0x58,
    ui_value: [0x28, 0, 4, 8, 0x24],
    refs: 0,
    names: 8,
};

/// A registry listing `items` with their names.
pub(crate) fn registry(items: &[(Guid, &str)]) -> Vec<u8> {
    let mut blob = Blob::new(&[0u8; 16]);
    let refs: Vec<(Vec<u8>, Option<usize>)> =
        items.iter().map(|(g, _)| (g.0.to_vec(), None)).collect();
    blob.grow_array(0, 16, 4, &[], &refs).unwrap();
    for (_, name) in items {
        let at = blob.append(name.as_bytes(), 1);
        let first = blob
            .grow_array(8, 8, 4, &[0], &[(vec![0; 8], None)])
            .unwrap();
        let (start, _) = blob.array(8).unwrap();
        let element = start + usize::try_from(first).unwrap() * 8;
        assert!(blob.set_array(element, at, u32::try_from(name.len()).unwrap()));
    }
    blob.into_bytes()
}

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

/// A game with the items `named`, a forge station of its own workshop,
/// a survivor's workshop and an item that places a plain template.
pub(crate) fn game(named: &[(Guid, &str)]) -> Vanilla {
    let data = game_data(named);
    Vanilla::with_parts(
        ITEMS,
        super::super::workshops::tests::LAYOUT,
        HASHES,
        move |key| data.get(&key).cloned(),
    )
}

/// The recipe registry's type hash in these tests.
const RECIPE_REGISTRY_HASH: u32 = 5;

/// The recipe layout of build 1269579, as far as these tests read it.
const RECIPES: RecipeLayout = RecipeLayout {
    recipes: 8,
    recipe: 0xa8,
    guid: 0,
    id: 0x10,
    workshop_guid: 0x14,
    workshop: 0x24,
    input: 0x28,
    output: 0x30,
    completion: 0x5c,
    relative: [0x28, 0x30, 0x40, 0x78],
    input_size: 48,
    stack_size: 24,
    stack_item: 0x10,
    stack_count: 0x14,
};

/// [`game`] with a recipe registry: each recipe's GUID, debug name and
/// the item it makes.
pub(crate) fn game_with_recipes(named: &[(Guid, &str)], recipes: &[(Guid, &str, Guid)]) -> Vanilla {
    let mut blob = Blob::new(&[0u8; 16]);
    let at = blob.append(&vec![0u8; 0xa8 * recipes.len()], 8);
    for (i, (guid, name, output)) in recipes.iter().enumerate() {
        let recipe = at + i * 0xa8;
        let mut stack = vec![0u8; 24];
        stack[..16].copy_from_slice(&output.0);
        let output_at = blob.append(&stack, 8);
        let name_at = blob.append(name.as_bytes(), 1);
        assert!(blob.write(recipe, &guid.0));
        assert!(blob.set_array(recipe + 0x30, output_at, 1));
        assert!(blob.set_array(recipe + 0x78, name_at, u32::try_from(name.len()).unwrap()));
    }
    assert!(blob.set_array(8, at, u32::try_from(recipes.len()).unwrap()));
    let mut data = game_data(named);
    data.insert(
        RECIPE_REGISTRY.key_by_hash(RECIPE_REGISTRY_HASH),
        blob.into_bytes(),
    );
    Vanilla::with_parts(
        ITEMS,
        super::super::workshops::tests::LAYOUT,
        HASHES,
        move |key| data.get(&key).cloned(),
    )
    .with_recipes(RECIPES, RECIPE_REGISTRY_HASH)
}

/// [`game`] with the item `chest`, named `name`, which places a
/// container of `slots` slots.
pub(crate) fn game_with_chest(chest: Guid, name: &str, slots: u8) -> Vanilla {
    game_with_container(chest, name, slots, 0x3c0)
}

/// A game with the backpack `backpack`, named `name`, of `slots` slots
/// in the template of its own entity.
pub(crate) fn game_with_backpack(backpack: Guid, name: &str, slots: u8) -> Vanilla {
    game_with_container(backpack, name, slots, ITEMS.pid_entity)
}

/// A game with `item`, named `name`, whose template at `at` in its data
/// holds a container of `slots` slots.
fn game_with_container(chest: Guid, name: &str, slots: u8, at: usize) -> Vanilla {
    use super::super::containers::tests::{LAYOUT as CONTAINERS, chest as template};

    let placed = Guid([0x77; 16]);
    let mut data = game_data(&[(chest, name)]);
    let mut info = vec![0u8; 0x400];
    info[at..at + 16].copy_from_slice(&placed.0);
    data.insert(chest.key_by_hash(HASHES.item), info);
    data.insert(placed.key_by_hash(HASHES.template), template(slots));
    Vanilla::with_parts(
        ITEMS,
        super::super::workshops::tests::LAYOUT,
        HASHES,
        move |key| data.get(&key).cloned(),
    )
    .with_containers(Some(CONTAINERS))
}

/// A game whose templates are the containers `containers`: name and
/// fill table each.
pub(crate) fn game_with_tables(containers: &[(&str, Guid)]) -> Vanilla {
    use super::super::loot_tables::tests::{FILL, container};

    let mut data = game_data(&[]);
    let mut guids = Vec::new();
    for (i, (name, table)) in containers.iter().enumerate() {
        let template = Guid([0x40 + u8::try_from(i).unwrap(); 16]);
        data.insert(
            template.key_by_hash(HASHES.template),
            container(name, *table),
        );
        guids.extend_from_slice(&template.0);
    }
    let mut collection = Blob::new(&[0u8; 8]);
    let at = collection.append(&guids, 8);
    assert!(collection.set_array(0, at, u32::try_from(containers.len()).unwrap()));
    data.insert(
        super::super::workshops::TEMPLATE_COLLECTION.key_by_hash(HASHES.template_collection),
        collection.into_bytes(),
    );
    Vanilla::with_parts(
        ITEMS,
        super::super::workshops::tests::LAYOUT,
        HASHES,
        move |key| data.get(&key).cloned(),
    )
    .with_fill_tables(Some(FILL))
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

/// The archive of [`game`], by key.
fn game_data(named: &[(Guid, &str)]) -> rustc_hash::FxHashMap<ResourceKey, Vec<u8>> {
    use super::super::workshops::tests::{CLIENT, CRAFTING, INTERACTION, station};

    let (forge, survivor, prop) = (Guid([5; 16]), Guid([6; 16]), Guid([7; 16]));
    let (template, plain, workshop) = (Guid([8; 16]), Guid([9; 16]), Guid([10; 16]));
    let mut workshops = Blob::new(&[0u8; 24]);
    let mut entries = Vec::new();
    for (item, npc) in [(forge, 0u8), (survivor, 1), (prop, 0)] {
        let mut entry = vec![0u8; 56];
        entry[0] = npc;
        entry[4..20].copy_from_slice(&workshop.0);
        entry[28..32].copy_from_slice(&item.fnv1a().to_le_bytes());
        entries.extend_from_slice(&entry);
    }
    let at = workshops.append(&entries, 4);
    assert!(workshops.set_array(8, at, 3));
    let placing = |template: Guid| {
        let mut info = vec![0u8; 0x400];
        info[0x3c0..0x3d0].copy_from_slice(&template.0);
        info
    };
    let (crafting, _) = station(&[(CRAFTING, 20), (INTERACTION, 32), (CLIENT, 16)]);
    let (other, _) = station(&[(0x1234_5678, 24)]);
    let mut data = rustc_hash::FxHashMap::default();
    data.insert(
        ITEM_REGISTRY.key_by_hash(HASHES.item_registry),
        registry(named),
    );
    data.insert(
        WORKSHOP_REGISTRY.key_by_hash(HASHES.workshop_registry),
        workshops.into_bytes(),
    );
    for (item, places) in [(forge, template), (survivor, template), (prop, plain)] {
        data.insert(item.key_by_hash(HASHES.item), placing(places));
    }
    data.insert(template.key_by_hash(HASHES.template), crafting.into_bytes());
    data.insert(plain.key_by_hash(HASHES.template), other.into_bytes());
    data
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
        super::super::workshops::tests::LAYOUT,
        HASHES,
        |_| None,
    );
    assert_eq!(
        empty.item("Tool_Torch01"),
        Err(VanillaError::Unreadable("item registry"))
    );
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
