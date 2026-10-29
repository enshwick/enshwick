//! Build table ids under `voxel.`.

use super::{Constant, Symbol};

pub(crate) const BUILDING_FIRST: Constant = Constant::new("voxel.building_first");
pub(crate) const MATERIALS: Constant = Constant::new("voxel.materials");
pub(crate) const READ_BOX: Symbol = Symbol::new("voxel.read_box");
pub(crate) const STORE: Constant = Constant::new("voxel.store");
pub(crate) const UNDO_APPLY: Symbol = Symbol::new("voxel.undo_apply");
pub(crate) const UNDO_EXTENT: Constant = Constant::new("voxel.undo_extent");
pub(crate) const FINEST_LEVEL: Constant = Constant::new("voxel.finest_level");
pub(crate) const GRIDS: Constant = Constant::new("voxel.grids");
pub(crate) const GRID_PAGES: Constant = Constant::new("voxel.grid_pages");
pub(crate) const GRID_STRIDE: Constant = Constant::new("voxel.grid_stride");
pub(crate) const WRITE_BLOCK: Symbol = Symbol::new("voxel.write_block");
