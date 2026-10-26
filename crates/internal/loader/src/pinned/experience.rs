//! Build table ids under `experience.`.

use super::{Constant, Symbol};

pub(crate) const GAINED: Constant = Constant::new("experience.gained");
pub(crate) const LEVEL_CAP: Symbol = Symbol::new("experience.level_cap");
pub(crate) const NEED: Constant = Constant::new("experience.need");
pub(crate) const NEED_FOR_LEVEL: Symbol = Symbol::new("experience.need_for_level");
pub(crate) const PROGRESS: Constant = Constant::new("experience.progress");
