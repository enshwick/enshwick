//! Build table ids under `recipe.`.

use super::Symbol;

pub(crate) const LIMIT_CHECK: Symbol = Symbol::new("recipe.limit_check");
pub(crate) const LIMIT_NEXT: Symbol = Symbol::new("recipe.limit_next");
pub(crate) const LIMIT_STOP: Symbol = Symbol::new("recipe.limit_stop");
pub(crate) const LIST_LOOKUP: Symbol = Symbol::new("recipe.list_lookup");
