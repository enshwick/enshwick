//! The fields whose data lies outside a live component's own bytes: the
//! blob kinds, which a typed read reaches in the component's set-up data,
//! and the kinds it never reaches.

use crate::resources::{Kind, Registry, Type};

/// Where the fields of a type keep data outside its own bytes: each the
/// path, by the game's field names, of the first such field at any depth.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Outside {
    /// A field whose data a read of the live component does not reach: a
    /// designer data kind, which points to the heap, or a kind this API
    /// does not know, which may keep its data anywhere.
    pub(super) unreached: Option<String>,
    /// A field whose data follows the component in its set-up data: a blob
    /// kind, whose offset points past the component's own bytes.
    pub(super) blob: Option<String>,
}

impl Outside {
    /// The fields of `ty` that keep data outside its own bytes, found in
    /// one walk. A variant may hold its inner type or any struct derived
    /// from it, so the walk meets each of them; a derived one's fields are
    /// named after the derived type, as in `shape.demo::Square.side`.
    pub(super) fn of(registry: &Registry<'_>, ty: &Type<'_>) -> Self {
        let mut walk = Walk {
            registry,
            seen: Vec::new(),
            path: Vec::new(),
            found: Self::default(),
        };
        walk.ty(ty);
        walk.found
    }
}

/// One walk over a type's fields. `seen` holds every type met so far: a
/// type met again adds nothing its first meeting did not, so a loop ends
/// and each type is walked once.
struct Walk<'r, 'a> {
    registry: &'r Registry<'a>,
    seen: Vec<*const ()>,
    path: Vec<&'r str>,
    found: Outside,
}

impl<'r, 'a> Walk<'r, 'a> {
    /// Walks `ty`; `true` once both kinds of field are found, which ends
    /// the walk.
    fn ty(&mut self, ty: &'r Type<'a>) -> bool {
        let at = core::ptr::from_ref(ty).cast::<()>();
        if self.seen.contains(&at) {
            return self.done();
        }
        self.seen.push(at);
        let registry = self.registry;
        let inner = ty.inner().and_then(|name| registry.named(name));
        match ty.kind() {
            Kind::DsArray
            | Kind::DsString
            | Kind::DsOptional
            | Kind::DsVariant
            | Kind::Other(_) => {
                if self.found.unreached.is_none() {
                    self.found.unreached = Some(self.path.join("."));
                }
                self.done()
            }
            Kind::BlobString => self.blob(),
            Kind::BlobArray => self.blob() || self.within("0", inner),
            Kind::BlobOptional => self.blob() || inner.is_some_and(|inner| self.ty(inner)),
            Kind::BlobVariant => {
                self.blob()
                    || inner.is_some_and(|inner| {
                        self.ty(inner)
                            || registry
                                .derived(inner)
                                .any(|held| self.within(held.qualified_name(), Some(held)))
                    })
            }
            Kind::Typedef => inner.is_some_and(|inner| self.ty(inner)),
            Kind::StaticArray => self.within("0", inner),
            Kind::Struct => registry.all_fields(ty).into_iter().any(|field| {
                let held = field.type_name().and_then(|name| registry.named(name));
                self.within(field.name(), held)
            }),
            _ => false,
        }
    }

    /// Walks `held` one step down the path, at `step`.
    fn within(&mut self, step: &'r str, held: Option<&'r Type<'a>>) -> bool {
        let Some(held) = held else {
            return false;
        };
        self.path.push(step);
        let done = self.ty(held);
        self.path.pop();
        done
    }

    /// Notes a blob field at the path walked.
    fn blob(&mut self) -> bool {
        if self.found.blob.is_none() {
            self.found.blob = Some(self.path.join("."));
        }
        self.done()
    }

    fn done(&self) -> bool {
        self.found.unreached.is_some() && self.found.blob.is_some()
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
        let blob = |name| Outside::of(&registry, registry.named(name).unwrap()).blob;
        let outside = |name| Outside::of(&registry, registry.named(name).unwrap()).unreached;
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
        let found = Outside::of(&registry, held);
        assert_eq!(found.unreached.as_deref(), Some("list"));
        assert_eq!(found.blob, None);
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
        let outside = |name| Outside::of(&registry, registry.named(name).unwrap()).unreached;
        assert_eq!(outside("demo::Listed").as_deref(), Some("entries.0.name"));
        assert_eq!(outside("demo::Maybe").as_deref(), Some("entry.name"));
    }

    #[test]
    fn a_designer_data_field_of_a_type_derived_from_a_variant_s_inner_type_is_never_reached() {
        let registry = Registry::new([
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::DsString", Kind::DsString, 16, 8),
            Type::new("demo::Shape", Kind::Struct, 1, 1).with_fields(vec![Field::new(
                "sides",
                0,
                "demo::uint8",
            )]),
            Type::new("demo::Square", Kind::Struct, 2, 1)
                .with_inner("demo::Shape")
                .with_fields(vec![Field::new("side", 1, "demo::uint8")]),
            Type::new("demo::Label", Kind::Struct, 24, 8)
                .with_inner("demo::Square")
                .with_fields(vec![Field::new("text", 8, "demo::DsString")]),
            Type::new("demo::BlobVariant<demo::Shape>", Kind::BlobVariant, 12, 4)
                .with_inner("demo::Shape"),
            Type::new("demo::Drawn", Kind::Struct, 12, 4).with_fields(vec![Field::new(
                "shape",
                0,
                "demo::BlobVariant<demo::Shape>",
            )]),
        ]);
        let drawn = registry.named("demo::Drawn").unwrap();
        assert_eq!(
            Outside::of(&registry, drawn),
            Outside {
                unreached: Some("shape.demo::Label.text".to_owned()),
                blob: Some("shape".to_owned()),
            },
            "a variant holds any type derived from its inner one, at any depth"
        );
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
        let outside = |name| Outside::of(&registry, registry.named(name).unwrap()).unreached;
        let blob = |name| Outside::of(&registry, registry.named(name).unwrap()).blob;
        assert_eq!(blob("demo::Named").as_deref(), Some("entries"));
        assert_eq!(blob("demo::Loop"), None);
        assert_eq!(outside("demo::Loop"), None);
        assert_eq!(outside("demo::Nest"), None);
        assert_eq!(outside("demo::Tree"), None);
        assert_eq!(outside("demo::Odd").as_deref(), Some("mystery"));
    }
}
