//! What core calls inside this crate and a mod must not: the objects core's
//! `Load` hands a script's handlers.

/// What a script's frame and HUD handlers get, which core's `Load` hands
/// them.
pub use crate::ui::script::{ScriptFrame, ScriptHud};

/// What a script's item tooltip handler gets, which core's `Load` hands it.
#[cfg(feature = "unstable")]
pub use crate::ui::script::ScriptTooltipLines;
