//! What you.lockers 1.2.0 offers other mods, by the alias `lockers`.
//!
//! Written by `cargo enshwick deps lockers`. Write it again for a newer
//! you.lockers rather than edit it; the checks hold each declaration to the
//! owner's layout. `enshwick::deps!()` makes it `deps::lockers`.

/// The rent of a stall, in coins.
#[enshwick::call(name = "rent", of = lockers, output = u64, version = 2)]
#[derive(Clone, Copy, Debug)]
pub struct Rent {
    /// The player's character.
    pub character: enshwick::Entity,
    /// What the player does.
    pub deal: Deal,
    /// Whether the stall is the player's own.
    pub own: bool,
    /// What the stall sells.
    pub stall: Stall,
    /// The stall's place in its row.
    pub place: u16,
}

const _: () = {
    assert!(
        <Rent as enshwick::services::Call>::LAYOUT[0].hash == 0xb53b_31eb_b124_3978,
        "`character` differs from the server's",
    );
    assert!(
        <Rent as enshwick::services::Call>::LAYOUT[1].hash == 0x4207_c59c_6958_bcb5,
        "`deal` differs from the server's",
    );
    assert!(
        <Rent as enshwick::services::Call>::LAYOUT[2].hash == 0x3212_dc21_1893_c5d6,
        "`own` differs from the server's",
    );
    assert!(
        <Rent as enshwick::services::Call>::LAYOUT[3].hash == 0xcb92_cc90_faad_3662,
        "`stall` differs from the server's",
    );
    assert!(
        <Rent as enshwick::services::Call>::LAYOUT[4].hash == 0x2d22_5a0a_1187_4bfd,
        "`place` differs from the server's",
    );
    assert!(
        <Rent as enshwick::services::Call>::OUTPUT[0].hash == 0x4685_e569_0ab2_dfdc,
        "the answer differs from the server's",
    );
    assert!(
        size_of::<Rent>() == 12,
        "the input's size differs from the server's"
    );
};

/// The fee for holding a stall, in coins.
#[enshwick::call(name = "fee", of = lockers, output = u64, version = 1)]
#[derive(Clone, Copy, Debug)]
pub struct Fee {
    /// The player's character.
    pub character: enshwick::Entity,
    /// How many days the stall is held.
    pub days: u32,
    /// The stall's place in its row.
    pub place: u16,
    /// Whether the stall is the player's own.
    pub own: bool,
    /// The stall's row.
    pub row: u8,
}

const _: () = {
    assert!(
        <Fee as enshwick::services::Call>::LAYOUT[0].hash == 0xb53b_31eb_b124_3978,
        "`character` differs from the server's",
    );
    assert!(
        <Fee as enshwick::services::Call>::LAYOUT[1].hash == 0x5300_d65b_4747_75e2,
        "`days` differs from the server's",
    );
    assert!(
        <Fee as enshwick::services::Call>::LAYOUT[2].hash == 0x6b17_e81c_2765_e03f,
        "`place` differs from the server's",
    );
    assert!(
        <Fee as enshwick::services::Call>::LAYOUT[3].hash == 0xf41d_4e0f_02b5_3194,
        "`own` differs from the server's",
    );
    assert!(
        <Fee as enshwick::services::Call>::LAYOUT[4].hash == 0xff32_53a4_8c32_88f3,
        "`row` differs from the server's",
    );
    assert!(
        <Fee as enshwick::services::Call>::OUTPUT[0].hash == 0x4685_e569_0ab2_dfdc,
        "the answer differs from the server's",
    );
    assert!(
        size_of::<Fee>() == 12,
        "the input's size differs from the server's"
    );
};

/// What a player does at a stall.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
#[shape(values(BUY, SELL))]
pub struct Deal(pub u32);

impl Deal {
    /// `BUY`, as `you.lockers` names it.
    pub const BUY: Self = Self(0);
    /// `SELL`, as `you.lockers` names it.
    pub const SELL: Self = Self(1);
}

/// What a stall sells.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
pub enum Stall {
    /// Food.
    Food = 1,
    /// Tools.
    Tools = 2,
}
