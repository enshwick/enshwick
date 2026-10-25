//! Example: publishes item prices as a typed service and sends
//! `trading-post:sold` when a player sells an item with the `sell` action.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use enshwick_example_trading_post_api::{Prices, Sold};
use enshwick_loader_api::actions::{Action, Arg, Outcome, Value};
use enshwick_loader_api::i18n::Texts;
use enshwick_loader_api::services::Emitter;
use enshwick_loader_api::{Load, Mod, Off};

/// Items with a price: ids 0 to 63.
const ITEMS: usize = 64;

static TABLE: [AtomicU32; ITEMS] = [const { AtomicU32::new(10) }; ITEMS];
static SOLD: OnceLock<Emitter<Sold>> = OnceLock::new();

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

/// Sells `item`: sends the event to every subscriber.
fn sell(item: u32) -> Outcome {
    let price = price(item);
    if let Some(sold) = SOLD.get() {
        sold.send(&Sold { item, price });
    }
    Outcome::done(format!("sold item {item} for {price} coins"))
}

/// The example trading post.
#[derive(Debug)]
pub struct TradingPost;

impl Mod for TradingPost {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(id: "trading-post", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.publish(&PRICES)?;
        let _first = SOLD.set(load.define_event::<Sold>()?);
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
                [Value::Number(item)] => sell(*item as u32),
                _ => Outcome::refused("pick an item"),
            },
        )?;
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(TradingPost);

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Status;

    #[test]
    fn publishes_prices_and_defines_sold() {
        let loader = enshwick_loader_api::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.published(), [("trading-post:prices".to_owned(), 1)]);
        assert_eq!(loader.defined_events(), ["trading-post:sold"]);
        assert_eq!(super::sell(3).text(), "sold item 3 for 10 coins");
        assert_eq!(loader.sent_events(), ["trading-post:sold"]);
    }
}
