//! Build table ids under `weather.`.

use super::{Constant, Symbol};

pub(crate) const CLOCK: Symbol = Symbol::new("weather.clock");
pub(crate) const ENTRY_DATA: Constant = Constant::new("weather.entry_data");
pub(crate) const ENTRY_GENERATION: Constant = Constant::new("weather.entry_generation");
pub(crate) const ENTRY_SIZE: Constant = Constant::new("weather.entry_size");
pub(crate) const FREE: Symbol = Symbol::new("weather.free");
pub(crate) const GENERATION: Constant = Constant::new("weather.generation");
pub(crate) const INSTANCE_DESPAWN: Constant = Constant::new("weather.instance_despawn");
pub(crate) const INSTANCE_PHENOMENON: Constant = Constant::new("weather.instance_phenomenon");
pub(crate) const INSTANCE_POSITION: Constant = Constant::new("weather.instance_position");
pub(crate) const INSTANCE_RADIUS: Constant = Constant::new("weather.instance_radius");
pub(crate) const LAYERS: Constant = Constant::new("weather.layers");
pub(crate) const LAYER_COUNT: Constant = Constant::new("weather.layer_count");
pub(crate) const MANAGER: Constant = Constant::new("weather.manager");
pub(crate) const PAGES: Constant = Constant::new("weather.pages");
pub(crate) const PAGE_COUNT: Constant = Constant::new("weather.page_count");
pub(crate) const PAGE_ENTRIES: Constant = Constant::new("weather.page_entries");
pub(crate) const PAGE_ENTRY_COUNT: Constant = Constant::new("weather.page_entry_count");
pub(crate) const PAGE_MASK: Constant = Constant::new("weather.page_mask");
pub(crate) const PAGE_SHIFT: Constant = Constant::new("weather.page_shift");
pub(crate) const PAGE_SIZE: Constant = Constant::new("weather.page_size");
pub(crate) const RESOURCE: Constant = Constant::new("weather.resource");
pub(crate) const RESOURCE_LOCK: Symbol = Symbol::new("weather.resource_lock");
pub(crate) const RESOURCE_RELEASE: Symbol = Symbol::new("weather.resource_release");
pub(crate) const SAMPLE: Symbol = Symbol::new("weather.sample");
pub(crate) const SLOT_CURRENT: Constant = Constant::new("weather.slot_current");
pub(crate) const SLOT_NEXT: Constant = Constant::new("weather.slot_next");
pub(crate) const SLOT_PHENOMENON: Constant = Constant::new("weather.slot_phenomenon");
pub(crate) const SLOT_SIZE: Constant = Constant::new("weather.slot_size");
pub(crate) const SLOT_TIME: Constant = Constant::new("weather.slot_time");
pub(crate) const SPAWN: Symbol = Symbol::new("weather.spawn");
pub(crate) const SYSTEM: Constant = Constant::new("weather.system");
