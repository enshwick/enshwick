//! Affixes as a script sees them: the namespace `affixes`, with the pool's
//! lines and rolls and the pure [`roll`](super::roll) and
//! [`reroll`](super::reroll).

use super::{AffixDef, AffixKey, RollRules, Rolled};
use crate::stats::ItemStats;

mod record {
    /// One affix of the pool a roll draws from.
    #[derive(Clone, Debug, crate::Script)]
    pub struct Candidate {
        /// The affix's key.
        pub key: super::AffixKey,
        /// Its definition.
        pub def: super::AffixDef,
    }
}

/// `pool` as [`roll`](super::roll) borrows it.
fn borrowed(pool: &[record::Candidate]) -> Vec<super::Candidate<'_>> {
    pool.iter()
        .map(|c| super::Candidate {
            key: c.key,
            def: &c.def,
        })
        .collect()
}

/// The free functions of [`affixes`](super) as a script's namespace
/// `affixes`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct AffixFns;

#[crate::script(affix_fns)]
#[expect(
    clippy::unused_self,
    clippy::needless_pass_by_value,
    reason = "a script call takes its namespace and hands its values over as its own copies"
)]
impl AffixFns {
    /// The affixes an item of the game's `ItemCategory` numbered `category`
    /// rolls from `pool` under `rules`, from `seed`, as
    /// [`roll`](super::roll) rolls them; each of `pool` is `{ key, def }`.
    /// Its name differs from Rust's, since `roll` rolls an item, as
    /// `Affixes::roll` does.
    #[script(name = "roll_pool")]
    #[must_use]
    pub fn roll(
        self,
        pool: Vec<record::Candidate>,
        category: u8,
        rules: RollRules,
        seed: u64,
    ) -> Vec<Rolled> {
        super::roll(&borrowed(&pool), category, rules, seed)
    }

    /// `stats` rolled anew from `pool`, as [`reroll`](super::reroll) rolls
    /// them; nil when `once` and it rolled before.
    #[script]
    #[must_use]
    pub fn reroll(
        self,
        stats: ItemStats,
        pool: Vec<record::Candidate>,
        category: u8,
        rules: RollRules,
        seed: u64,
        once: bool,
    ) -> Option<ItemStats> {
        super::reroll(&stats, &borrowed(&pool), category, rules, seed, once)
    }
}
