//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(altar_limits)]
impl Load<'_> {
    /// Lets the game group every listed altar zone into its bases, up to 32
    /// altars a Flame level in 16 bases, refusing an altar that would start a
    /// 17th base; `Ok(false)` when the platform does not recognise this
    /// build's code, and the game keeps its own. The loader's hooks go in
    /// together: when one cannot be armed, none does and the game keeps 16,
    /// which [`base_limit`](crate::altars::base_limit) tells from [`Mod::ready`](crate::Mod::ready) on.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     if !load.raise_base_limit()? {
    ///         enshwick::info!("the game keeps its own altar zones on this build");
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn raise_base_limit(&mut self) -> Result<bool, Off>;

    /// As [`Load::raise_base_limit`], up to `zones` altars a Flame level in
    /// up to `bases` bases; values below the game's own count as the game's,
    /// and the largest request of every mod holds. Bases past 16 need more
    /// of the game's code; where the loader cannot arm all of it, the game
    /// keeps 16 bases with the zones raised, which [`base_limit`](crate::altars::base_limit) tells. A
    /// world lowers the bases below the request with
    /// [`set_base_cap`](crate::altars::set_base_cap).
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, or for a value past the most the loader takes
    /// ([`BaseLimit::max_zones`](crate::altars::BaseLimit::max_zones),
    /// [`BaseLimit::max_bases`](crate::altars::BaseLimit::max_bases)).
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     if !load.request_base_limits(128, 32)? {
    ///         enshwick::info!("the game keeps its own altar zones on this build");
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[script]
    pub fn request_base_limits(&mut self, zones: u32, bases: u32) -> Result<bool, Off>;
}
