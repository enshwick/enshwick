//! Build table ids under `session.`.

use super::{Constant, Symbol};

pub(crate) const HOST_MACHINE: Constant = Constant::new("session.host_machine");
pub(crate) const KICK: Symbol = Symbol::new("session.kick");
