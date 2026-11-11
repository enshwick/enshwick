//! Build table ids under `user.`.

use super::{Constant, Symbol};

pub(crate) const INTERACTION_KIND: Constant = Constant::new("user.interaction_kind");
pub(crate) const JOIN_FAILED_KIND: Constant = Constant::new("user.join_failed_kind");
pub(crate) const START_INTERACTION: Symbol = Symbol::new("user.start_interaction");
