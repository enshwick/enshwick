//! The fields whose data lies outside a live component's own bytes: the
//! blob kinds, which a typed read reaches in the component's set-up data,
//! and the kinds it never reaches.

use crate::resources::{Kind, Registry, Type};

/// The path, by the game's field names, of the first field of `ty` at any
/// depth of its own bytes whose data a read of the live component does not
/// reach: a designer data kind, which points to the heap, or a kind this
/// API does not know, which may keep its data anywhere.
pub(super) fn outside_field(registry: &Registry<'_>, ty: &Type<'_>) -> Option<String> {
    let unreached = |kind| {
        matches!(
            kind,
            Kind::DsArray | Kind::DsString | Kind::DsOptional | Kind::DsVariant | Kind::Other(_)
        )
    };
    walk(registry, ty, &unreached, &mut Vec::new())
}

/// The path of the first field of `ty` at any depth of its own bytes whose
/// data follows the component in its set-up data: a blob kind, whose offset
/// points past the component's own bytes.
pub(super) fn blob_field(registry: &Registry<'_>, ty: &Type<'_>) -> Option<String> {
    let blob = |kind| {
        matches!(
            kind,
            Kind::BlobArray | Kind::BlobString | Kind::BlobOptional | Kind::BlobVariant
        )
    };
    walk(registry, ty, &blob, &mut Vec::new())
}

/// The path of the first field of `ty` of a kind `wanted` takes. `seen`
/// holds every type walked so far, none of which holds such a field: a type
/// met again adds nothing, so a loop ends and each type is walked once.
fn walk(
    registry: &Registry<'_>,
    ty: &Type<'_>,
    wanted: &dyn Fn(Kind) -> bool,
    seen: &mut Vec<*const ()>,
) -> Option<String> {
    let at = core::ptr::from_ref(ty).cast::<()>();
    if seen.contains(&at) {
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
        kind if wanted(kind) => Some(String::new()),
        // A variant may hold a type derived from its inner one; the inner
        // one's fields are the ones a read is sure to meet.
        Kind::Typedef | Kind::BlobOptional | Kind::BlobVariant => {
            walk(registry, inner()?, wanted, seen)
        }
        Kind::Struct => registry.all_fields(ty).into_iter().find_map(|field| {
            let held = registry.named(field.type_name()?)?;
            walk(registry, held, wanted, seen).map(|rest| join(field.name(), rest))
        }),
        Kind::StaticArray | Kind::BlobArray => {
            walk(registry, inner()?, wanted, seen).map(|rest| join("0", rest))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::Field;

    #[test]
    fn the_first_blob_field_of_a_type_is_named_by_its_path_and_is_reached() {
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
        let blob = |name| blob_field(&registry, registry.named(name).unwrap());
        let outside = |name| outside_field(&registry, registry.named(name).unwrap());
        assert_eq!(blob("demo::Outer").as_deref(), Some("pairs.0.entries"));
        assert_eq!(blob("demo::Inner").as_deref(), Some("entries"));
        assert_eq!(blob("demo::Plain"), None);
        assert_eq!(blob("demo::Level"), None);
        assert_eq!(outside("demo::Outer"), None, "a blob is reached");
    }

    #[test]
    fn a_designer_data_field_is_never_reached() {
        let registry = Registry::new([
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::DsArray<demo::uint8>", Kind::DsArray, 16, 8).with_inner("demo::uint8"),
            Type::new("demo::Held", Kind::Struct, 24, 8).with_fields(vec![
                Field::new("level", 0, "demo::uint8"),
                Field::new("list", 8, "demo::DsArray<demo::uint8>"),
            ]),
        ]);
        let held = registry.named("demo::Held").unwrap();
        assert_eq!(outside_field(&registry, held).as_deref(), Some("list"));
        assert_eq!(blob_field(&registry, held), None);
    }

    #[test]
    fn a_designer_data_field_inside_a_blob_element_is_never_reached() {
        let registry = Registry::new([
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::DsString", Kind::DsString, 16, 8),
            Type::new("demo::Entry", Kind::Struct, 16, 8).with_fields(vec![Field::new(
                "name",
                0,
                "demo::DsString",
            )]),
            Type::new("demo::BlobArray<demo::Entry>", Kind::BlobArray, 8, 4)
                .with_inner("demo::Entry"),
            Type::new("demo::BlobOptional<demo::Entry>", Kind::BlobOptional, 4, 4)
                .with_inner("demo::Entry"),
            Type::new("demo::Listed", Kind::Struct, 8, 4).with_fields(vec![Field::new(
                "entries",
                0,
                "demo::BlobArray<demo::Entry>",
            )]),
            Type::new("demo::Maybe", Kind::Struct, 4, 4).with_fields(vec![Field::new(
                "entry",
                0,
                "demo::BlobOptional<demo::Entry>",
            )]),
        ]);
        let outside = |name| outside_field(&registry, registry.named(name).unwrap());
        assert_eq!(outside("demo::Listed").as_deref(), Some("entries.0.name"));
        assert_eq!(outside("demo::Maybe").as_deref(), Some("entry.name"));
    }

    #[test]
    fn a_typedef_is_followed_a_loop_ends_and_an_unknown_kind_is_never_reached() {
        // Forty fields of its own type: a walk by depth alone would not end.
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
        let blob = |name| blob_field(&registry, registry.named(name).unwrap());
        assert_eq!(blob("demo::Named").as_deref(), Some("entries"));
        assert_eq!(blob("demo::Loop"), None);
        assert_eq!(outside("demo::Loop"), None);
        assert_eq!(outside("demo::Nest"), None);
        assert_eq!(outside("demo::Tree"), None);
        assert_eq!(outside("demo::Odd").as_deref(), Some("mystery"));
    }
}
