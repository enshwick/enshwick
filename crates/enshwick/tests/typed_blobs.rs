//! A generated component whose field keeps its data past the component, a
//! `BlobArray`, reads live from the set-up data that follows it, and a blob
//! that runs past the readable bytes turns off only that read, by name.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::resources::{Field, Kind, Type, fnv1a32};
    use enshwick::typed::ComponentType;
    use enshwick::typed::ecs::EntityTagContainer;
    use enshwick::world::World;
    use enshwick::{Entity, Load, Mod, Off};

    const TAGS: &str = "keen::ecs::EntityTagContainer";
    const ENTRY: &str = "keen::ecs::EntityTagReferenceEntry";
    const WHOLE: Entity = Entity(4096);
    const CUT: Entity = Entity(4097);

    static RESOLVED: OnceLock<(World, ComponentType<EntityTagContainer>)> = OnceLock::new();

    /// `EntityTagContainer` and what it holds, as server build 479 reflects
    /// them.
    fn types() -> Vec<Type<'static>> {
        let field = |name, offset, ty| Field::new(name, offset, ty);
        vec![
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
            Type::new(
                "keen::BlobArray<keen::ecs::EntityTagReferenceEntry>",
                Kind::BlobArray,
                8,
                4,
            )
            .with_inner(ENTRY),
            Type::new(TAGS, Kind::Struct, 8, 4)
                .with_inner("keen::ecs::Component")
                .with_fields(vec![field(
                    "tags",
                    0,
                    "keen::BlobArray<keen::ecs::EntityTagReferenceEntry>",
                )]),
        ]
    }

    /// A container's set-up data: its 8 bytes, an offset of 8 from the
    /// field and a count, then `held` of the `count` tags.
    fn container(count: u32, held: &[Guid]) -> Vec<u8> {
        let mut bytes = [8i32.to_le_bytes(), count.to_le_bytes()].concat();
        for tag in held {
            bytes.extend_from_slice(&tag.0);
        }
        bytes
    }

    struct Blobs;

    impl Mod for Blobs {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let tags = ComponentType::<EntityTagContainer>::resolve(load)?;
            let _ = RESOLVED.set((load.world()?, tags));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Blobs,
        r#"
        id = "you.blobs"
        side = "server"
        "#
    );

    #[test]
    fn a_live_component_reads_its_blob_array_and_a_cut_one_is_refused_by_name() {
        let hash = fnv1a32(TAGS.as_bytes());
        let (bandit, vukah) = (Guid([0x11; 16]), Guid([0x22; 16]));
        let loader = enshwick::test::FakeHost::new()
            .world()
            .types(types())
            .type_hash("ecs::EntityTagContainer", hash)
            .type_size("ecs::EntityTagContainer", 8)
            .component(WHOLE, hash, &container(2, &[bandit, vukah]))
            .component(CUT, hash, &container(3, &[bandit]))
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Blobs)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        let (world, tags) = RESOLVED.get().unwrap();
        let read = tags.read(*world, WHOLE).unwrap().unwrap();
        let held: Vec<Guid> = read.tags.iter().map(|entry| entry.tag).collect();
        assert_eq!(held, [bandit, vukah]);

        let refused = tags.read(*world, CUT).unwrap_err().to_string();
        assert!(refused.contains("tags"), "{refused}");
        assert_eq!(tags.read(*world, Entity(1)).unwrap(), None);
    }
}
