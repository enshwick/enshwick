//! A generated component whose variant may hold a type derived from its inner
//! one with a designer data field does not resolve, and its refusal names the
//! field through the derived type.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::Status;
    use enshwick::resources::{Field, Kind, Type, fnv1a32};
    use enshwick::test::FakeHost;
    use enshwick::typed::ComponentType;
    use enshwick::typed::ecs::ColliderResourceComponent;
    use enshwick::{Load, Mod, Off};

    const COMPONENT: &str = "keen::ecs::ColliderResourceComponent";
    const RESOURCE_DATA: &str = "keen::ecs::ColliderResourceData";
    const DATA: &str = "keen::ecs::ColliderData";
    const RESOURCES: &str = "keen::BlobArray<keen::ecs::ColliderResourceData>";
    const VARIANT: &str = "keen::BlobVariant<keen::ecs::ColliderData>";
    const VARIANTS: &str = "keen::BlobArray<keen::BlobVariant<keen::ecs::ColliderData>>";

    static REFUSED: OnceLock<String> = OnceLock::new();

    /// The collider component down to its variant of collider data, and a
    /// collider derived from that data with a designer data string.
    fn types() -> Vec<Type<'static>> {
        vec![
            Type::new("keen::ecs::Component", Kind::Struct, 1, 1),
            Type::new("keen::DsString", Kind::DsString, 16, 8),
            Type::new(DATA, Kind::Struct, 4, 4),
            Type::new("keen::ecs::NamedColliderData", Kind::Struct, 24, 8)
                .with_inner(DATA)
                .with_fields(vec![Field::new("name", 8, "keen::DsString")]),
            Type::new(VARIANT, Kind::BlobVariant, 12, 4).with_inner(DATA),
            Type::new(VARIANTS, Kind::BlobArray, 8, 4).with_inner(VARIANT),
            Type::new("keen::ecs::ColliderDataArray", Kind::Typedef, 8, 4).with_inner(VARIANTS),
            Type::new(RESOURCE_DATA, Kind::Struct, 56, 4).with_fields(vec![Field::new(
                "dataArray",
                0,
                "keen::ecs::ColliderDataArray",
            )]),
            Type::new(RESOURCES, Kind::BlobArray, 8, 4).with_inner(RESOURCE_DATA),
            Type::new(COMPONENT, Kind::Struct, 8, 4)
                .with_inner("keen::ecs::Component")
                .with_fields(vec![Field::new("colliders", 0, RESOURCES)]),
        ]
    }

    struct Variants;

    impl Mod for Variants {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            if let Err(why) = ComponentType::<ColliderResourceComponent>::resolve(load) {
                REFUSED.get_or_init(|| why.to_string());
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Variants,
        r#"
        id = "you.variants"
        side = "server"
        "#
    );

    #[test]
    fn a_variant_that_may_hold_a_derived_type_with_designer_data_is_refused_naming_the_field() {
        let loader = FakeHost::new()
            .world()
            .types(types())
            .type_hash(
                "ecs::ColliderResourceComponent",
                fnv1a32(COMPONENT.as_bytes()),
            )
            .type_size("ecs::ColliderResourceComponent", 8)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Variants)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        let refused = REFUSED.get().expect("the component resolved");
        assert!(
            refused.contains("colliders.0.dataArray.0.keen::ecs::NamedColliderData.name"),
            "{refused}"
        );
    }
}
