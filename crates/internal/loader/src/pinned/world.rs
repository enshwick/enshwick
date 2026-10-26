//! Build table ids under `world.`.

use super::{Constant, Symbol};

pub(crate) const ENTITIES: Constant = Constant::new("world.entities");
pub(crate) const STEP: Symbol = Symbol::new("world.step");
pub(crate) const TIME: Symbol = Symbol::new("world.time");
