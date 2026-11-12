//! The traits that hold `Load`'s methods of each module on the base
//! handle; core's forwarders import them all.

pub use crate::altars::LoadMethods as _;
pub use crate::altars::kinds::LoadMethods as _;
pub use crate::altars::limits::LoadMethods as _;
pub use crate::buffs::LoadMethods as _;
pub use crate::daytime::LoadMethods as _;
pub use crate::game_settings::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::map::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::map::events::LoadMethods as _;
pub use crate::terrain::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::typed::runtime::script::LoadMethods as _;
pub use crate::voxels::LoadMethods as _;
pub use crate::voxels::edits::LoadMethods as _;
pub use crate::weather::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::weather::phenomena::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::world::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::world::fields::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::world::rename::LoadMethods as _;
