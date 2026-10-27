//! Build table ids under `map.`.

use super::{Constant, Symbol};

pub(crate) const FOG_REVEAL: Symbol = Symbol::new("map.fog_reveal");
pub(crate) const FOG_SIZE_Z: Constant = Constant::new("map.fog_size_z");
pub(crate) const FOG_TILES: Constant = Constant::new("map.fog_tiles");
pub(crate) const FOG_TILES_WIDE: Constant = Constant::new("map.fog_tiles_wide");
pub(crate) const FOG_TILE_EDGE_SHIFT: Constant = Constant::new("map.fog_tile_edge_shift");
pub(crate) const FOG_TILE_SHIFT: Constant = Constant::new("map.fog_tile_shift");
