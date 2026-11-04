#![expect(
    unreachable_pub,
    reason = "the macros give test items the generated items' visibility"
)]

use super::*;
use crate::resources::{Field, Kind};

reflected_enum! {
    /// A test enum.
    Shade {
        /// Light.
        Light = "Light",
        /// Dark.
        Dark = "Dark",
    }
}

reflected_struct! {
    /// A test base.
    Base {
        /// A level.
        level: u8 = "level",
    }
}

reflected_struct! {
    /// A test type derived from `Base`.
    Derived: Base {
        /// A shade.
        shade: Shade = "shade",
        /// Whether it is on.
        on: bool = "on",
    }
}

fn derived() -> Value {
    Value::Struct(vec![
        ("level".into(), Value::Uint(3)),
        ("shade".into(), Value::Name("Dark".into())),
        ("on".into(), Value::Bool(true)),
    ])
}

#[test]
fn a_derived_struct_reads_its_base_and_writes_it_back_first() {
    let value = Derived::from_value(&derived()).unwrap();
    assert_eq!(value.base.level, 3);
    assert_eq!(value.shade, Shade::Dark);
    assert!(value.on);
    assert_eq!(value.to_value(), derived());
}

#[test]
fn a_missing_field_fails_the_check_and_names_the_field() {
    assert!(check::<Derived>(&derived()).is_ok());
    let Value::Struct(mut fields) = derived() else {
        unreachable!()
    };
    fields.remove(1);
    let error = check::<Derived>(&Value::Struct(fields)).unwrap_err();
    assert_eq!(error.to_string(), "shade: a field this build lacks");
}

#[test]
fn a_constant_the_enum_does_not_name_stays_as_the_game_gave_it() {
    for value in [Value::Name("Dusk".into()), Value::Uint(9)] {
        let shade = Shade::from_value(&value).unwrap();
        assert!(matches!(shade, Shade::Other(_)));
        assert_eq!(shade.to_value(), value);
    }
    assert!(Shade::from_value(&Value::Bool(true)).is_err());
}

#[test]
fn a_bool_reads_from_the_game_s_bool_or_byte() {
    assert!(bool::from_value(&Value::Bool(true)).unwrap());
    assert!(!bool::from_value(&Value::Bool(false)).unwrap());
    assert!(bool::from_value(&Value::Uint(2)).unwrap());
    assert!(bool::from_value(&Value::Text("y".into())).is_err());
}

#[test]
fn flags_write_names_or_bits_and_refuse_a_mix() {
    let named = Value::Flags(vec!["Dark".into()]);
    let flags = Flags::<Shade>::from_value(&named).unwrap();
    assert_eq!(flags.0, [Shade::Dark]);
    assert_eq!(flags.to_value(), named);
    let numbered = Flags::<Shade>::from_value(&Value::Uint(0b101)).unwrap();
    assert_eq!(numbered.to_value(), Value::Uint(0b101));
    let mixed = Flags(vec![Shade::Dark, Shade::Other(Constant::Number(5))]);
    assert_eq!(
        mixed.to_value(),
        Value::Flags(vec!["Dark".into(), "Uint(5)".into()])
    );
}

#[test]
fn a_sample_reaches_into_arrays_and_optionals() {
    let registry = Registry::new([
        Type::new("demo::uint8", Kind::Uint8, 1, 1),
        Type::new("demo::Base", Kind::Struct, 1, 1).with_fields(vec![Field::new(
            "rank",
            0,
            "demo::uint8",
        )]),
        Type::new("demo::Bases", Kind::BlobArray, 8, 4).with_inner("demo::Base"),
        Type::new("demo::Holder", Kind::Struct, 8, 4).with_fields(vec![Field::new(
            "bases",
            0,
            "demo::Bases",
        )]),
    ]);
    let sample = sample(&registry, "demo::Holder").unwrap();
    let Some(Value::List(items)) = sample.field("bases") else {
        panic!("{sample:?}")
    };
    assert_eq!(items.len(), 1);
    let error = <Vec<Base>>::from_value(&Value::List(items.clone())).unwrap_err();
    assert_eq!(error.to_string(), "0.level: a field this build lacks");
    assert!(super::sample(&registry, "demo::Missing").is_none());
}
