//! The extension point `you.stalls:price` of you.stalls 1.2.0, written by
//! `cargo enshwick point you.stalls:price`. Write it again for a newer you.stalls
//! rather than edit it; the checks at the end hold it to the owner's
//! layout. Declare this module `pub`.

/// A "fair" price, in coins.
#[enshwick::point(name = "you.stalls:price", mode = Override, output = u32, refusable)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    /// The player's character.
    pub character: enshwick::ecs::Entity,
    /// What the player does.
    pub deal: Deal,
    /// Whether the stall is the player's own.
    pub own: bool,
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
        <Price as enshwick::points::Point>::OUTPUT[0].hash == 0xcf08_9d7c_6717_36fe,
        "the answer differs from the owner's",
    );
};
