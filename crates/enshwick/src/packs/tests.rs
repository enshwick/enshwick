use crate::content::Guid;
use crate::content::changes::{Change, Field, Value};
use crate::content::vanilla::Vanilla;
use crate::content::workshops::stations;
use crate::loot::Kinds;

use super::*;

const CHEST: &str = "6943053a-32c1-4718-ae22-e61427ddf54d";

fn files(items: &str) -> PackFiles {
    PackFiles::new()
        .with(ITEMS_FILE, items)
        .with(TEXTS_FILE, "[en]\n\"lantern.name\" = \"Lantern\"\n")
}

#[test]
fn an_item_takes_the_pack_s_namespace() {
    let pack = Pack::parse(
        "you.lanterns",
        &files(&format!(
            "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\nname = \"lantern.name\"\n"
        )),
    )
    .unwrap();
    assert_eq!(pack.ids(), ["you.lanterns:lantern"]);
    assert_eq!(pack.items[0].like, Guid::parse(CHEST));
}

#[test]
fn problems_name_their_file_line_and_column() {
    let errors = Pack::parse(
        "you.lanterns",
        &files(&format!(
            "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\n\n[[item]]\nid = \"lantern\"\nlike = \"torch\"\nname = \"missing\"\n"
        )),
    )
    .unwrap_err();
    let lines: Vec<String> = errors.list().iter().map(ToString::to_string).collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("items.toml:5:1: item id \"lantern\" is taken")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("no item \"torch\" in this pack")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("text key \"missing\" has no text")),
        "{lines:?}"
    );
}

#[test]
fn vanilla_content_is_named_by_its_debug_name() {
    let torch = Guid([1; 16]);
    let vanilla = crate::content::vanilla::tests::game(&[
        (torch, "Tool_Torch01"),
        (Guid([5; 16]), "Prop_Production_T1_Forge"),
        (Guid([7; 16]), "Prop_Storage_24_T1_Box"),
    ]);
    let items = "[[item]]\nid = \"lantern\"\nlike = \"Tool_Torch01\"\nname = \"lantern.name\"\n";
    let workshops = "[[workshop]]\nid = \"forge\"\nlike = \"Prop_Production_T1_Forge\"\n\n\
                     [[workshop]]\nid = \"bench\"\nlike = \"workbench\"\n";
    let recipes = "[[recipe]]\nid = \"lantern.recipe\"\nlike = \"Tool_Torch01\"\n\
                   output = \"lantern\"\nworkshop = \"Prop_Production_T1_Forge\"\n";
    let named = files(items)
        .with(WORKSHOPS_FILE, workshops)
        .with(RECIPES_FILE, recipes);
    let pack = Pack::parse_with("lights", &named, Some(&vanilla)).unwrap();
    assert_eq!(pack.items[0].like, torch);
    assert_eq!(pack.workshops[0].like.template, Guid([8; 16]));
    assert_eq!(pack.workshops[1].like, stations::WORKBENCH);
    assert_eq!(pack.recipes[0].workshop, Some(Guid([10; 16])));

    let wrong = files("[[item]]\nid = \"lantern\"\nlike = \"Tool_Torch1\"\n").with(
        WORKSHOPS_FILE,
        "[[workshop]]\nid = \"forge\"\nlike = \"Prop_Storage_24_T1_Box\"\n",
    );
    let errors = Pack::parse_with("lights", &wrong, Some(&vanilla)).unwrap_err();
    let lines: Vec<String> = errors.list().iter().map(ToString::to_string).collect();
    let unknown = "items.toml:1:1: no item \"Tool_Torch1\" in this pack, \
                   and no vanilla item \"Tool_Torch1\"; near: Tool_Torch01";
    assert!(lines.iter().any(|l| l == unknown), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("workshops.toml:1:1: workshop forge:")
                && l.contains("no crafting station")),
        "{lines:?}"
    );
    let errors = Pack::parse_with("lights", &wrong, None).unwrap_err();
    assert!(
        errors.to_string().contains("needs the game's data"),
        "{errors}"
    );
}

#[test]
fn an_unknown_field_is_a_problem_at_its_line() {
    let errors = Pack::parse(
        "you.lanterns",
        &files(&format!(
            "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\ncolour = 3\n"
        )),
    )
    .unwrap_err();
    let first = &errors.list()[0];
    assert_eq!(
        (first.file.as_str(), first.line),
        (ITEMS_FILE, 4),
        "{errors}"
    );
    assert!(first.message.contains("colour"), "{errors}");
}

#[test]
fn recipes_reach_the_pack_s_items_and_workshops() {
    let files = files(&format!("[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\n"))
        .with(WORKSHOPS_FILE, "[[workshop]]\nid = \"bench\"\nlike = \"workbench\"\nrecipe = { cost = 2 }\n")
        .with(
            RECIPES_FILE,
            format!("[[recipe]]\nid = \"lantern.recipe\"\nlike = \"{CHEST}\"\noutput = \"lantern\"\nworkshop = \"bench\"\n"),
        );
    let pack = Pack::parse("you.lanterns", &files).unwrap();
    let recipe = &pack.recipes[0];
    assert_eq!(recipe.output, Guid::named("you.lanterns:lantern"));
    assert_eq!(
        recipe.workshop,
        Some(Guid::named("you.lanterns:bench.workshop"))
    );
    assert_eq!(
        pack.ids(),
        [
            "you.lanterns:bench",
            "you.lanterns:lantern",
            "you.lanterns:lantern.recipe"
        ]
    );
}

#[test]
fn a_drop_s_meter_follows_its_items() {
    let items = format!("[[item]]\nid = \"staff\"\nlike = \"{CHEST}\"\nmeter = {{ max = 3 }}\n");
    let good = files(&items).with(LOOT_FILE, "[[drop]]\nitems = [\"staff\"]\nchance = 0.5\nfrom = [\"corpses\", \"chests\"]\nmeter = [1, 3]\n");
    let pack = Pack::parse("staffs", &good).unwrap();
    let drop = &pack.drops()[0];
    assert_eq!(drop.items, [Guid::named("staffs:staff").id()]);
    assert_eq!(drop.from.kinds, Kinds::CONTAINERS);
    assert_eq!(drop.meter.map(|m| m.of), Some(3));
    let bad = files(&items).with(
        LOOT_FILE,
        "[[drop]]\nitems = [\"staff\"]\nchance = 1.5\nfrom = \"moon\"\nmeter = [1, 4]\n",
    );
    let errors = Pack::parse("staffs", &bad).unwrap_err().to_string();
    assert!(errors.contains("loot.toml:1:1: chance 1.5"), "{errors}");
    assert!(errors.contains("from \"moon\""), "{errors}");
}

#[test]
fn loot_alone_keeps_the_same_content() {
    let a = Pack::parse("p", &PackFiles::new()).unwrap();
    let b = Pack::parse("p", &PackFiles::new().with(TEXTS_FILE, "[en]\na = \"b\"\n")).unwrap();
    assert!(a.same_content(&a.clone()));
    assert!(!a.same_content(&b));
}

#[test]
fn a_manifest_names_its_problem_line() {
    let error = PackManifest::parse("version = \"1.0.0\"\nid = \"Bad Id\"\n").unwrap_err();
    assert_eq!((error.line, error.column), (2, 1), "{error}");
    let error = PackManifest::parse("id = \"a\"\nversion = \"1\"\nextra = 1\n").unwrap_err();
    assert_eq!(error.line, 3, "{error}");
}

#[test]
fn groups_differ_by_pack_and_are_never_a_mod_s() {
    assert_ne!(group("a"), group("b"));
    assert_ne!(group("a"), 0);
}

fn vanilla_wood() -> (Vanilla, Guid) {
    let wood = Guid([1; 16]);
    (
        crate::content::vanilla::tests::game(&[(wood, "Material_T1_Wood_Logs")]),
        wood,
    )
}

#[test]
fn an_item_change_sets_the_stack_size_of_every_item_it_names() {
    let (vanilla, wood) = vanilla_wood();
    let items =
        format!("[[change]]\nitems = [\"Material_T1_Wood_Logs\", \"{CHEST}\"]\nmax_stack = 999\n");
    let pack = Pack::parse_with("you.stacks", &files(&items), Some(&vanilla)).unwrap();
    let set = Value::Set(999.0);
    assert_eq!(
        pack.changes(),
        [
            (
                Change::new(wood, Field::MaxStack, set),
                "items.toml:1 (Material_T1_Wood_Logs)".to_owned()
            ),
            (
                Change::new(Guid::parse(CHEST), Field::MaxStack, set),
                "items.toml:1".to_owned()
            ),
        ]
    );
    assert!(pack.reaches_players());
}

#[test]
fn a_table_with_times_scales_the_value() {
    let pack = Pack::parse(
        "you.stacks",
        &files(&format!(
            "\n[[change]]\nitems = [\"{CHEST}\"]\nmax_stack = {{ times = 4 }}\n"
        )),
    )
    .unwrap();
    assert_eq!(pack.changes()[0].0.value, Value::Times(4.0));
    assert_eq!(pack.changes()[0].1, "items.toml:2");
}

#[test]
fn a_value_out_of_range_is_a_problem_at_its_change() {
    let items = format!(
        "[[change]]\nitems = [\"{CHEST}\"]\nmax_stack = 20\n\n[[change]]\nitems = [\"{CHEST}\"]\nmax_stack = 70000\n"
    );
    let errors = Pack::parse("you.stacks", &files(&items)).unwrap_err();
    assert_eq!(
        errors.to_string(),
        "items.toml:5:1: change 2: max_stack 70000 is not from 1 to 65535"
    );
}

#[test]
fn one_field_of_one_item_changed_twice_names_both_lines() {
    let items = format!(
        "[[change]]\nitems = [\"{CHEST}\"]\nmax_stack = 20\n\n[[change]]\nitems = [\"{CHEST}\"]\nmax_stack = {{ times = 2 }}\n"
    );
    let errors = Pack::parse("you.stacks", &files(&items)).unwrap_err();
    assert_eq!(
        errors.to_string(),
        format!("items.toml:5:1: change 2: max_stack of {CHEST} is changed at line 1 already")
    );
}

#[test]
fn a_change_that_picks_or_changes_nothing_is_a_problem() {
    let errors = Pack::parse(
        "you.stacks",
        &files("[[change]]\nitems = []\nmax_stack = 20\n"),
    )
    .unwrap_err();
    assert_eq!(
        errors.to_string(),
        "items.toml:1:1: change 1: items names no item"
    );
    let errors = Pack::parse(
        "you.stacks",
        &files(&format!("[[change]]\nitems = [\"{CHEST}\"]\n")),
    )
    .unwrap_err();
    assert_eq!(
        errors.to_string(),
        "items.toml:1:1: change 1: changes nothing; give max_stack, slots"
    );
}

#[test]
fn a_misspelt_item_in_a_change_suggests_the_near_name() {
    let (vanilla, _) = vanilla_wood();
    let items = "[[change]]\nitems = [\"Material_T1_Wood_Logss\"]\nmax_stack = 50\n";
    let errors = Pack::parse_with("you.stacks", &files(items), Some(&vanilla)).unwrap_err();
    assert!(
        errors.to_string().contains("near: Material_T1_Wood_Logs"),
        "{errors}"
    );
}

#[test]
fn a_pack_without_changes_reaches_players_only_through_its_content() {
    let pack = Pack::parse("you.stacks", &PackFiles::new()).unwrap();
    assert!(pack.changes().is_empty());
    assert!(!pack.reaches_players());
}

fn vanilla_torch() -> (Vanilla, Guid, Guid, Guid) {
    let (torch, wood, recipe) = (Guid([1; 16]), Guid([2; 16]), Guid([3; 16]));
    let vanilla = crate::content::vanilla::tests::game_with_recipes(
        &[(torch, "Tool_Torch01"), (wood, "Material_T1_Wood_Logs")],
        &[(recipe, "Tool_Torch01_T0_PK", torch)],
    );
    (vanilla, torch, wood, recipe)
}

fn recipes(text: &str) -> PackFiles {
    PackFiles::new().with(RECIPES_FILE, text)
}

#[test]
fn a_recipe_change_picks_recipes_by_name_and_by_what_they_make() {
    let (vanilla, _, wood, recipe) = vanilla_torch();
    let text = "[[change]]\nrecipes = [\"Tool_Torch01_T0_PK\"]\noutput_count = 3\n\n\
                [[change]]\noutput = [\"Tool_Torch01\"]\ntime = { times = 0.5 }\n\
                ingredients = { Material_T1_Wood_Logs = 0 }\n";
    let pack = Pack::parse_with("you.torches", &recipes(text), Some(&vanilla)).unwrap();
    let changes: Vec<(Change, &str)> = pack
        .changes()
        .iter()
        .map(|(c, s)| (*c, s.as_str()))
        .collect();
    assert_eq!(
        changes,
        [
            (
                Change::new(recipe, Field::Output, Value::Set(3.0)),
                "recipes.toml:1 (Tool_Torch01_T0_PK)"
            ),
            (
                Change::new(recipe, Field::Time, Value::Times(0.5)),
                "recipes.toml:5 (Tool_Torch01)"
            ),
            (
                Change::ingredient(recipe, wood, Value::Set(0.0)),
                "recipes.toml:5 (Tool_Torch01)"
            ),
        ]
    );
}

#[test]
fn a_recipe_change_without_the_game_s_data_takes_guids_only() {
    let text = format!("[[change]]\nrecipes = [\"{CHEST}\"]\ncost = 2\n");
    let pack = Pack::parse("you.torches", &recipes(&text)).unwrap();
    assert_eq!(pack.changes()[0].0.field, Field::Cost);
    let errors = Pack::parse(
        "you.torches",
        &recipes("[[change]]\noutput = [\"Tool_Torch01\"]\ncost = 2\n"),
    )
    .unwrap_err();
    assert!(
        errors.to_string().contains("the game's data is at hand"),
        "{errors}"
    );
}

#[test]
fn a_recipe_change_naming_no_recipe_or_a_misspelt_one_is_a_problem() {
    let (vanilla, ..) = vanilla_torch();
    let errors = Pack::parse_with(
        "you.torches",
        &recipes("[[change]]\ncost = 2\n"),
        Some(&vanilla),
    )
    .unwrap_err();
    assert_eq!(
        errors.to_string(),
        "recipes.toml:1:1: change 1: names no recipe; give recipes or output"
    );
    let errors = Pack::parse_with(
        "you.torches",
        &recipes("[[change]]\nrecipes = [\"Tool_Torch01_T0_PKK\"]\ncost = 2\n"),
        Some(&vanilla),
    )
    .unwrap_err();
    assert!(
        errors.to_string().contains("near: Tool_Torch01_T0_PK"),
        "{errors}"
    );
    let errors = Pack::parse_with(
        "you.torches",
        &recipes("[[change]]\noutput = [\"Material_T1_Wood_Logs\"]\ncost = 2\n"),
        Some(&vanilla),
    )
    .unwrap_err();
    assert!(
        errors
            .to_string()
            .contains("no vanilla recipe makes Material_T1_Wood_Logs"),
        "{errors}"
    );
}

#[test]
fn one_value_of_one_recipe_changed_twice_names_both_lines() {
    let (vanilla, ..) = vanilla_torch();
    let text = "[[change]]\nrecipes = [\"Tool_Torch01_T0_PK\"]\ncost = 2\n\n\
                [[change]]\noutput = [\"Tool_Torch01\"]\ncost = 3\n";
    let errors = Pack::parse_with("you.torches", &recipes(text), Some(&vanilla)).unwrap_err();
    assert_eq!(
        errors.to_string(),
        "recipes.toml:5:1: change 2: cost of Tool_Torch01 is changed at line 1 already"
    );
}

#[test]
fn a_recipe_a_mod_or_the_pack_adds_is_changed_where_it_is_defined() {
    let (vanilla, ..) = vanilla_torch();
    let text = format!(
        "[[recipe]]\nid = \"lantern.recipe\"\nlike = \"{CHEST}\"\noutput = \"{CHEST}\"\n\n\
         [[change]]\nrecipes = [\"lantern.recipe\", \"you.lockers:locker.recipe\"]\ncost = 2\n"
    );
    let errors = Pack::parse_with("you.torches", &recipes(&text), Some(&vanilla)).unwrap_err();
    let lines: Vec<String> = errors.list().iter().map(ToString::to_string).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines
            .iter()
            .all(|l| l.contains("set its values where it is defined")),
        "{lines:?}"
    );
}

#[test]
fn a_chest_s_slots_change_in_pages_of_eight() {
    let items = format!(
        "[[change]]\nitems = [\"{CHEST}\"]\nslots = 48\n\n[[change]]\nitems = [\"{CHEST}\"]\nslots = 50\n"
    );
    let errors = Pack::parse("you.chests", &files(&items)).unwrap_err();
    assert_eq!(
        errors.to_string(),
        "items.toml:5:1: change 2: slots 50 is not a multiple of 8"
    );
    let pack = Pack::parse(
        "you.chests",
        &files(&format!(
            "[[change]]\nitems = [\"{CHEST}\"]\nslots = {{ times = 2 }}\n"
        )),
    )
    .unwrap();
    assert_eq!(pack.changes()[0].0.field, Field::Slots);
}

#[test]
fn slots_go_only_to_a_container_and_never_shrink_it() {
    let chest = Guid([1; 16]);
    let vanilla =
        crate::content::vanilla::tests::game_with_chest(chest, "Prop_Storage_24_T1_Box", 24);
    let text = "[[change]]\nitems = [\"Prop_Storage_24_T1_Box\"]\nslots = 8\n";
    let errors = Pack::parse_with("you.chests", &files(text), Some(&vanilla)).unwrap_err();
    assert!(
        errors.to_string().ends_with("a container only grows"),
        "{errors}"
    );
    let text = "[[change]]\nitems = [\"Prop_Storage_24_T1_Box\"]\nslots = { times = 0.5 }\n";
    let errors = Pack::parse_with("you.chests", &files(text), Some(&vanilla)).unwrap_err();
    assert!(errors.to_string().contains("slots only grows"), "{errors}");
    let prop = Guid([7; 16]);
    let text = format!("[[change]]\nitems = [\"{prop}\"]\nslots = 32\n");
    let errors = Pack::parse_with("you.chests", &files(&text), Some(&vanilla)).unwrap_err();
    assert!(
        errors.to_string().contains("places no container"),
        "{errors}"
    );
    let text = "[[change]]\nitems = [\"Prop_Storage_24_T1_Box\"]\nslots = 32\n";
    assert!(Pack::parse_with("you.chests", &files(text), Some(&vanilla)).is_ok());
}
