//! The call `you.stalls:rent` v2 of you.stalls 1.2.0.
//!
//! Written by `cargo enshwick call you.stalls:rent`. Write it again for a newer
//! you.stalls rather than edit it; the checks at the end hold it to the server's
//! layout. Declare this module `pub`.

/// The rent of a stall, in coins.
#[enshwick::call(name = "you.stalls:rent", output = u64, version = 2)]
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

/// What a player does at a stall.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
#[shape(values(BUY, SELL))]
pub struct Deal(pub u32);

impl Deal {
    /// `BUY`, as `you.stalls` names it.
    pub const BUY: Self = Self(0);
    /// `SELL`, as `you.stalls` names it.
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
