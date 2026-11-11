//! The traits that hold `Load`'s methods of each module on the base
//! handle; core's forwarders import them all.

pub use crate::content::LoadMethods as _;
pub use crate::content::changes::LoadMethods as _;
pub use crate::content::changes::items::LoadMethods as _;
pub use crate::content::changes::live::LoadMethods as _;
pub use crate::content::changes::recipes::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::content::enemies::LoadMethods as _;
pub use crate::content::icons::LoadMethods as _;
pub use crate::content::items::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::content::journal::LoadMethods as _;
#[cfg(feature = "typed")]
pub(crate) use crate::content::knowledge::LoadHelpers as _;
#[cfg(feature = "typed")]
pub use crate::content::knowledge::LoadMethods as _;
pub use crate::content::props::LoadMethods as _;
pub(crate) use crate::content::recipes::LoadHelpers as _;
pub use crate::content::recipes::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::content::removal::LoadMethods as _;
pub use crate::content::renames::LoadMethods as _;
pub use crate::content::script::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::content::skills::LoadMethods as _;
#[cfg(feature = "typed")]
pub use crate::content::spawns::LoadMethods as _;
pub use crate::content::templates::LoadMethods as _;
pub use crate::content::workshops::LoadMethods as _;
pub use crate::rates::LoadMethods as _;
