//! Build table ids under `voxel.`.

use super::{Constant, Symbol};

pub(crate) const READ_BOX: Symbol = Symbol::new("voxel.read_box");
pub(crate) const STORE: Constant = Constant::new("voxel.store");
pub(crate) const UNDO_APPLY: Symbol = Symbol::new("voxel.undo_apply");
