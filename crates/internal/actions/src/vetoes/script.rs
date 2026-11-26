//! `Load`'s script-only method of this module, out of a mod's reach.

use super::{Placement, Verdict};
use crate::Off;
use crate::game_events::CraftRecipeEvent;

/// `Load`'s script-only methods of this module on the base handle, which
/// core's forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::craft_filter`.
    #[doc(hidden)]
    fn craft_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> Verdict + Send + Sync + 'static;

    /// As `Load::build_filter`.
    #[doc(hidden)]
    fn build_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> Verdict + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn craft_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> Verdict + Send + Sync + 'static,
    {
        super::craft_filter(self, filter)
    }

    fn build_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> Verdict + Send + Sync + 'static,
    {
        super::build_filter(self, filter)
    }
}
