//! The client's marker types: listed from the game's registry, and taken by
//! a mod's own markers.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{
        Headed, Header, MapMarkerTypeInfo, MarkerTypeSorting as RawSorting, Side, Status, UiMarker,
    };
    use enshwick::content::Guid;
    use enshwick::map::{MapMarkerTypeId, MarkerTypeSorting};
    use enshwick::test::FakeMarker;
    use enshwick::ui::{Marker, Ui, UiError};
    use enshwick::{Load, Mod, Off};

    static UI: OnceLock<Ui> = OnceLock::new();

    struct Pins;

    impl Mod for Pins {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = UI.set(load.ui()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Pins,
        r#"
        id = "you.pins"
        side = "both"
        "#
    );

    const DUNGEON: u32 = 0x1d2c_3b4a;
    const ALTAR: u32 = 0x0a0b_0c0d;
    const ODD: u32 = 0x5555_aaaa;
    const NAME: Guid = Guid::parse("0b6b1f7e-3f43-4a2e-9a0d-5f2c1e7d8a90");

    fn registry() -> Vec<MapMarkerTypeInfo> {
        vec![
            MapMarkerTypeInfo {
                type_name: NAME,
                marker_id: DUNGEON,
                sorting_category: RawSorting::DUNGEONS,
                show_label: 1,
                ..MapMarkerTypeInfo::EMPTY
            },
            MapMarkerTypeInfo {
                marker_id: ALTAR,
                sorting_category: RawSorting::FLAME_RELATED,
                is_fast_travel_destination: 1,
                show_above_fog_of_war: 1,
                ..MapMarkerTypeInfo::EMPTY
            },
            MapMarkerTypeInfo {
                marker_id: ODD,
                sorting_category: RawSorting::UNKNOWN,
                is_custom_marker_type: 1,
                ..MapMarkerTypeInfo::EMPTY
            },
        ]
    }

    fn start(host: enshwick::test::FakeHost) -> (enshwick::test::Installed, Ui) {
        let loader = host.side(Side::CLIENT).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Pins)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (loader, *UI.get().unwrap())
    }

    #[test]
    fn the_registry_lists_each_type_with_its_group_and_flags() {
        let (_loader, ui) = start(enshwick::test::FakeHost::new().map_marker_types(registry()));
        let types = ui.map_marker_types().unwrap();
        assert_eq!(types.len(), 3);
        let dungeon = &types[0];
        assert_eq!(dungeon.marker_type, MapMarkerTypeId::new(DUNGEON));
        assert_eq!(dungeon.type_name, Some(NAME));
        assert_eq!(dungeon.sorting_category, Some(MarkerTypeSorting::Dungeons));
        assert!(dungeon.show_label);
        assert!(!dungeon.is_fast_travel_destination);
        let altar = &types[1];
        assert_eq!(altar.type_name, None);
        assert_eq!(
            altar.sorting_category,
            Some(MarkerTypeSorting::FlameRelated)
        );
        assert!(altar.is_fast_travel_destination && altar.show_above_fog_of_war);
        let odd = &types[2];
        assert_eq!(odd.sorting_category, None, "a group this API does not know");
        assert!(odd.is_custom_marker_type);
    }

    #[test]
    fn a_marker_past_the_fixed_point_is_refused_and_the_last_set_stays() {
        let (loader, ui) = start(enshwick::test::FakeHost::new());
        let edge = [2_147_483_647.0, 0.0, -2_147_483_647.0];
        ui.set_markers(&[Marker::new(edge)]).unwrap();
        for far in [
            [f64::NAN, 0.0, 0.0],
            [0.0, 1e300, 0.0],
            [0.0, 0.0, -2_147_483_648.0],
        ] {
            assert_eq!(
                ui.set_markers(&[Marker::new([1.0; 3]), Marker::new(far)]),
                Err(UiError::Refused),
                "{far:?}"
            );
        }
        let kept: Vec<[f64; 3]> = loader.markers().iter().map(|m| m.position).collect();
        assert_eq!(kept, [edge]);
    }

    #[test]
    fn a_marker_takes_a_listed_type_and_an_unlisted_one_is_refused() {
        let (loader, ui) = start(enshwick::test::FakeHost::new().map_marker_types(registry()));
        let altar = MapMarkerTypeId::new(ALTAR);
        ui.set_markers(&[
            Marker::new([1.0, 0.0, 0.0]).marker_type(altar),
            Marker::new([2.0, 0.0, 0.0]),
        ])
        .unwrap();
        let types: Vec<u32> = loader.markers().iter().map(|m| m.marker_type).collect();
        assert_eq!(types, [ALTAR, 0]);
        let unlisted = Marker::new([0.0; 3]).marker_type(MapMarkerTypeId::new(7));
        assert_eq!(ui.set_markers(&[unlisted]), Err(UiError::Refused));
        assert_eq!(
            loader.markers().len(),
            2,
            "a refused set leaves the last one"
        );
        assert_eq!(
            loader.markers()[1],
            FakeMarker {
                position: [2.0, 0.0, 0.0],
                label: String::new(),
                color: UiMarker::WHITE,
                marker_type: 0,
            }
        );
    }

    #[test]
    fn entries_shorter_than_their_first_version_are_refused_and_whole_ones_say_their_size() {
        let (loader, _ui) = start(enshwick::test::FakeHost::new().map_marker_types(registry()));
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let mut out = [MapMarkerTypeInfo::EMPTY; 3];
        let mut count = 0u32;
        let header = u32::try_from(size_of::<Header>()).unwrap();
        let born = u32::try_from(<MapMarkerTypeInfo as Headed>::BORN).unwrap();
        for short in [header - 1, born - 1] {
            let refused =
                (api.map_marker_types)(loader.handle(), out.as_mut_ptr(), short, 3, &raw mut count);
            assert_eq!((refused, count), (Status::BAD_ARGS, 0), "{short}");
        }
        let status =
            (api.map_marker_types)(loader.handle(), out.as_mut_ptr(), born, 3, &raw mut count);
        assert_eq!((status, count), (Status::OK, 3));
        for entry in out {
            assert_eq!(
                entry.header.size, born,
                "the header says what the entry holds"
            );
        }
    }

    #[test]
    fn without_a_registry_types_are_unavailable_and_typed_markers_too() {
        let (loader, ui) = start(enshwick::test::FakeHost::new());
        assert_eq!(ui.map_marker_types(), Err(UiError::Unavailable));
        let typed = Marker::new([0.0; 3]).marker_type(MapMarkerTypeId::new(DUNGEON));
        assert_eq!(ui.set_markers(&[typed]), Err(UiError::Unavailable));
        ui.set_markers(&[Marker::new([0.0; 3])]).unwrap();
        assert_eq!(loader.markers().len(), 1);
    }

    #[test]
    fn without_a_registry_a_typed_marker_beside_a_far_one_is_unavailable_as_the_loader_answers() {
        let (_loader, ui) = start(enshwick::test::FakeHost::new());
        let typed = Marker::new([0.0; 3]).marker_type(MapMarkerTypeId::new(DUNGEON));
        assert_eq!(
            ui.set_markers(&[typed, Marker::new([1e300, 0.0, 0.0])]),
            Err(UiError::Unavailable),
            "the registry is read before the positions"
        );
    }
}
