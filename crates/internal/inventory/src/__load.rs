//! The traits that hold `Load`'s methods of each module on the base
//! handle; core's forwarders import them all.

pub use crate::affixes::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::affixes::rolling::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::damage::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::damage::script::LoadMethods as _;
pub use crate::effects::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::entity_data::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::entity_data::script::LoadMethods as _;
pub use crate::inventory::LoadMethods as _;
pub(crate) use crate::item_data::LoadHelpers as _;
#[cfg(feature = "unstable")]
pub use crate::item_data::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::item_data::script::LoadMethods as _;
#[cfg(feature = "unstable")]
pub use crate::items::LoadMethods as _;
pub use crate::loot::LoadMethods as _;
pub use crate::loot::drops::LoadMethods as _;
pub use crate::loot::script::LoadMethods as _;
pub use crate::stats::LoadMethods as _;
pub use crate::survivors::LoadMethods as _;
pub use crate::survivors::script::LoadMethods as _;
