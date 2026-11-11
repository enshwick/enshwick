//! Build table ids under `save.`.

use super::{Constant, Symbol};

pub(crate) const FLAGS: Constant = Constant::new("save.flags");
pub(crate) const PATH: Symbol = Symbol::new("save.path");
pub(crate) const PATH_DATA: Constant = Constant::new("save.path_data");
pub(crate) const PATH_LENGTHS: Constant = Constant::new("save.path_lengths");
pub(crate) const PATH_PARTS: Constant = Constant::new("save.path_parts");
pub(crate) const PHASE: Constant = Constant::new("save.phase");
