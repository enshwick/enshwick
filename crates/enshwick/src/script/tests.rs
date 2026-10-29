use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::abi::{Header, ModHandle, PlayerData};
use crate::schedule::Scheduler;
use crate::world::{Notice, Player, World};
use crate::{Entity, PlayerKey};

fn record(ty: &Ty) -> &'static Record {
    match *ty {
        Ty::Record(record) => record,
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn every_event_of_the_tables_has_a_script_entry_named_as_in_rust() {
    let names: Vec<&str> = events().map(EventKind::name).collect();
    assert!(names.contains(&"PlayerJoined"));
    assert!(names.contains(&"WorldSaved"));
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), names.len());
    for event in events() {
        assert!(
            !event.doc().trim().is_empty(),
            "{} has no doc",
            event.name()
        );
    }
}

#[test]
fn event_data_leaves_out_the_header_and_reserved_fields() {
    for event in events() {
        if let Ty::Record(data) = event.data() {
            for field in data.fields() {
                assert_ne!(field.name(), "header", "{}", event.name());
                assert!(!field.name().starts_with("reserved"), "{}", event.name());
                assert!(!field.doc().is_empty(), "{}.{}", data.name(), field.name());
            }
        }
    }
    let joined = events()
        .find(|e| e.name() == "PlayerJoined")
        .map(EventKind::data);
    let fields: Vec<&str> = joined
        .map(record)
        .unwrap()
        .fields()
        .iter()
        .map(Field::name)
        .collect();
    assert_eq!(fields, ["slot", "entity", "key"]);
}

#[test]
fn an_event_reads_its_data_as_a_record_in_field_order() {
    let player = PlayerData {
        header: Header::of::<PlayerData>(),
        slot: 2,
        entity: Entity(9),
        key: PlayerKey::new(u64::MAX),
    };
    let joined = events().find(|e| e.name() == "PlayerJoined").unwrap();
    // SAFETY: `player` is a whole `PlayerData` on this stack frame, as the
    // loader passes with the event.
    let value = unsafe { joined.read((&raw const player).cast()) };
    let Some(Value::Record(fields)) = value else {
        panic!("expected a record, got {value:?}");
    };
    assert!(matches!(fields[1], Value::Number(n) if (n - 9.0).abs() < f64::EPSILON));
    assert!(matches!(&fields[2], Value::String(key) if *key == u64::MAX.to_string()));
}

#[test]
fn whole_numbers_refuse_fractions_and_values_out_of_range() {
    assert_eq!(u32::from_value(Value::Number(4.0)).unwrap(), 4);
    assert!(u32::from_value(Value::Number(4.5)).is_err());
    assert!(u32::from_value(Value::Number(-1.0)).is_err());
    assert!(i8::from_value(Value::Number(128.0)).is_err());
    assert!(u32::from_value(Value::String("4".into())).is_err());
    assert!(u32::from_value(Value::Number(f64::NAN)).is_err());
}

#[test]
fn keys_travel_as_their_decimal_digits() {
    let key = u64::MAX - 1;
    assert_eq!(u64::from_value(key.into_value()).unwrap(), key);
    assert!(u64::from_value(Value::String("x".into())).is_err());
}

#[test]
fn a_player_comes_back_from_its_record() {
    let player = Player {
        entity: Entity(5),
        slot: 1,
        key: PlayerKey::new(77),
        permissions: 3,
        name: "Ash".into(),
    };
    assert_eq!(
        Player::from_value(player.clone().into_value()).unwrap(),
        player
    );
    assert!(Player::from_value(Value::Record(vec![Value::Nil])).is_err());
}

#[test]
fn a_choice_is_its_variant_s_name() {
    assert!(matches!(Notice::Chat.into_value(), Value::String(s) if s == "Chat"));
    assert_eq!(
        Notice::from_value(Value::String("OnScreen".into())).unwrap(),
        Notice::OnScreen
    );
    assert!(Notice::from_value(Value::String("Elsewhere".into())).is_err());
}

#[test]
fn every_bound_function_has_a_doc_and_named_parameters() {
    let world: Vec<&str> = World::functions().map(Function::name).collect();
    for name in [
        "players",
        "player",
        "broadcast",
        "tell",
        "notify",
        "position",
    ] {
        assert!(world.contains(&name), "world lacks {name}");
    }
    let functions = World::functions()
        .map(|f| (f.name(), f.doc(), f.signature()))
        .chain(Scheduler::functions().map(|f| (f.name(), f.doc(), f.signature())));
    for (name, doc, signature) in functions {
        assert!(!doc.is_empty(), "{name}");
        assert!(
            signature.params().iter().all(|p| !p.name().is_empty()),
            "{name}"
        );
    }
    let players = World::functions().find(|f| f.name() == "players").unwrap();
    assert!(matches!(players.signature().ret(), Ty::List(Ty::Record(r)) if r.name() == "Player"));
}

#[test]
fn a_task_from_a_script_runs_until_it_answers_false() {
    let (scheduler, _) = Scheduler::get(ModHandle(41));
    let runs = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&runs);
    let task = Callback::new(move |args| {
        assert!(args.is_empty());
        let run = counted.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(Value::Bool(run < 2))
    });
    let every = Scheduler::functions()
        .find(|f| f.name() == "every")
        .unwrap();
    assert!(matches!(
        every.signature().params()[1].ty(),
        Ty::Function(s) if matches!(s.ret(), Ty::Optional(Ty::Bool))
    ));
    every
        .call(&scheduler, vec![Value::Number(0.0), Value::Function(task)])
        .unwrap();
    for _ in 0..4 {
        scheduler.run_due(1);
    }
    assert_eq!(runs.load(Ordering::Relaxed), 2);
}

#[test]
fn a_call_names_the_argument_of_the_wrong_type() {
    let (scheduler, _) = Scheduler::get(ModHandle(42));
    let after = Scheduler::functions()
        .find(|f| f.name() == "after")
        .unwrap();
    let error = after
        .call(&scheduler, vec![Value::String("soon".into())])
        .unwrap_err();
    assert!(error.to_string().starts_with("delay: "), "{error}");
    let error = after
        .call(&scheduler, vec![Value::Number(1.0), Value::Nil, Value::Nil])
        .unwrap_err();
    assert!(error.to_string().contains("takes 2 arguments"), "{error}");
}

#[test]
fn typed_game_events_have_unique_names_apart_from_loader_events() {
    let mut names: Vec<&str> = game_events().map(GameEventKind::name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), game_events().count());
    assert!(
        names
            .iter()
            .all(|n| !n.is_empty() && !events().any(|e| e.name() == *n))
    );
}

#[test]
fn a_mask_is_the_names_of_its_bits() {
    use crate::game_events::DamageTypeMask;
    let mask = DamageTypeMask::MAGICAL_FIRE | DamageTypeMask::POISON;
    let value = mask.into_value();
    assert!(matches!(&value, Value::List(names) if names.len() == 2));
    assert_eq!(DamageTypeMask::from_value(value).unwrap(), mask);
}

#[test]
fn a_game_value_is_its_name_and_an_unnamed_one_its_number() {
    use crate::game_events::ExperienceSourceType;
    assert!(matches!(ExperienceSourceType::Combat.into_value(), Value::String(s) if s == "Combat"));
    let other = ExperienceSourceType::Other(9);
    assert_eq!(
        ExperienceSourceType::from_value(other.into_value()).unwrap(),
        other
    );
}
