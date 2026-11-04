use proptest::prelude::*;

use super::*;
use crate::content::Guid;

/// A registry with every kind a resource holds, under `keen::Root`.
fn registry() -> Registry<'static> {
    Registry::new([
        Type::new("keen::bool", Kind::Bool, 1, 1),
        Type::new("keen::uint8", Kind::Uint8, 1, 1),
        Type::new("keen::sint16", Kind::Sint16, 2, 2),
        Type::new("keen::uint16", Kind::Uint16, 2, 2),
        Type::new("keen::uint32", Kind::Uint32, 4, 4),
        Type::new("keen::uint64", Kind::Uint64, 8, 8),
        Type::new("keen::sint64", Kind::Sint64, 8, 8),
        Type::new("keen::float32", Kind::Float32, 4, 4),
        Type::new("keen::Guid", Kind::Guid, 16, 4),
        Type::new(
            "keen::ObjectReference<keen::Item>",
            Kind::ObjectReference,
            16,
            4,
        )
        .with_inner("keen::Item"),
        Type::new("keen::BlobString", Kind::BlobString, 8, 4),
        Type::new("keen::Side", Kind::Enum, 1, 1).with_constants(vec![
            Constant::new("Left", 0),
            Constant::new("Right", 1),
            Constant::new("Up", 2),
        ]),
        Type::new("keen::Flag", Kind::Enum, 1, 1).with_constants(vec![
            Constant::new("Hot", 0),
            Constant::new("Cold", 1),
            Constant::new("Wet", 5),
        ]),
        Type::new("keen::Bitmask8<keen::Flag>", Kind::Bitmask8, 1, 1).with_inner("keen::Flag"),
        Type::new("keen::Count", Kind::Typedef, 4, 4).with_inner("keen::uint32"),
        Type::new("keen::StaticArray<keen::uint8,4>", Kind::StaticArray, 4, 1)
            .with_inner("keen::uint8")
            .with_count(4),
        Type::new("keen::BlobArray<keen::uint16>", Kind::BlobArray, 8, 4)
            .with_inner("keen::uint16"),
        Type::new("keen::Item", Kind::Struct, 16, 4).with_fields(vec![
            Field::new("label", 0, "keen::BlobString"),
            Field::new("values", 8, "keen::BlobArray<keen::uint16>"),
        ]),
        Type::new("keen::BlobArray<keen::Item>", Kind::BlobArray, 8, 4).with_inner("keen::Item"),
        Type::new("keen::BlobOptional<keen::uint32>", Kind::BlobOptional, 4, 4)
            .with_inner("keen::uint32"),
        Type::new("keen::Shape", Kind::Struct, 4, 4).with_fields(vec![Field::new(
            "id",
            0,
            "keen::uint32",
        )]),
        Type::new("keen::Sphere", Kind::Struct, 8, 4)
            .with_inner("keen::Shape")
            .with_fields(vec![Field::new("radius", 4, "keen::float32")]),
        Type::new("keen::Box", Kind::Struct, 4, 1).with_fields(vec![Field::new(
            "size",
            0,
            "keen::StaticArray<keen::uint8,4>",
        )]),
        Type::new("keen::BlobVariant<keen::Shape>", Kind::BlobVariant, 12, 4)
            .with_inner("keen::Shape"),
        Type::new("keen::Root", Kind::Struct, 96, 8).with_fields(vec![
            Field::new("flag", 0, "keen::bool"),
            Field::new("small", 1, "keen::uint8"),
            Field::new("delta", 2, "keen::sint16"),
            Field::new("count", 4, "keen::Count"),
            Field::new("big", 8, "keen::uint64"),
            Field::new("neg", 16, "keen::sint64"),
            Field::new("ratio", 24, "keen::float32"),
            Field::new("side", 28, "keen::Side"),
            Field::new("mask", 29, "keen::Bitmask8<keen::Flag>"),
            Field::new("pad", 30, "keen::uint16").untyped(),
            Field::new("id", 32, "keen::Guid"),
            Field::new("target", 48, "keen::ObjectReference<keen::Item>"),
            Field::new("name", 64, "keen::BlobString"),
            Field::new("items", 72, "keen::BlobArray<keen::Item>"),
            Field::new("maybe", 80, "keen::BlobOptional<keen::uint32>"),
            Field::new("shape", 84, "keen::BlobVariant<keen::Shape>"),
        ]),
    ])
}

fn item(label: &str, values: &[u16]) -> Value {
    Value::Struct(vec![
        ("label".into(), Value::Text(label.into())),
        (
            "values".into(),
            Value::List(values.iter().map(|v| Value::Uint(u64::from(*v))).collect()),
        ),
    ])
}

fn root(name: &str, items: Vec<Value>, maybe: Value, shape: Value) -> Value {
    Value::Struct(vec![
        ("flag".into(), Value::Bool(true)),
        ("small".into(), Value::Uint(200)),
        ("delta".into(), Value::Int(-3)),
        ("count".into(), Value::Uint(7)),
        ("big".into(), Value::Uint(u64::MAX)),
        ("neg".into(), Value::Int(i64::MIN)),
        ("ratio".into(), Value::Float(1.5)),
        ("side".into(), Value::Name("Up".into())),
        (
            "mask".into(),
            Value::Flags(vec!["Hot".into(), "Wet".into()]),
        ),
        ("id".into(), Value::Guid(Guid([1; 16]))),
        ("target".into(), Value::Guid(Guid([2; 16]))),
        ("name".into(), Value::Text(name.into())),
        ("items".into(), Value::List(items)),
        ("maybe".into(), maybe),
        ("shape".into(), shape),
    ])
}

fn sphere(id: u32, radius: f32) -> Value {
    Value::Variant(Box::new(Variant::new(
        "keen::Sphere",
        Value::Struct(vec![
            ("id".into(), Value::Uint(u64::from(id))),
            ("radius".into(), Value::Float(f64::from(radius))),
        ]),
    )))
}

#[test]
fn every_kind_writes_into_new_bytes_and_reads_back() {
    let registry = registry();
    let value = root(
        "chest",
        vec![item("a", &[1, 2]), item("", &[])],
        Value::Uint(9),
        sphere(4, 2.0),
    );
    let bytes = registry.encode("keen::Root", &value, None).unwrap();
    assert_eq!(registry.decode("keen::Root", &bytes).unwrap(), value);
}

#[test]
fn an_unchanged_value_writes_back_the_same_bytes() {
    let registry = registry();
    let value = root("chest", vec![item("abc", &[5])], Value::Nil, Value::Nil);
    let bytes = registry.encode("keen::Root", &value, None).unwrap();
    let decoded = registry.decode("keen::Root", &bytes).unwrap();
    assert_eq!(
        registry
            .encode("keen::Root", &decoded, Some(&bytes))
            .unwrap(),
        bytes
    );
}

#[test]
fn a_grown_array_moves_to_the_end_and_keeps_the_rest_in_place() {
    let registry = registry();
    let value = root("chest", vec![item("a", &[1])], Value::Nil, Value::Nil);
    let base = registry.encode("keen::Root", &value, None).unwrap();
    let mut grown = value.clone();
    let Some(Value::List(items)) = grown.field_mut("items") else {
        panic!("items")
    };
    items.push(item("second", &[2, 3]));
    let bytes = registry.encode("keen::Root", &grown, Some(&base)).unwrap();
    assert!(bytes.len() > base.len());
    assert_eq!(
        bytes[..64],
        base[..64],
        "the fields before the array keep their bytes"
    );
    assert_eq!(registry.decode("keen::Root", &bytes).unwrap(), grown);
}

#[test]
fn a_partial_struct_changes_only_its_fields() {
    let registry = registry();
    let value = root("chest", vec![], Value::Uint(1), sphere(1, 1.0));
    let base = registry.encode("keen::Root", &value, None).unwrap();
    let change = Value::Struct(vec![("small".into(), Value::Uint(3))]);
    let bytes = registry.encode("keen::Root", &change, Some(&base)).unwrap();
    let mut expected = value;
    *expected.field_mut("small").unwrap() = Value::Uint(3);
    assert_eq!(registry.decode("keen::Root", &bytes).unwrap(), expected);
    assert_eq!(bytes.len(), base.len());
}

#[test]
fn values_out_of_range_and_unknown_names_are_refused_with_their_path() {
    let registry = registry();
    let base = registry
        .encode(
            "keen::Root",
            &root("x", vec![], Value::Nil, Value::Nil),
            None,
        )
        .unwrap();
    for (field, value, message) in [
        (
            "small",
            Value::Uint(256),
            "small: 256 does not fit keen::uint8",
        ),
        (
            "delta",
            Value::Int(40_000),
            "delta: 40000 does not fit keen::sint16",
        ),
        (
            "side",
            Value::Name("Down".into()),
            "side: keen::Side has no constant Down",
        ),
        (
            "mask",
            Value::Flags(vec!["Dry".into()]),
            "mask: keen::Bitmask8<keen::Flag> has no flag Dry",
        ),
        ("nothing", Value::Nil, "keen::Root has no field nothing"),
        (
            "name",
            Value::Uint(1),
            "name: keen::BlobString (BlobString) does not take a number",
        ),
    ] {
        let change = Value::Struct(vec![(field.into(), value)]);
        let error = registry
            .encode("keen::Root", &change, Some(&base))
            .unwrap_err();
        assert_eq!(error.to_string(), message);
    }
    let nested = Value::Struct(vec![(
        "items".into(),
        Value::List(vec![Value::Struct(vec![(
            "values".into(),
            Value::List(vec![Value::Int(-1)]),
        )])]),
    )]);
    let error = registry
        .encode("keen::Root", &nested, Some(&base))
        .unwrap_err();
    assert_eq!(error.path(), "items[0].values[0]");
}

#[test]
fn unnamed_enum_values_and_flags_stay_numbers() {
    let registry = registry();
    let mut bytes = registry
        .encode(
            "keen::Root",
            &root("x", vec![], Value::Nil, Value::Nil),
            None,
        )
        .unwrap();
    bytes[28] = 9;
    bytes[29] = 0b100;
    bytes[0] = 2;
    let decoded = registry.decode("keen::Root", &bytes).unwrap();
    assert_eq!(decoded.field("side"), Some(&Value::Uint(9)));
    assert_eq!(decoded.field("mask"), Some(&Value::Uint(4)));
    assert_eq!(decoded.field("flag"), Some(&Value::Uint(2)));
    assert_eq!(
        registry
            .encode("keen::Root", &decoded, Some(&bytes))
            .unwrap(),
        bytes
    );
}

#[test]
fn an_array_past_the_end_stays_bytes_and_damaged_strings_fail() {
    let registry = registry();
    let mut bytes = registry
        .encode(
            "keen::Root",
            &root("x", vec![item("a", &[1])], Value::Nil, Value::Nil),
            None,
        )
        .unwrap();
    bytes[76..80].copy_from_slice(&u32::MAX.to_le_bytes());
    let decoded = registry.decode("keen::Root", &bytes).unwrap();
    assert_eq!(
        decoded.field("items"),
        Some(&Value::Bytes(bytes[72..80].to_vec()))
    );
    assert_eq!(
        registry
            .encode("keen::Root", &decoded, Some(&bytes))
            .unwrap(),
        bytes
    );
    bytes[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
    let error = registry.decode("keen::Root", &bytes).unwrap_err();
    assert!(
        error.to_string().contains("outside the resource"),
        "{error}"
    );
    assert!(registry.decode("keen::Root", &[0; 10]).is_err());
    assert!(registry.decode("keen::Missing", &[]).is_err());
}

#[test]
fn a_typedef_loop_and_deep_nesting_are_errors() {
    let registry = Registry::new([
        Type::new("keen::A", Kind::Typedef, 4, 4).with_inner("keen::B"),
        Type::new("keen::B", Kind::Typedef, 4, 4).with_inner("keen::A"),
        Type::new("keen::Deep", Kind::BlobOptional, 4, 4).with_inner("keen::Deep"),
    ]);
    assert!(registry.decode("keen::A", &[0; 4]).is_err());
    // Every optional points at the next one, four bytes on.
    let deep: Vec<u8> = (0..100).flat_map(|_| 4u32.to_le_bytes()).collect();
    let error = registry.decode("keen::Deep", &deep).unwrap_err();
    assert!(error.to_string().contains("deeper than 64"), "{error}");
}

#[test]
fn kinds_name_their_own_bytes() {
    for byte in 0..=u8::MAX {
        let kind = Kind::from_byte(byte);
        assert_eq!(kind.byte(), byte, "{kind:?}");
    }
}

#[test]
fn the_registry_finds_types_by_every_name_and_hash() {
    let registry = Registry::new([
        Type::new("keen::ecs::TemplateResource", Kind::Struct, 24, 4)
            .with_names("TemplateResource", "ecs.TemplateResource"),
    ]);
    let ty = registry.named("keen::ecs::TemplateResource").unwrap();
    assert_eq!(ty.namespace(), "keen::ecs");
    assert!(
        registry
            .hashed(fnv1a32(b"keen::ecs::TemplateResource"))
            .is_some()
    );
    assert!(registry.impact_named("ecs.TemplateResource").is_some());
    assert!(
        registry
            .impact_hashed(fnv1a32(b"ecs.TemplateResource"))
            .is_some()
    );
    let array = Type::new("keen::BlobArray<keen::ecs::Thing>", Kind::BlobArray, 8, 4);
    assert_eq!(array.name(), "BlobArray<keen::ecs::Thing>");
    assert_eq!(array.namespace(), "keen");
}

fn label() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-zA-Z0-9 äöü€]{0,12}").unwrap()
}

fn items() -> impl Strategy<Value = Vec<Value>> {
    proptest::collection::vec(
        (label(), proptest::collection::vec(any::<u16>(), 0..5))
            .prop_map(|(label, values)| item(&label, &values)),
        0..4,
    )
}

fn arbitrary_root() -> impl Strategy<Value = Value> {
    (
        (any::<bool>(), any::<u8>(), any::<i16>(), any::<u32>()),
        (
            any::<u64>(),
            any::<i64>(),
            any::<f32>().prop_filter("finite", |f| f.is_finite()),
        ),
        (
            0..3usize,
            proptest::sample::subsequence(vec!["Hot", "Cold", "Wet"], 0..=3),
        ),
        (any::<[u8; 16]>(), any::<[u8; 16]>(), label(), items()),
        (
            proptest::option::of(any::<u32>()),
            proptest::option::of((
                any::<u32>(),
                any::<f32>().prop_filter("finite", |f| f.is_finite()),
            )),
        ),
    )
        .prop_map(
            |(
                (flag, small, delta, count),
                (big, neg, ratio),
                (side, mask),
                (id, target, name, items),
                (maybe, shape),
            )| {
                Value::Struct(vec![
                    ("flag".into(), Value::Bool(flag)),
                    ("small".into(), Value::Uint(u64::from(small))),
                    ("delta".into(), Value::Int(i64::from(delta))),
                    ("count".into(), Value::Uint(u64::from(count))),
                    ("big".into(), Value::Uint(big)),
                    ("neg".into(), Value::Int(neg)),
                    ("ratio".into(), Value::Float(f64::from(ratio))),
                    (
                        "side".into(),
                        Value::Name(["Left", "Right", "Up"][side].into()),
                    ),
                    (
                        "mask".into(),
                        Value::Flags(mask.into_iter().map(String::from).collect()),
                    ),
                    ("id".into(), Value::Guid(Guid(id))),
                    ("target".into(), Value::Guid(Guid(target))),
                    ("name".into(), Value::Text(name)),
                    ("items".into(), Value::List(items)),
                    (
                        "maybe".into(),
                        maybe.map_or(Value::Nil, |m| Value::Uint(u64::from(m))),
                    ),
                    (
                        "shape".into(),
                        shape.map_or(Value::Nil, |(id, radius)| sphere(id, radius)),
                    ),
                ])
            },
        )
}

proptest! {
    #[test]
    fn any_value_reads_back_as_written(value in arbitrary_root()) {
        let registry = registry();
        let bytes = registry.encode("keen::Root", &value, None).unwrap();
        prop_assert_eq!(registry.decode("keen::Root", &bytes).unwrap(), value);
    }

    #[test]
    fn any_edit_over_a_base_reads_back_and_an_unchanged_one_keeps_the_bytes(
        first in arbitrary_root(),
        second in arbitrary_root(),
    ) {
        let registry = registry();
        let base = registry.encode("keen::Root", &first, None).unwrap();
        let same = registry.encode("keen::Root", &first, Some(&base)).unwrap();
        prop_assert_eq!(&same, &base);
        let edited = registry.encode("keen::Root", &second, Some(&base)).unwrap();
        prop_assert_eq!(registry.decode("keen::Root", &edited).unwrap(), second);
    }

    #[test]
    fn any_bytes_decode_or_fail_without_panicking(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let registry = registry();
        if let Ok(value) = registry.decode("keen::Root", &bytes) {
            let again = registry.encode("keen::Root", &value, Some(&bytes)).unwrap();
            prop_assert_eq!(again, bytes);
        }
    }
}
