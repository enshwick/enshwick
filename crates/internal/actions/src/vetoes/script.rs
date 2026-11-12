//! `Load`'s script-only method of this module, out of a mod's reach.

use super::{BuildVerdict, Placement};
use crate::Off;

/// `Load`'s script-only methods of this module on the base handle, which
/// core's forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::build_filter`.
    #[doc(hidden)]
    fn build_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> BuildVerdict + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn build_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> BuildVerdict + Send + Sync + 'static,
    {
        super::build_filter(self, filter)
    }
}
