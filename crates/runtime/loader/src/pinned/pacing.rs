//! Build table ids under `pacing.`.

use super::Symbol;

pub(crate) const BACKGROUND_WORKERS: Symbol = Symbol::new("pacing.background_workers");
pub(crate) const DEFAULT_WORKERS: Symbol = Symbol::new("pacing.default_workers");
pub(crate) const LOOP_WAIT: Symbol = Symbol::new("pacing.loop_wait");
pub(crate) const MESSAGE_WAIT: Symbol = Symbol::new("pacing.message_wait");
pub(crate) const SERVER_WAIT: Symbol = Symbol::new("pacing.server_wait");
pub(crate) const WAIT_DIVIDE: Symbol = Symbol::new("pacing.wait_divide");
