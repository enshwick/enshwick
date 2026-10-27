//! The game's own types, written by `cargo enshwick types` from the
//! reflection data of `enshrouded_server.exe sample`. Offsets and sizes change between
//! game builds: write it again for another build.

/// The game's namespace `keen`.
pub mod keen {
    /// `keen::EquipmentSlotType`: an enum stored as `u8`.
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct EquipmentSlotType(pub u8);

    impl EquipmentSlotType {
        /// `Invalid`.
        pub const INVALID: Self = Self(0);
        /// `Roof`.
        pub const ROOF: Self = Self(11);
        /// `MagicalFog`.
        pub const MAGICAL_FOG: Self = Self(12);
    }

    // SAFETY: every field is an integer, a float or an array of them, so
    // all bytes zero are a valid value.
    unsafe impl enshwick::raw::Zeroable for EquipmentSlotType {}
    // SAFETY: every field is an integer, a float or an array of them, so
    // any bytes are a valid value; the type is `Copy` and `'static`.
    unsafe impl enshwick::raw::AnyBitPattern for EquipmentSlotType {}
    const _: () = {
        assert!(size_of::<EquipmentSlotType>() == 1);
        assert!(align_of::<EquipmentSlotType>() == 1);
    };

    /// `keen::HashKey32`: 4 bytes, aligned to 4.
    #[repr(C, align(4))]
    #[derive(Clone, Copy, Debug)]
    pub struct HashKey32 {
        /// `value`, of `keen::uint32`, at 0x0.
        pub value: u32,
    }

    // SAFETY: every field is an integer, a float or an array of them, so
    // all bytes zero are a valid value.
    unsafe impl enshwick::raw::Zeroable for HashKey32 {}
    // SAFETY: every field is an integer, a float or an array of them, so
    // any bytes are a valid value; the type is `Copy` and `'static`.
    unsafe impl enshwick::raw::AnyBitPattern for HashKey32 {}
    const _: () = {
        assert!(size_of::<HashKey32>() == 4);
        assert!(align_of::<HashKey32>() == 4);
    };

    /// `keen::ItemId`, another name for `keen::HashKey32`.
    pub type ItemId = HashKey32;

    /// `keen::Packed`: 12 bytes, aligned to 4.
    #[repr(C, align(4))]
    #[derive(Clone, Copy, Debug)]
    pub struct Packed {
        _pad_0: [u8; 4],
        /// `wide`, of `keen::uint64`, at 0x4.
        pub wide: [u8; 8],
    }

    // SAFETY: every field is an integer, a float or an array of them, so
    // all bytes zero are a valid value.
    unsafe impl enshwick::raw::Zeroable for Packed {}
    // SAFETY: every field is an integer, a float or an array of them, so
    // any bytes are a valid value; the type is `Copy` and `'static`.
    unsafe impl enshwick::raw::AnyBitPattern for Packed {}
    const _: () = {
        assert!(size_of::<Packed>() == 12);
        assert!(align_of::<Packed>() == 4);
    };

    /// The game's namespace `keen::ai`.
    pub mod ai {
        /// `keen::ai::State`: 4 bytes, aligned to 4.
        #[repr(C, align(4))]
        #[derive(Clone, Copy, Debug)]
        pub struct State {
            _pad_0: [u8; 4],
        }

        // SAFETY: every field is an integer, a float or an array of them, so
        // all bytes zero are a valid value.
        unsafe impl enshwick::raw::Zeroable for State {}
        // SAFETY: every field is an integer, a float or an array of them, so
        // any bytes are a valid value; the type is `Copy` and `'static`.
        unsafe impl enshwick::raw::AnyBitPattern for State {}
        const _: () = {
            assert!(size_of::<State>() == 4);
            assert!(align_of::<State>() == 4);
        };
    }

    /// The game's namespace `keen::ecs`.
    pub mod ecs {
        /// `keen::ecs::GameEvent`: 8 bytes, aligned to 4.
        #[repr(C, align(4))]
        #[derive(Clone, Copy, Debug)]
        pub struct GameEvent {
            /// `time`, of `keen::uint32`, at 0x0.
            pub time: u32,
            /// `flags`, of `keen::uint8`, at 0x4.
            pub flags: u8,
            _pad_5: [u8; 3],
        }

        // SAFETY: every field is an integer, a float or an array of them, so
        // all bytes zero are a valid value.
        unsafe impl enshwick::raw::Zeroable for GameEvent {}
        // SAFETY: every field is an integer, a float or an array of them, so
        // any bytes are a valid value; the type is `Copy` and `'static`.
        unsafe impl enshwick::raw::AnyBitPattern for GameEvent {}
        const _: () = {
            assert!(size_of::<GameEvent>() == 8);
            assert!(align_of::<GameEvent>() == 4);
        };

        /// `keen::ecs::HitEvent`: 24 bytes, aligned to 4. Left in padding, as Rust cannot place them there: `lost`.
        #[repr(C, align(4))]
        #[derive(Clone, Copy, Debug)]
        pub struct HitEvent {
            /// The game's `keen::ecs::GameEvent`, which this type derives from.
            pub base: GameEvent,
            /// `damage`, of `keen::float32`, at 0x8.
            pub damage: f32,
            /// `item`, of `keen::ItemId`, at 0xc.
            pub item: super::ItemId,
            /// `slots`, of `keen::StaticArray<keen::uint8,4>`, at 0x10.
            pub slots: [u8; 4],
            /// `isCritical`, of `keen::bool`, at 0x14.
            pub is_critical: u8,
            /// `type`, of `keen::EquipmentSlotType`, at 0x15.
            pub r#type: super::EquipmentSlotType,
            _pad_16: [u8; 2],
        }

        // SAFETY: every field is an integer, a float or an array of them, so
        // all bytes zero are a valid value.
        unsafe impl enshwick::raw::Zeroable for HitEvent {}
        // SAFETY: every field is an integer, a float or an array of them, so
        // any bytes are a valid value; the type is `Copy` and `'static`.
        unsafe impl enshwick::raw::AnyBitPattern for HitEvent {}
        const _: () = {
            assert!(size_of::<HitEvent>() == 24);
            assert!(align_of::<HitEvent>() == 4);
        };
    }

    /// The game's namespace `keen::fish`.
    pub mod fish {
        /// `keen::fish::State`: 4 bytes, aligned to 4.
        #[repr(C, align(4))]
        #[derive(Clone, Copy, Debug)]
        pub struct State {
            _pad_0: [u8; 4],
        }

        // SAFETY: every field is an integer, a float or an array of them, so
        // all bytes zero are a valid value.
        unsafe impl enshwick::raw::Zeroable for State {}
        // SAFETY: every field is an integer, a float or an array of them, so
        // any bytes are a valid value; the type is `Copy` and `'static`.
        unsafe impl enshwick::raw::AnyBitPattern for State {}
        const _: () = {
            assert!(size_of::<State>() == 4);
            assert!(align_of::<State>() == 4);
        };
    }
}
