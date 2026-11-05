//! Weather phenomena a mod adds and changes reach the game's weather
//! resource through the typed resource and the loader's serve.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::{Guid, Id};
    use enshwick::resources::{Constant, Field, Kind, Registry, Type, Value, fnv1a32};
    use enshwick::typed::{
        Reflected, WeatherPhenomenonEffect as GameEffect, WeatherSystemResource,
    };
    use enshwick::weather::{
        NewPhenomenon, PhenomenonChange, PhenomenonId, WeatherPhenomenonEffect,
    };
    use enshwick::{Load, Mod, Off};

    const SYSTEM: &str = "keen::WeatherSystemResource";

    type Added = (PhenomenonId, PhenomenonId, Result<(), String>);

    static ADDED: OnceLock<Added> = OnceLock::new();

    fn guid(n: u8) -> Guid {
        Guid([n; 16])
    }

    /// The weather resource's types, laid out at the sizes server build 224
    /// reflects.
    #[expect(clippy::too_many_lines, reason = "one reflected type per row")]
    fn types() -> Vec<Type<'static>> {
        let field = |name, offset, ty| Field::new(name, offset, ty);
        let array = |name, inner| Type::new(name, Kind::BlobArray, 8, 4).with_inner(inner);
        let index = |name| Type::new(name, Kind::Typedef, 1, 1).with_inner("keen::uint8");
        let pair = |name, of| {
            Type::new(name, Kind::Struct, 8, 4)
                .with_fields(vec![field("x", 0, of), field("y", 4, of)])
        };
        vec![
            Type::new("keen::uint8", Kind::Uint8, 1, 1),
            Type::new("keen::uint32", Kind::Uint32, 4, 4),
            Type::new("keen::sint64", Kind::Sint64, 8, 8),
            Type::new("keen::float32", Kind::Float32, 4, 4),
            Type::new("keen::Guid", Kind::Guid, 16, 4),
            Type::new("keen::ObjectReference", Kind::ObjectReference, 16, 4),
            Type::new("keen::BlobString", Kind::BlobString, 8, 4),
            Type::new("keen::HashKey32", Kind::Struct, 4, 4).with_fields(vec![field(
                "value",
                0,
                "keen::uint32",
            )]),
            pair("keen::uint2", "keen::uint32"),
            pair("keen::float2", "keen::float32"),
            Type::new("keen::Range8", Kind::Struct, 2, 1).with_fields(vec![
                field("start", 0, "keen::uint8"),
                field("count", 1, "keen::uint8"),
            ]),
            Type::new("keen::Time", Kind::Struct, 8, 8).with_fields(vec![field(
                "value",
                0,
                "keen::sint64",
            )]),
            Type::new("keen::FloatRange", Kind::Struct, 8, 4).with_fields(vec![
                field("minValue", 0, "keen::float32"),
                field("maxValue", 4, "keen::float32"),
            ]),
            index("keen::WeatherLayerMaskIndex"),
            index("keen::WeatherRainMaskIndex"),
            index("keen::WeatherPhenomenonIndex"),
            Type::new("keen::WeatherPhenomenonEffect", Kind::Enum, 1, 1)
                .with_inner("keen::uint8")
                .with_constants(vec![
                    Constant::new("None", 0),
                    Constant::new("Rain", 1),
                    Constant::new("Blizzard", 2),
                ]),
            Type::new(
                "keen::WeatherSystemPhenomenonResourceFlag",
                Kind::Enum,
                4,
                4,
            )
            .with_inner("keen::uint32")
            .with_constants(vec![
                Constant::new("Rotation", 0),
                Constant::new("Tiling", 1),
                Constant::new("MaskCloudMap", 2),
                Constant::new("MaskRain", 3),
            ]),
            Type::new(
                "keen::WeatherSystemPhenomenonResourceFlags",
                Kind::Bitmask32,
                4,
                4,
            )
            .with_inner("keen::WeatherSystemPhenomenonResourceFlag"),
            Type::new(
                "keen::WeatherSystemLayerStateConfigurationType",
                Kind::Enum,
                1,
                1,
            )
            .with_inner("keen::uint8")
            .with_constants(vec![
                Constant::new("Ignore", 0),
                Constant::new("Reset", 1),
                Constant::new("SetPhenomenon", 2),
            ]),
            Type::new("keen::BlobOptional<keen::Time>", Kind::BlobOptional, 4, 4)
                .with_inner("keen::Time"),
            array("keen::BlobArray<keen::uint8>", "keen::uint8"),
            Type::new("keen::WeatherSystemMaskResource", Kind::Struct, 56, 4).with_fields(vec![
                field("worldToRelative", 0, "keen::float2"),
                field("bboxMin", 8, "keen::float2"),
                field("bboxMax", 16, "keen::float2"),
                field("originalSize", 24, "keen::uint2"),
                field("pixelOffset", 32, "keen::uint2"),
                field("pixelCount", 40, "keen::uint2"),
                field("pixelData", 48, "keen::BlobArray<keen::uint8>"),
            ]),
            Type::new("keen::WeatherSystemLayerMaskResource", Kind::Struct, 56, 4)
                .with_fields(vec![field("mask", 0, "keen::WeatherSystemMaskResource")]),
            Type::new(
                "keen::WeatherSystemPhenomenonResource",
                Kind::Struct,
                176,
                8,
            )
            .with_fields(vec![
                field("guid", 0, "keen::Guid"),
                field("debugName", 16, "keen::BlobString"),
                field("phenomenon", 24, "keen::ObjectReference"),
                field("probability", 40, "keen::float32"),
                field("flags", 44, "keen::WeatherSystemPhenomenonResourceFlags"),
                field("durationMin", 48, "keen::Time"),
                field("durationMax", 56, "keen::Time"),
                field("positionYMin", 64, "keen::float32"),
                field("positionYMax", 68, "keen::float32"),
                field("radiusMin", 72, "keen::float32"),
                field("radiusMax", 76, "keen::float32"),
                field("moveSpeedMin", 80, "keen::float32"),
                field("moveSpeedMax", 84, "keen::float32"),
                field("spawnFadeTime", 88, "keen::Time"),
                field("despawnFadeTime", 96, "keen::Time"),
                field("strength", 104, "keen::float32"),
                field("cloudMapRange", 108, "keen::FloatRange"),
                field("height", 116, "keen::float32"),
                field("rainThreshold", 120, "keen::float32"),
                field("rainMaskIndex", 124, "keen::WeatherRainMaskIndex"),
                field("renderWeather", 128, "keen::ObjectReference"),
                field("fogModel", 144, "keen::ObjectReference"),
                field("fogModelHalfSizeXZ", 160, "keen::float2"),
                field("effect", 168, "keen::WeatherPhenomenonEffect"),
            ]),
            Type::new("keen::WeatherSystemLayerResource", Kind::Struct, 200, 8).with_fields(vec![
                field("guid", 0, "keen::Guid"),
                field("restrictMaskIndex", 16, "keen::WeatherLayerMaskIndex"),
                field("excludeMaskIndex", 17, "keen::WeatherLayerMaskIndex"),
                field("phenomenonIndexRange", 18, "keen::Range8"),
                field(
                    "nothingPhenomenon",
                    24,
                    "keen::WeatherSystemPhenomenonResource",
                ),
            ]),
            Type::new(
                "keen::WeatherSystemStateConfigurationResource",
                Kind::Struct,
                20,
                4,
            )
            .with_fields(vec![
                field("guid", 0, "keen::Guid"),
                field("layerStateIndexRange", 16, "keen::Range8"),
            ]),
            Type::new(
                "keen::WeatherSystemLayerStateConfigurationResource",
                Kind::Struct,
                8,
                4,
            )
            .with_fields(vec![
                field("type", 0, "keen::WeatherSystemLayerStateConfigurationType"),
                field("layerPhenomenonIndex", 1, "keen::WeatherPhenomenonIndex"),
                field("overrideDuration", 4, "keen::BlobOptional<keen::Time>"),
            ]),
            array("keen::BlobArray<Layer>", "keen::WeatherSystemLayerResource"),
            array(
                "keen::BlobArray<LayerMask>",
                "keen::WeatherSystemLayerMaskResource",
            ),
            array(
                "keen::BlobArray<Phenomenon>",
                "keen::WeatherSystemPhenomenonResource",
            ),
            array("keen::BlobArray<Mask>", "keen::WeatherSystemMaskResource"),
            array(
                "keen::BlobArray<StateConfiguration>",
                "keen::WeatherSystemStateConfigurationResource",
            ),
            array(
                "keen::BlobArray<LayerStateConfiguration>",
                "keen::WeatherSystemLayerStateConfigurationResource",
            ),
            Type::new(SYSTEM, Kind::Struct, 156, 4).with_fields(vec![
                field("guid", 0, "keen::Guid"),
                field("dataHash", 16, "keen::HashKey32"),
                field("worldSize", 20, "keen::uint2"),
                field("snowBorderRangeMapping", 28, "keen::float2"),
                field("snowBorderMask", 36, "keen::WeatherSystemMaskResource"),
                field("layers", 92, "keen::BlobArray<Layer>"),
                field("layerMasks", 100, "keen::BlobArray<LayerMask>"),
                field("phenomenons", 108, "keen::BlobArray<Phenomenon>"),
                field("rainMasks", 116, "keen::BlobArray<Mask>"),
                field(
                    "stateConfigurations",
                    124,
                    "keen::BlobArray<StateConfiguration>",
                ),
                field(
                    "layerStateConfigurations",
                    132,
                    "keen::BlobArray<LayerStateConfiguration>",
                ),
                field("newWorldStateConfiguration", 140, "keen::ObjectReference"),
            ]),
        ]
    }

    fn fields(fields: Vec<(&str, Value)>) -> Value {
        Value::Struct(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    fn phenomenon(n: u8) -> Value {
        fields(vec![
            ("guid", Value::Guid(guid(n))),
            ("debugName", Value::Text(format!("vanilla {n}"))),
            ("phenomenon", Value::Guid(guid(100 + n))),
            ("probability", Value::Float(0.25)),
            ("radiusMax", Value::Float(f64::from(n) * 100.0)),
            ("strength", Value::Float(0.5)),
        ])
    }

    fn layer(n: u8, start: u8, count: u8) -> Value {
        let range = fields(vec![
            ("start", Value::Uint(start.into())),
            ("count", Value::Uint(count.into())),
        ]);
        fields(vec![
            ("guid", Value::Guid(guid(n))),
            ("phenomenonIndexRange", range),
        ])
    }

    /// Layer 10 holds phenomena 1 and 2, layer 20 phenomenon 3; phenomenon
    /// 4 is in no layer, as the game's debug ones.
    fn vanilla() -> Vec<u8> {
        let system = fields(vec![
            ("guid", Value::Guid(guid(0))),
            (
                "layers",
                Value::List(vec![layer(10, 0, 2), layer(20, 2, 1)]),
            ),
            (
                "phenomenons",
                Value::List((1..=4).map(phenomenon).collect()),
            ),
        ]);
        Registry::new(types())
            .encode(SYSTEM, &system, None)
            .unwrap()
    }

    fn key() -> ResourceKey {
        guid(0).key_by_hash(fnv1a32(SYSTEM.as_bytes()))
    }

    struct Weather;

    impl Mod for Weather {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let rain = PhenomenonChange::new()
                .probability(1.0)
                .effect(WeatherPhenomenonEffect::Rain);
            let drizzle = load.add_weather_phenomenon(
                NewPhenomenon::new(Id::new("you.weather:drizzle"), guid(2)).values(rain),
            )?;
            let mist = load.add_weather_phenomenon(
                NewPhenomenon::new(Id::new("you.weather:mist"), guid(1)).in_layer(guid(20)),
            )?;
            load.change_weather_phenomenon(
                guid(3),
                PhenomenonChange::new()
                    .strength(2.0)
                    .effect(WeatherPhenomenonEffect::Blizzard),
            )?;
            let refused = load
                .add_weather_phenomenon(
                    NewPhenomenon::new(Id::new("you.weather:fog"), guid(1)).in_layer(guid(30)),
                )
                .map(drop)
                .map_err(|off| off.to_string());
            let _first = ADDED.set((drizzle, mist, refused));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Weather,
        r#"
        id = "you.weather"
        side = "both"
        "#
    );

    #[test]
    fn added_phenomena_end_their_layers_move_the_later_one_and_a_change_and_a_refusal_apply() {
        let base = vanilla();
        let loader = enshwick::test::FakeHost::new()
            .types(types())
            .type_hash("WeatherSystemResource", fnv1a32(SYSTEM.as_bytes()))
            .resource(key(), base.clone())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Weather)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let (drizzle, mist, refused) = ADDED.get().unwrap();
        let refused = refused.as_ref().unwrap_err();
        assert!(
            refused.contains("you.weather:fog") && refused.contains("weather layer"),
            "{refused}"
        );

        let built = loader.build_resource(&key(), &base).unwrap();
        let value = Registry::new(types()).decode(SYSTEM, &built).unwrap();
        let system = WeatherSystemResource::from_value(&value).unwrap();
        let order: Vec<Guid> = system.phenomenons.iter().map(|p| p.guid).collect();
        assert_eq!(
            order,
            [guid(1), guid(2), drizzle.0, guid(3), mist.0, guid(4)],
            "each copy at the end of its layer"
        );
        let ranges: Vec<(u8, u8)> = system
            .layers
            .iter()
            .map(|l| {
                let range = &l.phenomenon_index_range;
                (range.start, range.count)
            })
            .collect();
        assert_eq!(ranges, [(0, 3), (3, 2)], "the later layer moved up by one");

        let copy = &system.phenomenons[2];
        assert_eq!(copy.debug_name, "you.weather:drizzle");
        assert_eq!((copy.probability, &copy.effect), (1.0, &GameEffect::Rain));
        assert_eq!(copy.radius_max, 200.0, "the rest is the original's");
        assert_eq!(copy.phenomenon, guid(102));
        assert_eq!(system.phenomenons[4].radius_max, 100.0);
        let changed = &system.phenomenons[3];
        assert_eq!(
            (changed.strength, &changed.effect),
            (2.0, &GameEffect::Blizzard)
        );
        assert_eq!(changed.probability, 0.25, "values it does not set stay");
        assert_eq!(system.phenomenons[1].strength, 0.5, "others stay");
    }
}
