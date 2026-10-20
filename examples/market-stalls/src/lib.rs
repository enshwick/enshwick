//! Example: raises `trading-post`'s prices for rare items through its
//! service, and counts the coins earned from its `trading-post.sold` event.

use std::sync::atomic::{AtomicU64, Ordering};

use enshwick_example_trading_post_api::{Prices, Sold};
use enshwick_loader_api::{Load, Mod, Off};

/// Items from this id on are rare.
const RARE: u32 = 32;

static EARNED: AtomicU64 = AtomicU64::new(0);

/// Coins earned at the trading post since start.
#[must_use]
pub fn earned() -> u64 {
    EARNED.load(Ordering::Relaxed)
}

/// The example market stalls.
#[derive(Debug)]
pub struct MarketStalls;

impl Mod for MarketStalls {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "market-stalls",
            side: SERVER,
            requires: ["trading-post" => ">=0.1, <1.0"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let prices: &'static Prices = load.service()?;
        for item in RARE..RARE * 2 {
            let base = (prices.price)(item);
            let _old = (prices.set_price)(item, base.saturating_mul(3));
        }
        load.on_mod_event(|sold: &Sold| {
            let total = EARNED.fetch_add(u64::from(sold.price), Ordering::Relaxed);
            enshwick_loader_api::info!(
                "item {} sold for {}; {} coins earned",
                sold.item,
                sold.price,
                total + u64::from(sold.price)
            );
        })?;
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(MarketStalls);

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick_example_trading_post_api::{Prices, Sold};
    use enshwick_loader_api::abi::Status;

    static PRICE: AtomicU32 = AtomicU32::new(10);

    extern "system" fn price(_item: u32) -> u32 {
        PRICE.load(Ordering::Relaxed)
    }

    extern "system" fn set_price(_item: u32, price: u32) -> u32 {
        PRICE.swap(price, Ordering::Relaxed)
    }

    static PRICES: Prices = Prices { price, set_price };

    #[test]
    fn raises_rare_prices_and_counts_sales() {
        let loader = enshwick_loader_api::test::FakeHost::new()
            .service(&PRICES)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(PRICE.load(Ordering::Relaxed) > 10);
        assert_eq!(
            loader.send_mod_event(&Sold {
                item: 40,
                price: 30
            }),
            1
        );
        assert_eq!(super::earned(), 30);
    }

    #[test]
    fn without_trading_post_it_turns_off() {
        let loader = enshwick_loader_api::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_ne!(status, Status::OK);
    }
}
