//! The ids of game content: GUIDs and content ids.

mod ids;
mod script;

#[doc(hidden)]
pub use enshwick_macros::id;
pub use ids::{DebugName, Guid, Id, NAMESPACE, is_id};
#[doc(hidden)]
pub use ids::{NAME_PART, part_name};
