//! Build table ids under `net.`.

use super::{Constant, Symbol};

pub(crate) const CHANNEL_BASE: Constant = Constant::new("net.channel_base");
pub(crate) const CLIENT: Constant = Constant::new("net.client");
pub(crate) const JOIN_GAME: Symbol = Symbol::new("net.join_game");
pub(crate) const MESSAGE_ABORT: Symbol = Symbol::new("net.message_abort");
pub(crate) const MESSAGE_COMMIT: Symbol = Symbol::new("net.message_commit");
pub(crate) const MESSAGE_OPEN: Symbol = Symbol::new("net.message_open");
pub(crate) const MESSAGE_RECEIVE: Symbol = Symbol::new("net.message_receive");
pub(crate) const MESSAGE_RELEASE: Symbol = Symbol::new("net.message_release");
pub(crate) const PROVIDER_VTABLE: Symbol = Symbol::new("net.provider_vtable");
pub(crate) const SESSION_LIMIT: Symbol = Symbol::new("net.session_limit");
pub(crate) const SESSION_MANAGER: Constant = Constant::new("net.session_manager");
pub(crate) const SOCKET_CHANNEL: Constant = Constant::new("net.socket_channel");
pub(crate) const SOCKET_CREATE: Symbol = Symbol::new("net.socket_create");
pub(crate) const SOCKET_DESTROY: Symbol = Symbol::new("net.socket_destroy");
pub(crate) const SOCKET_KIND_LOCAL_MACHINE: Constant =
    Constant::new("net.socket_kind_local_machine");
pub(crate) const SOCKET_KIND_MACHINE: Constant = Constant::new("net.socket_kind_machine");
pub(crate) const SOCKET_PEER: Constant = Constant::new("net.socket_peer");
