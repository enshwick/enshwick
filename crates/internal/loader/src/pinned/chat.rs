//! Build table ids under `chat.`.

use super::{Constant, Symbol};

pub(crate) const CLIENT_QUEUE: Symbol = Symbol::new("chat.client_queue");
pub(crate) const CLIENT_SEND: Symbol = Symbol::new("chat.client_send");
pub(crate) const CLIENT_STORE: Constant = Constant::new("chat.client_store");
pub(crate) const MARK_DELIVERED: Symbol = Symbol::new("chat.mark_delivered");
pub(crate) const MAX_TEXT: Constant = Constant::new("chat.max_text");
pub(crate) const NODE_DELIVERED: Constant = Constant::new("chat.node_delivered");
pub(crate) const POST: Symbol = Symbol::new("chat.post");
pub(crate) const POST_RETURN: Symbol = Symbol::new("chat.post_return");
pub(crate) const RECEIVE_RETURN_A: Symbol = Symbol::new("chat.receive_return_a");
pub(crate) const RECEIVE_RETURN_B: Symbol = Symbol::new("chat.receive_return_b");
pub(crate) const REPLY_TYPE: Constant = Constant::new("chat.reply_type");
pub(crate) const STORE_SLOTS: Constant = Constant::new("chat.store_slots");
pub(crate) const STORE_SLOT_COUNT: Constant = Constant::new("chat.store_slot_count");
pub(crate) const STORE_SLOT_SIZE: Constant = Constant::new("chat.store_slot_size");
pub(crate) const STORE_TAIL: Constant = Constant::new("chat.store_tail");
