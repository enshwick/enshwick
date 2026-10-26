use crate::content::Guid;
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
        (torch, "Torch"),
        (Guid([5; 16]), "Forge"),
        (Guid([7; 16]), "Prop"),
    ]);
    let items = "[[item]]\nid = \"lantern\"\nlike = \"Torch\"\nname = \"lantern.name\"\n";
    let workshops = "[[workshop]]\nid = \"forge\"\nlike = \"Forge\"\n\n\
                     [[workshop]]\nid = \"bench\"\nlike = \"workbench\"\n";
    let recipes = "[[recipe]]\nid = \"lantern.recipe\"\nlike = \"Torch\"\n\
                   output = \"lantern\"\nworkshop = \"Forge\"\n";
    let named = files(items)
        .with(WORKSHOPS_FILE, workshops)
        .with(RECIPES_FILE, recipes);
    let pack = Pack::parse_with("lights", &named, Some(&vanilla)).unwrap();
    assert_eq!(pack.items[0].like, torch);
    assert_eq!(pack.workshops[0].like.template, Guid([8; 16]));
    assert_eq!(pack.workshops[1].like, stations::WORKBENCH);
    assert_eq!(pack.recipes[0].workshop, Some(Guid([10; 16])));

    let wrong = files("[[item]]\nid = \"lantern\"\nlike = \"Torchh\"\n").with(
        WORKSHOPS_FILE,
        "[[workshop]]\nid = \"forge\"\nlike = \"Prop\"\n",
    );
    let errors = Pack::parse_with("lights", &wrong, Some(&vanilla)).unwrap_err();
    let lines: Vec<String> = errors.list().iter().map(ToString::to_string).collect();
    let unknown = "items.toml:1:1: no item \"Torchh\" in this pack, \
                   and no vanilla item \"Torchh\"; near: Torch";
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
