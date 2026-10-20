//! What `trading-post` shares with the mods built on it: its service table
//! and its event, declared once. Both mods compile this crate, so the layout
//! hashes the loader compares come from one declaration.
//!
//! Grow the table by appending fields: mods built against an older version
//! of this crate keep working with a newer `trading-post`.

/// Item prices, published by `trading-post`.
#[enshwick_loader_api::service(name = "trading-post.prices", version = 1)]
#[derive(Debug)]
pub struct Prices {
    /// The price of `item` in coins, 0 for an item with no price.
    pub price: extern "system" fn(item: u32) -> u32,
    /// Sets the price of `item`; returns the old one.
    pub set_price: extern "system" fn(item: u32, price: u32) -> u32,
}

/// An item sold at the trading post, sent by `trading-post` on the thread
/// that sold it.
#[enshwick_loader_api::event(name = "trading-post.sold")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sold {
    /// The item sold.
    pub item: u32,
    /// What it sold for, in coins.
    pub price: u32,
}
