use super::*;
use crate::reflect::{Image, Layout};

#[test]
fn the_relevance_rule_reads() {
    let relevance = Relevance::read();
    assert_eq!(
        relevance.roots.len() + relevance.suffixes.len(),
        Role::ALL.len()
    );
}

/// The catalogue of the server executable `ENSH_SERVER_EXE` names, when it
/// names one of `age`; `None` to skip.
fn server(age: u32) -> Option<Catalogue> {
    let path = std::env::var("ENSH_SERVER_EXE").ok()?;
    let bytes = std::fs::read(path).unwrap();
    let image = Image::parse(&bytes, Layout::File, None).unwrap();
    (image.fingerprint()?.age() == age)
        .then(|| Catalogue::from_reflection(&Reflection::scan(&image)))
}

/// The API coverage count was made on server build 387: 648 components,
/// 127 events and 211 resources.
#[test]
fn build_387_covers_the_counted_types() {
    let Some(catalogue) = server(387) else {
        return;
    };
    let counts = Role::ALL.map(|role| catalogue.count(role));
    assert_eq!(counts, [648, 127, 211]);
}

/// A catalogue with one type of each role and every kind the writer maps.
const FIXTURE: &str = include_str!("fixture.jsonl");

fn fixture() -> Catalogue {
    Catalogue::from_snapshot(FIXTURE).unwrap()
}

#[test]
fn the_generated_api_of_a_fixture_build_reads_as_written() {
    let catalogue = fixture();
    let files = catalogue.rust("demo build 1").unwrap();
    for (path, text) in &files {
        insta::assert_snapshot!(path.replace(['/', '.'], "_"), text);
    }
    insta::assert_snapshot!(
        catalogue
            .ids_file("demo build 1", &Renames::default())
            .unwrap()
    );
}

#[test]
fn a_snapshot_read_back_writes_the_same_bytes() {
    let catalogue = fixture();
    let again = Catalogue::from_snapshot(&catalogue.snapshot()).unwrap();
    assert_eq!(again, catalogue);
    assert_eq!(again.rust("b").unwrap(), catalogue.rust("b").unwrap());
    assert_eq!(again.snapshot(), catalogue.snapshot());
}

#[test]
fn roles_follow_bases_suffixes_and_the_left_out_names() {
    let mut catalogue = fixture();
    for ty in catalogue.types.values_mut() {
        ty.roles.clear();
    }
    let relevance = Relevance {
        roots: BTreeMap::from([
            (Role::Component, "demo::ecs::Component".to_owned()),
            (Role::Event, "demo::ecs::Event".to_owned()),
        ]),
        suffixes: BTreeMap::from([(Role::Resource, "Resource".to_owned())]),
        left_out: LeftOut {
            namespaces: vec!["demo::ds::".to_owned()],
            words: vec!["Rang".to_owned()],
            prefixes: vec!["Dev".to_owned()],
        },
    };
    catalogue.assign_roles(&relevance);
    let named = |role| catalogue.names(role).collect::<Vec<_>>();
    assert_eq!(named(Role::Component), ["demo::ecs::Fuel"]);
    assert!(named(Role::Event).is_empty(), "left out by a word");
    assert_eq!(named(Role::Resource), ["demo::LootResource"]);
    assert!(relevance.left_out("demo::DevLootResource"));
    assert!(
        !relevance.left_out("demo::DeviceResource"),
        "a word, not the prefix"
    );
    assert!(relevance.left_out("demo::ds::LootResource"));
}

#[test]
fn an_id_names_the_engine_type_and_nothing_else() {
    let ids = fixture().ids().unwrap();
    assert_eq!(
        ids,
        [
            ("LootResource".to_owned(), "demo::LootResource".to_owned()),
            ("ecs::Fuel".to_owned(), "demo::ecs::Fuel".to_owned()),
            ("ecs::Rang".to_owned(), "demo::ecs::Rang".to_owned()),
        ]
    );
}

#[test]
fn two_types_with_one_rust_name_are_refused() {
    let mut catalogue = fixture();
    let mut twin = catalogue.types["demo::ecs::Fuel"].clone();
    twin.name = "demo::ecs::fuel".to_owned();
    catalogue.types.insert(twin.name.clone(), twin);
    let error = catalogue.rust("b").unwrap_err().to_string();
    assert!(
        error.contains("demo::ecs::Fuel and demo::ecs::fuel"),
        "{error}"
    );
}

/// The fixture as a later build changes it: one change of each case.
fn next_build() -> Catalogue {
    let mut new = fixture();
    let rename = |t: &mut Ty, to: &str| t.name = to.to_owned();
    let mut loot = new.types.remove("demo::LootResource").unwrap();
    rename(&mut loot, "demo::LootTableResource");
    new.types.insert(loot.name.clone(), loot);
    new.types.remove("demo::ItemId");
    let mut torch = new.types["demo::ecs::Component"].clone();
    rename(&mut torch, "demo::ecs::Torch");
    new.types.insert(torch.name.clone(), torch);
    let fuel = new.types.get_mut("demo::ecs::Fuel").unwrap();
    fuel.fields[0].name = "fuelAmount".to_owned();
    let rang = new.types.get_mut("demo::ecs::Rang").unwrap();
    rang.fields[1].offset = 10;
    new.types.get_mut("demo::ecs::Event").unwrap().size = 16;
    let faction = new.types.get_mut("demo::ecs::Faction").unwrap();
    faction.values[1].1 = 5;
    faction.values.push(("Neutral".to_owned(), 6));
    new
}

#[test]
fn each_change_between_builds_is_classified_by_its_case() {
    let diff = fixture().diff(&next_build(), &Renames::default());
    let of = |name: &str| diff.changes.get(name).cloned().unwrap_or_default();
    assert_eq!(
        of("demo::LootResource"),
        [Change::Renamed {
            to: "demo::LootTableResource".to_owned()
        }]
    );
    assert!(
        !diff.changes.contains_key("demo::LootTableResource"),
        "not added too"
    );
    assert_eq!(of("demo::ItemId"), [Change::Removed]);
    assert_eq!(of("demo::ecs::Torch"), [Change::Added]);
    assert_eq!(
        of("demo::ecs::Fuel"),
        [Change::FieldRenamed {
            from: "amount".to_owned(),
            to: "fuelAmount".to_owned()
        }]
    );
    assert_eq!(
        of("demo::ecs::Rang"),
        [Change::FieldMoved {
            field: "allies".to_owned(),
            from: 9,
            to: 10
        }]
    );
    assert!(matches!(
        of("demo::ecs::Event")[..],
        [Change::Retyped { .. }]
    ));
    assert_eq!(
        of("demo::ecs::Faction"),
        [
            Change::ConstantRenumbered {
                name: "Enemy".to_owned(),
                from: 1,
                to: 5
            },
            Change::ConstantAdded("Neutral".to_owned()),
        ]
    );
    assert!(diff.needs_person(), "a retyped type");
    insta::assert_snapshot!(diff.to_string());
}

#[test]
fn a_rename_two_layouts_could_take_is_left_to_a_person() {
    let old = fixture();
    let mut new = fixture();
    let loot = new.types.remove("demo::LootResource").unwrap();
    for name in ["demo::ARes", "demo::BRes"] {
        let mut copy = loot.clone();
        copy.name = name.to_owned();
        new.types.insert(copy.name.clone(), copy);
    }
    let diff = old.diff(&new, &Renames::default());
    assert!(
        matches!(&diff.changes["demo::LootResource"][..], [Change::Unsure { candidates }] if candidates.len() == 2)
    );
    let decided =
        Renames::parse("[[types]]\nfrom = \"demo::LootResource\"\nto = \"demo::BRes\"\n").unwrap();
    let diff = old.diff(&new, &decided);
    assert_eq!(
        diff.changes["demo::LootResource"],
        [Change::Renamed {
            to: "demo::BRes".to_owned()
        }]
    );
    assert_eq!(diff.changes["demo::ARes"], [Change::Added]);
}

#[test]
fn a_renamed_type_resolves_under_its_old_id() {
    let new = next_build();
    let renames = Renames::default().with(&fixture().diff(&new, &Renames::default()));
    let table = new.ids_file("b", &renames).unwrap();
    assert!(
        table.contains("\"LootResource\" => \"demo::LootTableResource\""),
        "{table}"
    );
    assert!(table.contains("\"LootTableResource\" => \"demo::LootTableResource\""));
    assert_eq!(
        Renames::parse(&renames.to_toml()).unwrap(),
        renames,
        "the table reads back"
    );
}

/// The real change from server build 387 to the committed snapshot's build,
/// when `ENSH_SERVER_EXE` names build 387; its counts go on the stability
/// rules' ticket.
#[test]
fn the_previous_build_diffs_against_the_committed_one() {
    let Some(old) = server(387) else {
        return;
    };
    let diff = old.diff(&committed(), &Renames::default());
    let report = diff.to_string();
    std::fs::write(std::env::temp_dir().join("typed-387-diff.txt"), &report).unwrap();
    assert!(!diff.changes.is_empty());
}

#[test]
fn every_id_of_the_committed_table_names_one_engine_type() {
    let ids: Vec<&str> = ids::IDS.iter().map(|(id, _)| *id).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
    for (id, name) in ids::IDS {
        assert_eq!(engine_name(id), Some(*name));
    }
}

/// The snapshot the committed typed API was generated from: the newest
/// server build's.
fn committed() -> Catalogue {
    let tables: Vec<crate::Table> = crate::embedded()
        .iter()
        .map(|(_, text)| crate::Table::parse(text).unwrap())
        .collect();
    let table = crate::newest(&tables, crate::Side::Server).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("typed")
        .join(std::path::Path::new(&table.file_name()).with_extension("jsonl"));
    Catalogue::from_snapshot(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Every covered type of the committed snapshot has a generated item, and
/// a zeroed value of the build's type reads as it: the shape check each
/// handle runs at resolve holds for every one.
#[test]
#[cfg_attr(miri, ignore = "decodes thousands of types")]
fn every_covered_type_reads_as_its_generated_item() {
    let catalogue = committed();
    let registry = catalogue.registry();
    let named = enshwick::typed::NAMED;
    let covered = catalogue
        .types
        .values()
        .filter(|t| !t.roles.is_empty() && t.kind != Kind::Typedef)
        .count();
    assert_eq!(named.len(), covered);
    assert!(named.iter().all(|(id, _)| engine_name(id).is_some()));
    let mut wrong = Vec::new();
    for (id, check) in named {
        let name = engine_name(id).unwrap();
        let ty = registry.named(name).unwrap();
        let zero = vec![0; usize::try_from(ty.size()).unwrap()];
        let read = registry
            .decode(name, &zero)
            .map_err(|e| e.to_string())
            .and_then(|value| check(&value).map_err(|e| e.to_string()));
        if let Err(why) = read {
            wrong.push(format!("{id}: {why}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

proptest::proptest! {
    /// Every kind byte maps to a Rust type with balanced brackets.
    #[test]
    fn every_kind_maps_to_a_rust_type(byte: u8, inner_byte: u8) {
        let mut catalogue = fixture();
        let probe = Ty {
            name: "demo::Probe".to_owned(),
            kind: Kind::from_byte(byte),
            size: 8,
            align: 4,
            base: None,
            inner: Some("demo::Inner".to_owned()),
            count: Some(2),
            fields: Vec::new(),
            values: Vec::new(),
            attributes: Vec::new(),
            roles: Vec::new(),
        };
        let mut inner = probe.clone();
        inner.name = "demo::Inner".to_owned();
        inner.kind = Kind::from_byte(inner_byte);
        inner.inner = Some("demo::Probe".to_owned());
        catalogue.types.insert(probe.name.clone(), probe);
        catalogue.types.insert(inner.name.clone(), inner);
        let text = rust::type_of(&catalogue, "demo::Probe");
        proptest::prop_assert!(!text.is_empty());
        proptest::prop_assert_eq!(text.matches('<').count(), text.matches('>').count());
    }

    /// Any name cases into a Rust identifier.
    #[test]
    fn any_name_cases_into_an_identifier(name in "[A-Za-z0-9_<>:,]{0,24}") {
        for cased in [names::camel(&name), names::snake(&name), names::shout(&name)] {
            let bare = cased.trim_start_matches("r#");
            proptest::prop_assert!(!bare.is_empty());
            proptest::prop_assert!(!bare.starts_with(|c: char| c.is_ascii_digit()));
            proptest::prop_assert!(bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        }
    }
}
