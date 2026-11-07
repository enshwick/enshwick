//! The fields whose data lies outside a live component's own bytes, which a
//! typed read of the component does not reach.

use crate::resources::{Kind, Registry, Type};

/// Nesting deeper than this is taken for a loop in bad data, as the codec
/// takes it.
const MAX_DEPTH: usize = 64;

/// The path, by the game's field names, of the first field of `ty` at any
/// depth of its own bytes whose data lies outside them: a blob kind, whose
/// offset points past them, or a designer data kind, which points to the
/// heap.
pub(super) fn outside_field(registry: &Registry<'_>, ty: &Type<'_>) -> Option<String> {
    outside(registry, ty, 0, &mut Vec::new())
}

/// `seen` holds every type walked so far, none of which holds such a field:
/// a type met again adds nothing, so a type holding itself twice ends.
fn outside(
    registry: &Registry<'_>,
    ty: &Type<'_>,
    depth: usize,
    seen: &mut Vec<*const ()>,
) -> Option<String> {
    let at = core::ptr::from_ref(ty).cast::<()>();
    if depth > MAX_DEPTH || seen.contains(&at) {
        return None;
    }
    seen.push(at);
    let join = |head: &str, rest: String| {
        if rest.is_empty() {
            head.to_owned()
        } else {
            format!("{head}.{rest}")
        }
    };
    let inner = || ty.inner().and_then(|name| registry.named(name));
    match ty.kind() {
        Kind::BlobArray
        | Kind::BlobString
        | Kind::BlobOptional
        | Kind::BlobVariant
        | Kind::DsArray
        | Kind::DsString
        | Kind::DsOptional
        | Kind::DsVariant
        // A kind this API does not know may keep its data anywhere.
        | Kind::Other(_) => Some(String::new()),
        Kind::Typedef => outside(registry, inner()?, depth + 1, seen),
        Kind::Struct => registry.all_fields(ty).into_iter().find_map(|field| {
            let held = registry.named(field.type_name()?)?;
            outside(registry, held, depth + 1, seen).map(|rest| join(field.name(), rest))
        }),
        Kind::StaticArray => {
            outside(registry, inner()?, depth + 1, seen).map(|rest| join("0", rest))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::Field;

    #[test]
    fn the_first_field_whose_data_lies_outside_a_type_is_named_by_its_path() {
        let registry = Registry::new([
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::Level", Kind::Typedef, 1, 1).with_inner("demo::uint8"),
            Type::new("demo::BlobArray<demo::uint8>", Kind::BlobArray, 8, 4)
                .with_inner("demo::uint8"),
            Type::new("demo::Inner", Kind::Struct, 8, 4).with_fields(vec![Field::new(
                "entries",
                0,
                "demo::BlobArray<demo::uint8>",
            )]),
            Type::new("demo::StaticArray<demo::Inner,2>", Kind::StaticArray, 16, 4)
                .with_inner("demo::Inner")
                .with_count(2),
            Type::new("demo::Outer", Kind::Struct, 20, 4).with_fields(vec![
                Field::new("level", 0, "demo::Level"),
                Field::new("pairs", 4, "demo::StaticArray<demo::Inner,2>"),
            ]),
            Type::new("demo::Plain", Kind::Struct, 1, 1).with_fields(vec![Field::new(
                "level",
                0,
                "demo::Level",
            )]),
        ]);
        let outside = |name| outside_field(&registry, registry.named(name).unwrap());
        assert_eq!(outside("demo::Outer").as_deref(), Some("pairs.0.entries"));
        assert_eq!(outside("demo::Inner").as_deref(), Some("entries"));
        assert_eq!(outside("demo::Plain"), None);
        assert_eq!(outside("demo::Level"), None);
    }

    #[test]
    fn a_typedef_is_followed_a_loop_ends_and_an_unknown_kind_counts_as_outside() {
        // Forty fields of its own type, walked to the depth limit, never end.
        let branches: Vec<String> = (0..40).map(|i| format!("branch{i}")).collect();
        let registry = Registry::new([
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::BlobArray<demo::uint8>", Kind::BlobArray, 8, 4)
                .with_inner("demo::uint8"),
            Type::new("demo::Entries", Kind::Typedef, 8, 4)
                .with_inner("demo::BlobArray<demo::uint8>"),
            Type::new("demo::Named", Kind::Struct, 8, 4).with_fields(vec![Field::new(
                "entries",
                0,
                "demo::Entries",
            )]),
            Type::new("demo::Loop", Kind::Typedef, 1, 1).with_inner("demo::Loop"),
            Type::new("demo::Nest", Kind::Struct, 1, 1).with_fields(vec![Field::new(
                "again",
                0,
                "demo::Nest",
            )]),
            Type::new("demo::Tree", Kind::Struct, 2, 1).with_fields(
                branches
                    .iter()
                    .map(|name| Field::new(name, 0, "demo::Tree"))
                    .collect(),
            ),
            Type::new("demo::Unknown", Kind::Other(0x0b), 4, 4),
            Type::new("demo::Odd", Kind::Struct, 4, 4).with_fields(vec![Field::new(
                "mystery",
                0,
                "demo::Unknown",
            )]),
        ]);
        let outside = |name| outside_field(&registry, registry.named(name).unwrap());
        assert_eq!(outside("demo::Named").as_deref(), Some("entries"));
        assert_eq!(outside("demo::Loop"), None);
        assert_eq!(outside("demo::Nest"), None);
        assert_eq!(outside("demo::Tree"), None);
        assert_eq!(outside("demo::Odd").as_deref(), Some("mystery"));
    }
}
