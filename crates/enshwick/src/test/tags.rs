//! The fake's `ecs::EntityTagContainer` reflected: a component whose one
//! field, a `BlobArray`, keeps its data in the set-up data after it.

use crate::content::Guid;
use crate::resources::{Field, Kind, Type, fnv1a32};

use super::FakeHost;

/// The qualified name of the tag container, whose `fnv1a32` hash keys its
/// components and its blob data.
pub const TAG_CONTAINER: &str = "keen::ecs::EntityTagContainer";

const ENTRY: &str = "keen::ecs::EntityTagReferenceEntry";
const ENTRIES: &str = "keen::BlobArray<keen::ecs::EntityTagReferenceEntry>";
const ID: &str = "ecs::EntityTagContainer";

impl FakeHost {
    /// `ecs::EntityTagContainer` and the types it holds, as server build 479
    /// reflects them, with its hash and size: what a typed component type
    /// of it resolves.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::fnv1a32;
    /// use enshwick::test::{FakeHost, TAG_CONTAINER};
    ///
    /// let loader = FakeHost::new()
    ///     .world()
    ///     .entity_tags()
    ///     .tagged(enshwick::Entity(7), 1, &[enshwick::content::Guid([0x11; 16])])
    ///     .install();
    /// let bytes = loader.component_bytes(enshwick::Entity(7), fnv1a32(TAG_CONTAINER.as_bytes()));
    /// assert_eq!(bytes.map(|bytes| bytes.len()), Some(24));
    /// ```
    #[must_use]
    pub fn entity_tags(self) -> Self {
        let field = Field::new;
        self.types([
            Type::new("keen::ecs::Component", Kind::Struct, 1, 1),
            Type::new(
                "keen::ObjectReference<keen::ecs::EntityTag>",
                Kind::ObjectReference,
                16,
                4,
            ),
            Type::new("keen::ecs::EntityTagReference", Kind::Typedef, 16, 4)
                .with_inner("keen::ObjectReference<keen::ecs::EntityTag>"),
            Type::new(ENTRY, Kind::Struct, 16, 4).with_fields(vec![field(
                "tag",
                0,
                "keen::ecs::EntityTagReference",
            )]),
            Type::new(ENTRIES, Kind::BlobArray, 8, 4).with_inner(ENTRY),
            Type::new(TAG_CONTAINER, Kind::Struct, 8, 4)
                .with_inner("keen::ecs::Component")
                .with_fields(vec![field("tags", 0, ENTRIES)]),
        ])
        .type_hash(ID, fnv1a32(TAG_CONTAINER.as_bytes()))
        .type_size(ID, 8)
    }

    /// Gives `entity` a tag container whose set-up data holds its 8 bytes,
    /// an offset of 8 from the field and `count`, then the tags `held`; a
    /// `held` shorter than `count` cuts the blob short.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::entity_tags`].
    #[must_use]
    pub fn tagged(self, entity: crate::Entity, count: u32, held: &[Guid]) -> Self {
        let mut bytes = [8i32.to_le_bytes(), count.to_le_bytes()].concat();
        for tag in held {
            bytes.extend_from_slice(&tag.0);
        }
        self.component(entity, fnv1a32(TAG_CONTAINER.as_bytes()), &bytes)
    }
}
