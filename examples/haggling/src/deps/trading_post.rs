//! What you.trading-post 0.0.0-dev offers other mods, by the alias `trading_post`.
//!
//! Written by `cargo enshwick deps trading_post`. Write it again for a newer
//! you.trading-post rather than edit it; the checks hold each declaration to the
//! owner's layout. `enshwick::deps!()` makes it `deps::trading_post`.

/// The price of an item at the trading post, in coins: the trading post's own price first.
///
/// A refusal means the trading post does not deal. Asked on the thread that runs the player's action.
#[enshwick::point(name = "price", of = trading_post, mode = Override, output = u32, refusable)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    /// The item, 0 to 63.
    pub item: u32,
    /// How many items the player trades.
    pub count: u32,
    /// The player's account.
    pub player: enshwick::PlayerKey,
    /// The player's character.
    pub character: enshwick::Entity,
    /// Whether the player buys or sells.
    pub deal: Deal,
}

const _: () = {
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[0].hash == 0x526a_6d69_25a4_bb52,
        "`item` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[1].hash == 0x6988_4f05_ffb7_79d5,
        "`count` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[2].hash == 0xf065_a0b6_cf87_b711,
        "`player` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[3].hash == 0xc58e_c15b_022f_9768,
        "`character` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::LAYOUT[4].hash == 0x525b_550b_ba64_1aa5,
        "`deal` differs from the owner's",
    );
    assert!(
        <Price as enshwick::points::Point>::OUTPUT[0].hash == 0xcf08_9d7c_6717_36fe,
        "the answer differs from the owner's",
    );
};

/// An item sold at the trading post, sent on the thread that sold it.
#[enshwick::event(name = "sold", of = trading_post)]
#[derive(Clone, Copy, Debug)]
pub struct Sold {
    /// The item sold.
    pub item: u32,
    /// What it sold for, in coins.
    pub price: u32,
}

const _: () = {
    assert!(
        <Sold as enshwick::services::ModEvent>::LAYOUT[0].hash == 0x526a_6d69_25a4_bb52,
        "`item` differs from the owner's",
    );
    assert!(
        <Sold as enshwick::services::ModEvent>::LAYOUT[1].hash == 0xbffc_7ec1_a88c_d167,
        "`price` differs from the owner's",
    );
};

/// What a player does at the trading post.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
#[shape(values(BUY, SELL))]
pub struct Deal(pub u32);

impl Deal {
    /// `BUY`, as `you.trading-post` names it.
    pub const BUY: Self = Self(0);
    /// `SELL`, as `you.trading-post` names it.
    pub const SELL: Self = Self(1);
}
