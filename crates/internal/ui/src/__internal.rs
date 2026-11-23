//! What core calls inside this crate and a mod must not: the objects core's
//! `Load` hands a script's handlers.

/// What a script's frame and HUD handlers get, which core's `Load` hands
/// them.
pub use crate::ui::script::{ScriptFrame, ScriptHud};

/// Where a script's screen panel sits, which core's `Load` reads.
#[cfg(feature = "unstable")]
pub use crate::ui::script::ScriptPlacement;

/// What a script's item tooltip handler gets, which core's `Load` hands it.
#[cfg(feature = "unstable")]
pub use crate::ui::script::ScriptTooltipLines;

/// Why `change` would be refused, so a pack names it at its line before
/// init: the bounds live once, in the sound module.
///
/// # Errors
///
/// The value out of its bound, as the sound module words it.
pub fn check_sound_change(change: &crate::sound::SoundChange) -> Result<(), ChangeSoundError> {
    change.fields().map(drop)
}

/// Why a sound change is refused, which a pack names at its line.
pub use crate::sound::change::ChangeSoundError;

/// The `FramelessQOA` encoding of `samples`, `channels` interleaved, for the
/// encoder's benchmark.
#[cfg(feature = "unstable")]
#[must_use]
pub fn encode_qoa(samples: &[i16], channels: usize) -> Vec<u8> {
    crate::sound::qoa::encode(samples, channels)
}
