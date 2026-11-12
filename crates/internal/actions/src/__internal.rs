//! What core calls inside this crate and a mod must not: the objects core's
//! `Load` hands a script's handlers, and the crate-private parts the
//! domains above build on.

/// A control as a script declares it, and what its handler gets, for
/// `Load::control_script`.
#[cfg(feature = "unstable")]
pub use crate::controls::script::{ScriptControl, ScriptFired};
