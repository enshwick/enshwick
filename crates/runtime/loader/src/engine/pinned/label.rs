//! Build table ids under `label.`.

use super::{Constant, Symbol};

pub(crate) const ADD: Symbol = Symbol::new("label.add");
pub(crate) const FIND: Symbol = Symbol::new("label.find");
pub(crate) const MAX_TEXT: Constant = Constant::new("label.max_text");
pub(crate) const RENAME_ENTITY: Constant = Constant::new("label.rename_entity");
pub(crate) const RENAME_RETURN: Symbol = Symbol::new("label.rename_return");
pub(crate) const REPLACE: Symbol = Symbol::new("label.replace");
