//! Build table ids under `map.`.

use super::{Constant, Symbol};

pub(crate) const CONTEXT_STATE: Constant = Constant::new("map.context_state");
pub(crate) const ENTRY_COLOR: Constant = Constant::new("map.entry_color");
pub(crate) const ENTRY_LABEL: Constant = Constant::new("map.entry_label");
pub(crate) const ENTRY_POSITION: Constant = Constant::new("map.entry_position");
pub(crate) const ENTRY_SIZE: Constant = Constant::new("map.entry_size");
pub(crate) const ENTRY_TYPE: Constant = Constant::new("map.entry_type");
pub(crate) const ENTRY_VISIBILITY: Constant = Constant::new("map.entry_visibility");
pub(crate) const FOG_DESTROY: Symbol = Symbol::new("map.fog_destroy");
pub(crate) const FOG_REVEAL: Symbol = Symbol::new("map.fog_reveal");
pub(crate) const FOG_SIZE_Z: Constant = Constant::new("map.fog_size_z");
pub(crate) const FOG_TILES: Constant = Constant::new("map.fog_tiles");
pub(crate) const FOG_TILES_WIDE: Constant = Constant::new("map.fog_tiles_wide");
pub(crate) const FOG_TILE_EDGE_SHIFT: Constant = Constant::new("map.fog_tile_edge_shift");
pub(crate) const FOG_TILE_SHIFT: Constant = Constant::new("map.fog_tile_shift");
pub(crate) const LIST: Constant = Constant::new("map.list");
pub(crate) const LIST_CAPACITY: Constant = Constant::new("map.list_capacity");
pub(crate) const LIST_COUNT: Constant = Constant::new("map.list_count");
pub(crate) const REGISTRY: Constant = Constant::new("map.registry");
pub(crate) const REGISTRY_BLOB: Constant = Constant::new("map.registry_blob");
pub(crate) const STATE_PLAY_DATA: Constant = Constant::new("map.state_play_data");
