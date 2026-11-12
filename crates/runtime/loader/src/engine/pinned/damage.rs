//! Build table ids under `damage.`.

use super::{Constant, Symbol};

pub(crate) const ADMIN_KILL: Symbol = Symbol::new("damage.admin_kill");
pub(crate) const DROWNING: Symbol = Symbol::new("damage.drowning");
pub(crate) const DROWNING_DEATH: Symbol = Symbol::new("damage.drowning_death");
pub(crate) const FACTION_KILL: Symbol = Symbol::new("damage.faction_kill");
pub(crate) const FALL: Symbol = Symbol::new("damage.fall");
pub(crate) const FREEZING: Symbol = Symbol::new("damage.freezing");
pub(crate) const HIT: Symbol = Symbol::new("damage.hit");
pub(crate) const HIT_SOURCE: Constant = Constant::new("damage.hit_source");
pub(crate) const HIT_TARGET: Constant = Constant::new("damage.hit_target");
pub(crate) const HIT_VECTOR: Constant = Constant::new("damage.hit_vector");
pub(crate) const SHROUD: Symbol = Symbol::new("damage.shroud");
pub(crate) const SHROUD_DEATH: Symbol = Symbol::new("damage.shroud_death");
