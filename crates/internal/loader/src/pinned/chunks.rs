//! Build table ids under `chunks.`.

use super::{Constant, Symbol};

pub(crate) const HEIGHT: Constant = Constant::new("chunks.height");
pub(crate) const INDEX: Constant = Constant::new("chunks.index");
pub(crate) const MANAGER: Constant = Constant::new("chunks.manager");
pub(crate) const STEP: Symbol = Symbol::new("chunks.step");
pub(crate) const WIDTH: Constant = Constant::new("chunks.width");
