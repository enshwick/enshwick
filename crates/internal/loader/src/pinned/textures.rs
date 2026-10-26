//! Build table ids under `textures.`.

use super::{Constant, Symbol};

pub(crate) const KEYS: Symbol = Symbol::new("textures.keys");
pub(crate) const REGISTRY_FILL: Symbol = Symbol::new("textures.registry_fill");
pub(crate) const REGISTRY_LISTED: Constant = Constant::new("textures.registry_listed");
pub(crate) const REGISTRY_LISTED_AT: Symbol = Symbol::new("textures.registry_listed_at");
pub(crate) const REQUEST: Symbol = Symbol::new("textures.request");
pub(crate) const REQUEST_CONTEXT: Constant = Constant::new("textures.request_context");
pub(crate) const REQUEST_HASH: Constant = Constant::new("textures.request_hash");
pub(crate) const REQUEST_OFFSET: Constant = Constant::new("textures.request_offset");
pub(crate) const REQUEST_POST: Symbol = Symbol::new("textures.request_post");
pub(crate) const REQUEST_RING: Symbol = Symbol::new("textures.request_ring");
pub(crate) const REQUEST_SIGNAL: Symbol = Symbol::new("textures.request_signal");
pub(crate) const REQUEST_SIZE: Constant = Constant::new("textures.request_size");
pub(crate) const RING_EVENT: Constant = Constant::new("textures.ring_event");
pub(crate) const RING_HEAD: Constant = Constant::new("textures.ring_head");
pub(crate) const RING_LAST: Constant = Constant::new("textures.ring_last");
pub(crate) const RING_RECORDS: Constant = Constant::new("textures.ring_records");
pub(crate) const RING_TAIL: Constant = Constant::new("textures.ring_tail");
pub(crate) const RING_TAIL_NEXT: Constant = Constant::new("textures.ring_tail_next");
pub(crate) const SIGNAL: Symbol = Symbol::new("textures.signal");
