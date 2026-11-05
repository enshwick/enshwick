//! `rewards.toml`: rewards read, resolved and checked; the `on` table of an
//! item in `items.toml`.

use super::super::rewards::{LimitEntry, PackGive, PackOn};
use super::super::values::Number;
use super::*;

const BUFF: &str = "0d1e2f30-4152-4637-8899-aabbccddeeff";

fn pack(rewards: &str) -> Result<Pack, ParsePackError> {
    let items = format!("[[item]]\nid = \"tonic\"\nlike = \"{CHEST}\"\n");
    let recipes = format!("[[recipe]]\nid = \"brew\"\nlike = \"{CHEST}\"\noutput = \"tonic\"\n");
    Pack::parse_with(
        "you.tonics",
        &PackFiles::new()
            .with(ITEMS_FILE, items)
            .with(RECIPES_FILE, recipes)
            .with(REWARDS_FILE, rewards),
        None,
    )
}

#[test]
fn a_reward_resolves_the_pack_s_names_and_reads_each_gift() {
    let text = format!(
        r#"
        [[reward]]
        id = "first-brew"
        on = {{ consume = "tonic" }}
        give = [{{ item = "tonic", count = 2 }}, {{ experience = 50 }}, {{ buff = "{BUFF}" }}]
        limit = "once"
        chance = 0.5
        "#
    );
    let pack = pack(&text).unwrap();
    let reward = &pack.rewards[0];
    let tonic = Guid::named("you.tonics:tonic");
    assert_eq!(reward.on, PackOn::Consume(tonic));
    assert_eq!(
        reward.give,
        [
            PackGive::Items(tonic, Number::Fixed(2)),
            PackGive::Experience(Number::Fixed(50)),
            PackGive::Buff(Guid::parse(BUFF))
        ]
    );
    assert_eq!(
        (reward.limit, reward.chance.clone()),
        (LimitEntry::Once, Some(Number::Fixed(0.5)))
    );
}

#[test]
fn a_reward_with_no_action_two_actions_a_bad_chance_or_no_gift_is_an_error_at_its_line() {
    let text = r#"
[[reward]]
id = "none"
on = {}
give = [{ experience = 1 }]

[[reward]]
id = "two"
on = { join = true, loot = true }
give = [{ experience = 1 }]

[[reward]]
id = "odds"
on = { level_up = 0 }
give = [{ experience = 1 }]
chance = 2.0

[[reward]]
id = "empty"
on = { join = true }
give = []

[[reward]]
id = "Bad Name"
on = { quest = true }
give = [{ knowledge = "not a guid" }]
"#;
    let errors = pack(text).unwrap_err().to_string();
    for (line, why) in [
        (2, "reward none: `on` names one of"),
        (7, "reward two: `on` names one of"),
        (12, "reward odds: a chance is from 0 to 1"),
        (18, "reward empty: it gives nothing"),
        (23, "reward Bad Name: a name is 1 to 64"),
        (23, "\"not a guid\" is no GUID"),
    ] {
        assert!(
            errors.contains(&format!("rewards.toml:{line}")) && errors.contains(why),
            "{why} at {line}: {errors}"
        );
    }
}

#[test]
fn a_craft_names_a_recipe_and_an_unknown_one_is_an_error() {
    let ok = pack(
        "[[reward]]\nid = \"brewed\"\non = { craft = \"brew\" }\ngive = [{ experience = 5 }]\n",
    )
    .unwrap();
    assert_eq!(
        ok.rewards[0].on,
        PackOn::Craft(Guid::named("you.tonics:brew"))
    );
    let errors =
        pack("[[reward]]\nid = \"x\"\non = { craft = \"nothing\" }\ngive = [{ experience = 5 }]\n")
            .unwrap_err()
            .to_string();
    assert!(errors.contains("rewards.toml:1"), "{errors}");
}

#[test]
fn a_number_may_name_a_setting_of_pack_toml_of_its_type_and_no_other() {
    let manifest = "id = \"you.tonics\"\nversion = \"1.0.0\"\n\n[[setting]]\nkey = \"bonus\"\ndefault = 5\n\n[[setting]]\nkey = \"word\"\ndefault = \"x\"\n";
    let parse = |rewards: &str, loot: &str| {
        Pack::parse_with(
            "you.tonics",
            &PackFiles::new()
                .with(MANIFEST_FILE, manifest)
                .with(REWARDS_FILE, rewards)
                .with(LOOT_FILE, loot),
            None,
        )
    };
    let reward = "[[reward]]\nid = \"b\"\non = { join = true }\ngive = [{ experience = { setting = \"bonus\" } }]\n";
    let drop = format!(
        "[[drop]]\nitems = [\"{CHEST}\"]\nchance = {{ setting = \"bonus\" }}\nfrom = \"chests\"\n"
    );
    let errors = parse(reward, &drop).unwrap_err().to_string();
    assert!(
        errors.contains("chance 5 is not between 0 and 1"),
        "{errors}"
    );
    let pack = parse(reward, "").unwrap();
    assert_eq!(
        pack.rewards[0].give,
        [PackGive::Experience(Number::Setting {
            key: "bonus".to_owned(),
            default: 5
        })]
    );
    assert!(pack.settings_schema.is_some());
    let unknown = reward.replace("bonus", "missing");
    let errors = parse(&unknown, "").unwrap_err().to_string();
    assert!(
        errors.contains("setting \"missing\" is no [[setting]] of pack.toml"),
        "{errors}"
    );
    let wrong = reward.replace("bonus", "word");
    let errors = parse(&wrong, "").unwrap_err().to_string();
    assert!(
        errors.contains("setting \"word\" is not a whole number"),
        "{errors}"
    );
}

#[test]
fn an_item_s_on_table_needs_luau_names_and_a_main_luau() {
    let items = format!(
        "[[item]]\nid = \"tonic\"\nlike = \"{CHEST}\"\non = {{ consume = \"2drink\", equip = \"wear\" }}\n"
    );
    let errors = Pack::parse_with(
        "you.tonics",
        &PackFiles::new().with(ITEMS_FILE, items),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(
        errors.contains("\"2drink\" is no Luau function name"),
        "{errors}"
    );
    assert!(
        errors.contains("`on` names wear, but the pack has no main.luau"),
        "{errors}"
    );
    let bad_key =
        format!("[[item]]\nid = \"tonic\"\nlike = \"{CHEST}\"\non = {{ drink = \"f\" }}\n");
    let files = PackFiles::new()
        .with(ITEMS_FILE, bad_key)
        .with(SCRIPT_MAIN, "");
    assert!(
        Pack::parse_with("you.tonics", &files, None).is_err(),
        "an unknown kind"
    );
}
