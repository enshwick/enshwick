//! The traits that hold `Load`'s methods of each module on the base
//! handle; core's forwarders import them all.

pub use crate::knowledge::LoadMethods as _;
pub use crate::progress::LoadMethods as _;
pub use crate::progress::script::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::quests::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::rewards::LoadMethods as _;
