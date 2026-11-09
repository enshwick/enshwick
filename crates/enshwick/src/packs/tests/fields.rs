//! A misspelt or unknown field in any pack file is named, with the fields
//! its table takes and the one it is near, at its file, line and column.

use super::*;

/// The one problem of the pack `files`, as the loader and `pack check`
/// read them.
fn problem(files: &PackFiles) -> String {
    let errors = Pack::parse("you.fields", files).unwrap_err();
    let lines: Vec<String> = errors.list().iter().map(ToString::to_string).collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    lines.into_iter().next().unwrap()
}

/// Asserts `line` is at `at` and names `field` and the field `near` it.
fn names(line: &str, at: &str, field: &str, near: &str) {
    assert!(line.starts_with(at), "{line}");
    assert!(line.contains(&format!("unknown field `{field}`")), "{line}");
    assert!(line.ends_with(&format!("; near: {near}")), "{line}");
}

#[test]
fn a_misspelt_field_of_a_gift_is_named_with_the_one_it_is_near() {
    let line = problem(&PackFiles::new().with(
        REWARDS_FILE,
        "[[reward]]\nid = \"torch\"\non = { join = true }\ngive = [\n{ item = \"Tool_Torch99\", cout = 1 }\n]\n",
    ));
    // TOML places a problem of an array's element at the array.
    names(&line, "rewards.toml:4:8:", "cout", "count");
    assert!(line.contains("expected `item` or `count`"), "{line}");
}

#[test]
fn a_gift_of_no_known_kind_names_every_field_a_gift_takes() {
    let line = problem(&PackFiles::new().with(
        REWARDS_FILE,
        "[[reward]]\nid = \"xp\"\non = { join = true }\ngive = [{ xperience = 5 }]\n",
    ));
    names(&line, "rewards.toml:4:", "xperience", "experience");
    assert!(
        line.contains("`item`, `count`, `experience`, `knowledge`, `buff`"),
        "{line}"
    );
}

#[test]
fn a_misspelt_field_of_an_item_is_named() {
    let line = problem(&files(&format!(
        "[[item]]\nid = \"lantern\"\nlikee = \"{CHEST}\"\n"
    )));
    names(&line, "items.toml:3:", "likee", "like");
}

#[test]
fn a_misspelt_scale_of_a_change_is_named() {
    let line = problem(&files(
        "[[change]]\nitems = [\"Tool_Torch01\"]\nmax_stack = { tims = 4 }\n",
    ));
    names(&line, "items.toml:3:", "tims", "times");
}

#[test]
fn a_misspelt_field_of_a_recipe_is_named() {
    let line = problem(&PackFiles::new().with(
        RECIPES_FILE,
        "[[recipe]]\nid = \"brew\"\noutpt = \"tonic\"\n",
    ));
    names(&line, "recipes.toml:3:", "outpt", "output");
}

#[test]
fn a_misspelt_field_of_a_workshop_is_named() {
    let line = problem(&PackFiles::new().with(
        WORKSHOPS_FILE,
        "[[workshop]]\nid = \"forge\"\nlikes = \"workbench\"\n",
    ));
    names(&line, "workshops.toml:3:", "likes", "like");
}

#[test]
fn a_misspelt_setting_of_a_drop_is_named() {
    let line = problem(&PackFiles::new().with(
        LOOT_FILE,
        "[[drop]]\nitems = [\"Tool_Torch01\"]\nchance = { settings = \"rate\" }\nfrom = \"bosses\"\n",
    ));
    names(&line, "loot.toml:3:", "settings", "setting");
}

#[test]
fn a_misspelt_field_of_a_sound_is_named() {
    let line = problem(&PackFiles::new().with(SOUNDS_FILE, "[[sound]]\nidd = \"horn\"\n"));
    names(&line, "sounds.toml:2:", "idd", "id");
}

#[test]
fn a_misspelt_field_of_the_manifest_or_a_dependency_is_named() {
    let line = PackManifest::parse("id = \"you.fields\"\nversion = \"1.0.0\"\nversions = \"2\"\n")
        .unwrap_err()
        .to_string();
    names(&line, "pack.toml:3:", "versions", "version");
    let line = PackManifest::parse(
        "id = \"you.fields\"\nversion = \"1.0.0\"\n\n[requires]\nlockers = { id = \"you.lockers\", versions = \"1\" }\n",
    )
    .unwrap_err()
    .to_string();
    names(&line, "pack.toml:5:", "versions", "version");
}

#[test]
fn only_the_closest_fields_are_near() {
    assert_eq!(
        near_field("unknown field `abcd`, expected `abce` or `abxx`"),
        "; near: abce"
    );
    assert_eq!(near_field("unknown field `abcd`, there are no fields"), "");
    assert_eq!(near_field("missing field `id`"), "");
}

#[test]
fn a_field_near_none_is_named_without_a_guess() {
    let line = problem(&files(&format!(
        "[[item]]\nid = \"lantern\"\nlike = \"{CHEST}\"\nglow = true\n"
    )));
    assert!(line.contains("unknown field `glow`"), "{line}");
    assert!(!line.contains("near:"), "{line}");
}
