//! Build table ids under `net.`.

use super::{Constant, Symbol};

pub(crate) const JOIN_GAME: Symbol = Symbol::new("net.join_game");
pub(crate) const PROVIDER_JOIN_SLOT: Constant = Constant::new("net.provider_join_slot");
pub(crate) const PROVIDER_VTABLE: Symbol = Symbol::new("net.provider_vtable");
