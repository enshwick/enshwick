//! Build table ids under `text.`.

use super::{Constant, Symbol};

pub(crate) const LOOKUP: Symbol = Symbol::new("text.lookup");
pub(crate) const LOOKUP_ENGLISH: Symbol = Symbol::new("text.lookup_english");
pub(crate) const POOL_LANGUAGE: Constant = Constant::new("text.pool_language");
