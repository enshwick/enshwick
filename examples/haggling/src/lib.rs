//! Example: changes what the trading post pays at its extension point,
//! `you.trading-post:price`. Regular customers get more for what they sell,
//! and the trading post buys no item on the refused list.
//!
//! The definition of the point in [`price`] is written by
//! `cargo enshwick point you.trading-post:price --from <the trading post's DLL>`.

use std::sync::Arc;

use arc_swap::ArcSwap;
use enshwick::points::{Order, Verdict};
use enshwick::state::ModLocal;
use enshwick::{Load, Mod, Off};

pub mod price;

use price::{Deal, Price};

/// Haggling settings.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Account keys, 16 hex digits, of regular customers.
    #[config(runtime)]
    regulars: Vec<String>,
    /// How much more regulars get for what they sell, in percent.
    #[config(runtime, range = 0..=100)]
    bonus_percent: u32,
    /// Items the trading post buys from nobody.
    #[config(runtime)]
    refused_items: Vec<u32>,
}

/// The rules the link reads, swapped whole when the settings change.
#[derive(Debug, Default)]
struct Rules {
    regulars: Vec<u64>,
    bonus_percent: u32,
    refused_items: Vec<u32>,
}

impl Rules {
    fn from_settings(settings: &Settings) -> Self {
        let regulars = settings
            .regulars
            .iter()
            .filter_map(|key| {
                let parsed = u64::from_str_radix(key.trim(), 16).ok();
                if parsed.is_none() {
                    enshwick::warn!("regular {key:?} is no account key; skipped");
                }
                parsed
            })
            .collect();
        Self {
            regulars,
            bonus_percent: settings.bonus_percent,
            refused_items: settings.refused_items.clone(),
        }
    }

    /// The link's answer: the trading post's own price, `paid`, comes first.
    fn judge(&self, price: &Price, paid: u32) -> Verdict<u32> {
        if price.deal != Deal::SELL {
            return Verdict::Pass;
        }
        if self.refused_items.contains(&price.item) {
            return Verdict::Refuse;
        }
        if self.bonus_percent > 0 && self.regulars.contains(&price.player) {
            let more = paid.saturating_mul(self.bonus_percent) / 100;
            return Verdict::Set(paid.saturating_add(more));
        }
        Verdict::Pass
    }
}

static RULES: ModLocal<ArcSwap<Rules>> = ModLocal::new();

fn rules() -> &'static ArcSwap<Rules> {
    RULES.get_or_init(|| ArcSwap::from_pointee(Rules::default()))
}

/// The example's state.
#[derive(Debug)]
pub struct Haggling;

impl Mod for Haggling {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "you.haggling",
            side: SERVER,
            recommends: ["you.trading-post" => ""],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        rules().store(Arc::new(Rules::from_settings(&settings)));
        load.on_config_change(|settings: Settings| {
            rules().store(Arc::new(Rules::from_settings(&settings)));
        })?;
        load.extend_point(&Order::new(), |price: &Price, paid: &u32| {
            rules().load().judge(price, *paid)
        })?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Haggling);

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::ecs::Entity;
    use enshwick::points::Decision;

    use super::price::{Deal, Price};

    const REGULAR: u64 = 0x00ab_cdef_0123_4567;

    fn price(player: u64, item: u32, deal: Deal) -> Price {
        Price {
            item,
            player,
            character: Entity(10),
            deal,
        }
    }

    #[test]
    fn regulars_get_more_and_refused_items_are_not_bought() {
        let loader = enshwick::test::FakeHost::new()
            .settings(
                "regulars = [\"00abcdef01234567\", \"nope\"]\nbonus_percent = 20\nrefused_items = [5]\n",
            )
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Haggling)`.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.attached_points(), ["you.trading-post:price"]);
        assert_eq!(
            loader.ask_point(&price(REGULAR, 1, Deal::SELL), 10),
            Decision::Changed(12)
        );
        assert_eq!(
            loader.ask_point(&price(7, 1, Deal::SELL), 10),
            Decision::Kept(10)
        );
        assert_eq!(
            loader.ask_point(&price(7, 5, Deal::SELL), 10),
            Decision::Refused
        );
        assert_eq!(
            loader.ask_point(&price(REGULAR, 5, Deal::BUY), 10),
            Decision::Kept(10)
        );
        assert!(loader.faults().is_empty());
    }
}
