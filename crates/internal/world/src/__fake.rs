//! What the fake loader of `enshwick-fake` and core's tests read inside
//! this crate; only the `fake` feature, which core's turns on, builds it.

use crate::abi::ModHandle;
use crate::voxels::{EditSource, Voxel};
#[cfg(feature = "unstable")]
use crate::world::HeldState;

/// The `TransactionInfo::HOLDS_` bit of `state`.
#[cfg(feature = "unstable")]
#[must_use]
pub fn held_bit(state: HeldState) -> u32 {
    state.bit()
}

/// `voxel` as the loader writes it, and reads it from a mod.
#[must_use]
pub const fn raw_voxel(voxel: Voxel) -> crate::abi::Voxel {
    voxel.to_abi()
}

/// The handle a write of the world's voxels by `source` comes under for the
/// mod `me`, the inverse of how an edit's source is read: the game writes
/// under none, another mod under the handle after `me`.
#[must_use]
pub fn edit_handle(source: EditSource, me: ModHandle) -> ModHandle {
    match source {
        EditSource::Game => ModHandle::NONE,
        EditSource::ThisMod => me,
        EditSource::OtherMod => ModHandle(me.0.wrapping_add(1).max(1)),
    }
}
