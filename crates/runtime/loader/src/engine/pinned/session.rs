//! Build table ids under `session.`.

use super::{Constant, Symbol};

pub(crate) const FIND_MACHINE: Symbol = Symbol::new("session.find_machine");
pub(crate) const HOST_MACHINE: Constant = Constant::new("session.host_machine");
pub(crate) const JOIN_ACCOUNT_ID: Constant = Constant::new("session.join_account_id");
pub(crate) const JOIN_ACCOUNT_KIND: Constant = Constant::new("session.join_account_kind");
pub(crate) const JOIN_ACCOUNT_SIZE: Constant = Constant::new("session.join_account_size");
pub(crate) const JOIN_ACCOUNTS: Constant = Constant::new("session.join_accounts");
pub(crate) const JOIN_COUNT: Constant = Constant::new("session.join_count");
pub(crate) const JOIN_GAME: Symbol = Symbol::new("session.join_game");
pub(crate) const JOIN_REPLY: Symbol = Symbol::new("session.join_reply");
pub(crate) const KICK: Symbol = Symbol::new("session.kick");
pub(crate) const MESSAGE_SIZE: Constant = Constant::new("session.message_size");
pub(crate) const PEER_SLOT_CLOSE: Constant = Constant::new("session.peer_slot_close");
pub(crate) const PEER_SLOT_OPEN: Constant = Constant::new("session.peer_slot_open");
pub(crate) const PEER_SLOT_SIZE: Constant = Constant::new("session.peer_slot_size");
pub(crate) const PEER_SLOTS: Constant = Constant::new("session.peer_slots");
pub(crate) const PLATFORM: Constant = Constant::new("session.platform");
pub(crate) const REPLY_BANNED_A: Constant = Constant::new("session.reply_banned_a");
pub(crate) const REPLY_BANNED_B: Constant = Constant::new("session.reply_banned_b");
pub(crate) const REPLY_FRAME: Constant = Constant::new("session.reply_frame");
