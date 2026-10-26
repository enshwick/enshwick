//! Build table ids under `building.`.

use super::{Constant, Symbol};

pub(crate) const CORNER_SIZE: Constant = Constant::new("building.corner_size");
pub(crate) const CTX_PLAYER: Constant = Constant::new("building.ctx_player");
pub(crate) const CTX_REPLAY: Constant = Constant::new("building.ctx_replay");
pub(crate) const CURSOR_CHECK: Symbol = Symbol::new("building.cursor_check");
pub(crate) const CURSOR_ITEM_INFO: Constant = Constant::new("building.cursor_item_info");
pub(crate) const CURSOR_NO_PERMISSION: Constant = Constant::new("building.cursor_no_permission");
pub(crate) const IN_BUILDING_AREA: Symbol = Symbol::new("building.in_building_area");
pub(crate) const MAX_ALTARS: Symbol = Symbol::new("building.max_altars");
pub(crate) const NO_BUILD_TEST: Symbol = Symbol::new("building.no_build_test");
pub(crate) const PICKUP: Symbol = Symbol::new("building.pickup");
pub(crate) const PICKUP_ENTITY: Constant = Constant::new("building.pickup_entity");
pub(crate) const PICKUP_MODE: Constant = Constant::new("building.pickup_mode");
pub(crate) const SEAM_CHECK: Symbol = Symbol::new("building.seam_check");
pub(crate) const SPAWN_PROP: Symbol = Symbol::new("building.spawn_prop");
pub(crate) const ZONE_ENTRY_IGNORED: Constant = Constant::new("building.zone_entry_ignored");
pub(crate) const ZONE_ENTRY_SHAPE: Constant = Constant::new("building.zone_entry_shape");
pub(crate) const ZONE_ENTRY_SIZE: Constant = Constant::new("building.zone_entry_size");
pub(crate) const ZONE_LIST_COUNT: Constant = Constant::new("building.zone_list_count");
pub(crate) const ZONE_LIST_ENTRIES: Constant = Constant::new("building.zone_list_entries");
pub(crate) const ZONE_TOUCHES: Symbol = Symbol::new("building.zone_touches");
pub(crate) const BLOCK_BOX: Symbol = Symbol::new("building.block_box");
pub(crate) const BLUEPRINT_LOOKUP: Symbol = Symbol::new("building.blueprint_lookup");
