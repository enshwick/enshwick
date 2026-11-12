//! The traits that hold `Load`'s methods of each module on the base
//! handle; core's forwarders import them all.

#[cfg(feature = "unstable")]
pub use crate::actions::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::actions::script::LoadMethods as _;
pub use crate::chat::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::controls::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::controls::script::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::input::LoadMethods as _;
pub use crate::interactions::LoadMethods as _;
pub use crate::vetoes::LoadMethods as _;
pub use crate::vetoes::script::LoadMethods as _;
