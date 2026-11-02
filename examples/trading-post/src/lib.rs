//! Example: publishes item prices as a typed service, sends
//! `you.trading-post:sold` when a player sells an item with the `sell` action,
//! and lets other mods change what it pays at its extension point
//! `you.trading-post:price`.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick::actions::{Action, Arg, Outcome, Value};
use enshwick::i18n::Texts;
use enshwick::points::OwnedPoint;
use enshwick::services::Emitter;
use enshwick::state::ModLocal;
use enshwick::{Entity, PlayerKey};
use enshwick::{Load, Mod, Off};

/// Item prices, published for other mods.
#[enshwick::service(name = "prices", version = 1)]
#[derive(Debug)]
pub struct Prices {
    /// The price of `item` in coins, 0 for an item with no price.
    pub price: extern "system" fn(item: u32) -> u32,
    /// Sets the price of `item`; returns the old one.
    pub set_price: extern "system" fn(item: u32, price: u32) -> u32,
}

/// An item sold at the trading post, sent on the thread that sold it.
#[enshwick::event(name = "sold")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sold {
    /// The item sold.
    pub item: u32,
    /// What it sold for, in coins.
    pub price: u32,
}

/// Items with a price: ids 0 to 63.
const ITEMS: usize = 64;

static TABLE: [AtomicU32; ITEMS] = [const { AtomicU32::new(10) }; ITEMS];
static SOLD: ModLocal<Emitter<Sold>> = ModLocal::new();

extern "system" fn price(item: u32) -> u32 {
    usize::try_from(item)
        .ok()
        .and_then(|i| TABLE.get(i))
        .map_or(0, |p| p.load(Ordering::Relaxed))
}

extern "system" fn set_price(item: u32, price: u32) -> u32 {
    usize::try_from(item)
        .ok()
        .and_then(|i| TABLE.get(i))
        .map_or(0, |p| p.swap(price, Ordering::Relaxed))
}

static PRICES: Prices = Prices { price, set_price };

/// What a player does at the trading post.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
#[shape(values(BUY, SELL))]
pub struct Deal(pub u32);

impl Deal {
    /// The player buys from the trading post.
    pub const BUY: Self = Self(0);
    /// The player sells to the trading post.
    pub const SELL: Self = Self(1);
}

/// The price of an item at the trading post, in coins: the trading post's
/// own price first. A refusal means the trading post does not deal. Asked
/// on the thread that runs the player's action.
#[enshwick::point(name = "price", mode = Override, output = u32, refusable)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    /// The item, 0 to 63.
    pub item: u32,
    /// How many items the player trades.
    pub count: u32,
    /// The player's account.
    pub player: PlayerKey,
    /// The player's character.
    pub character: Entity,
    /// Whether the player buys or sells.
    pub deal: Deal,
}

static PRICE: ModLocal<OwnedPoint<Price>> = ModLocal::new();

/// Buys `item` from the player `price` describes, at the price the
/// trading post and the mods extending it agree on, and sends the event
/// to every subscriber.
fn sell(asked: &Price) -> Outcome {
    let own = price(asked.item);
    let decision = PRICE.get().map(|point| point.decide(asked, own));
    let paid = match decision {
        Some(decision) if decision.is_refused() => {
            return Outcome::refused(format!("the trading post does not buy item {}", asked.item));
        }
        Some(decision) => decision.value_or(own),
        None => own,
    };
    if let Some(sold) = SOLD.get() {
        sold.send(&Sold {
            item: asked.item,
            price: paid,
        });
    }
    Outcome::done(format!("sold item {} for {paid} coins", asked.item))
}

/// The example trading post.
#[derive(Debug)]
pub struct TradingPost;

impl Mod for TradingPost {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.publish(&PRICES)?;
        let _first = SOLD.set(load.define_event::<Sold>()?);
        let _first = PRICE.set(load.define_point::<Price>()?);
        #[expect(clippy::cast_precision_loss, reason = "a small count")]
        let last = (ITEMS - 1) as f64;
        load.actions(Texts::default())?.add(
            Action::new("sell", "Sell an item").arg(Arg::number("Item", 0.0..=last, 1.0)),
            |request| match request.args {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the argument's range is within the items"
                )]
                [Value::Number(item)] => sell(&Price {
                    item: *item as u32,
                    count: 1,
                    player: request.player.key,
                    character: request.player.entity,
                    deal: Deal::SELL,
                }),
                _ => Outcome::refused("pick an item"),
            },
        )?;
        Ok(Self)
    }
}

enshwick::declare_mod!(TradingPost);

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::points::Verdict;
    use enshwick::{Entity, PlayerKey};

    use super::{Deal, Price};

    fn selling(item: u32) -> Price {
        Price {
            item,
            count: 1,
            player: PlayerKey::new(7),
            character: Entity(10),
            deal: Deal::SELL,
        }
    }

    #[test]
    fn the_published_schema_is_the_manifest_s() {
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let manifest = unsafe { &*super::enshwick_mod_query() };
        // SAFETY: the manifest's static text.
        let text = unsafe { manifest.points.as_str() }.unwrap_or_default();
        assert_eq!(text, include_str!("../points.toml"));
    }

    #[test]
    fn a_link_changes_or_refuses_the_price() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(TradingPost)`.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.defined_points(), ["you.trading-post:price"]);
        let attached = loader.extend_point(|price: &Price, _paid: &u32| match price.item {
            1 => Verdict::Set(12),
            2 => Verdict::Refuse,
            _ => Verdict::Pass,
        });
        assert!(attached);
        assert_eq!(super::sell(&selling(1)).text(), "sold item 1 for 12 coins");
        assert_eq!(
            super::sell(&selling(2)).text(),
            "the trading post does not buy item 2"
        );
    }

    #[test]
    fn publishes_prices_and_defines_sold() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this crate's `declare_mod!(TradingPost)`.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(
            loader.published(),
            [("you.trading-post:prices".to_owned(), 1)]
        );
        assert_eq!(loader.defined_events(), ["you.trading-post:sold"]);
        assert_eq!(super::sell(&selling(3)).text(), "sold item 3 for 10 coins");
        assert_eq!(loader.sent_events(), ["you.trading-post:sold"]);
    }
}
