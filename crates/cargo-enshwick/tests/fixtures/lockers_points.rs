//! What you.lockers 1.2.0 offers other mods, by the alias `lockers`.
//!
//! Written by `cargo enshwick deps lockers`. Write it again for a newer
//! you.lockers rather than edit it; the checks hold each declaration to the
//! owner's layout. `enshwick::deps!()` makes it `deps::lockers`.

/// A "fair" price, in coins.
#[enshwick::point(name = "price", of = lockers, mode = Override, output = u32, refusable)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    /// The player's character.
    pub character: enshwick::Entity,
    /// What the player does.
    pub deal: Deal,
    /// Whether the stall is the player's own.
    pub own: bool,
    /// The stall's row.
    pub row: u8,
    /// The stall's place in its row.
    pub place: u16,
}

const _: () = {
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[0].hash == 0xb53b_31eb_b124_3978,
        "`character` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[1].hash == 0x4207_c59c_6958_bcb5,
        "`deal` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[2].hash == 0x3212_dc21_1893_c5d6,
        "`own` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[3].hash == 0xc13c_c592_7653_f4b1,
        "`row` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[4].hash == 0x2d22_5a0a_1187_4bfd,
        "`place` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::OUTPUT[0].hash == 0xcf08_9d7c_6717_36fe,
        "the answer differs from the owner's",
    );
};

/// A player opened a locker.
#[enshwick::event(name = "opened", of = lockers)]
#[derive(Clone, Copy, Debug)]
pub struct Opened {
    /// The player's character.
    pub character: enshwick::Entity,
    /// What the player does.
    pub deal: Deal,
}

const _: () = {
    assert!(
        <Opened as enshwick::services::ModEvent>::LAYOUT[0].hash == 0xb53b_31eb_b124_3978,
        "`character` differs from the owner's",
    );
    assert!(
        <Opened as enshwick::services::ModEvent>::LAYOUT[1].hash == 0x4207_c59c_6958_bcb5,
        "`deal` differs from the owner's",
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
