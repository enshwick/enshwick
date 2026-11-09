//! A generated component whose field keeps its data past the component, a
//! `BlobArray`, reads live from the set-up data that follows it, and a blob
//! that runs past the readable bytes turns off only that read, by name.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::resources::fnv1a32;
    use enshwick::test::{FakeHost, TAG_CONTAINER};
    use enshwick::typed::ComponentType;
    use enshwick::typed::ecs::EntityTagContainer;
    use enshwick::world::World;
    use enshwick::{Entity, Load, Mod, Off};

    const WHOLE: Entity = Entity(4096);
    const CUT: Entity = Entity(4097);
    const SHORT: Entity = Entity(4098);
    const EMPTY: Entity = Entity(4099);

    static RESOLVED: OnceLock<(World, ComponentType<EntityTagContainer>)> = OnceLock::new();

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
    fn a_live_component_reads_its_blob_array_and_a_cut_or_short_one_is_refused_by_name() {
        let hash = fnv1a32(TAG_CONTAINER.as_bytes());
        let (bandit, vukah) = (Guid([0x11; 16]), Guid([0x22; 16]));
        let loader = FakeHost::new()
            .world()
            .entity_tags()
            .tagged(WHOLE, 2, &[bandit, vukah])
            .tagged(CUT, 3, &[bandit])
            .component(SHORT, hash, &8i32.to_le_bytes())
            .tagged(EMPTY, 0, &[])
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
        assert!(
            refused.contains("typed ecs::EntityTagContainer") && refused.contains("tags"),
            "{refused}"
        );
        let short = tags.read(*world, SHORT).unwrap_err().to_string();
        assert!(
            short.contains("does not read as far as its field tags"),
            "{short}"
        );
        let empty = tags.read(*world, EMPTY).unwrap().unwrap();
        assert!(empty.tags.is_empty(), "a component of its own size reads");
        assert_eq!(tags.read(*world, Entity(1)).unwrap(), None);
    }

    #[test]
    fn a_write_of_a_component_with_blob_fields_is_refused_and_leaves_its_bytes() {
        let hash = fnv1a32(TAG_CONTAINER.as_bytes());
        let loader = FakeHost::new()
            .world()
            .entity_tags()
            .tagged(WHOLE, 1, &[Guid([0x11; 16])])
            .install();
        let bytes = loader.component_bytes(WHOLE, hash);
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Blobs)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        let (world, tags) = RESOLVED.get().unwrap();
        let read = tags.read(*world, WHOLE).unwrap().unwrap();
        let refused = tags.write(*world, WHOLE, &read).unwrap_err().to_string();
        assert!(refused.contains("read-only set-up data"), "{refused}");
        assert_eq!(loader.component_bytes(WHOLE, hash), bytes);
    }
}
