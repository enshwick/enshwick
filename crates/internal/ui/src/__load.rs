//! The traits that hold `Load`'s methods of each module on the base
//! handle; core's forwarders import them all.

pub use crate::sound::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::sound::add::LoadMethods as _;
pub use crate::sound::script::LoadMethods as _;
pub use crate::ui::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::ui::extension::LoadMethods as _;
pub use crate::ui::hud::LoadMethods as _;
pub use crate::ui::script::LoadMethods as _;
