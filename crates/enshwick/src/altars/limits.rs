//! The requests that raise the altar zones and bases the game groups.

use crate::{Load, Off};

impl Load<'_> {
    /// Lets the game group every listed altar zone into its bases, up to 32
    /// altars a Flame level in 16 bases, refusing an altar that would start a
    /// 17th base; `Ok(false)` when the platform does not recognise this
    /// build's code, and the game keeps its own. The loader's hooks go in
    /// together: when one cannot be armed, none does and the game keeps 16,
    /// which [`base_limit`](super::base_limit) tells from [`Mod::ready`](crate::Mod::ready) on.
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
    pub fn raise_base_limit(&mut self) -> Result<bool, Off> {
        let host = self.host();
        armed((host.api().raise_base_limit)(host.handle()))
    }

    /// As [`Load::raise_base_limit`], up to `zones` altars a Flame level in
    /// up to `bases` bases; values below the game's own count as the game's,
    /// and the largest request of every mod holds. Bases past 16 need more
    /// of the game's code; where the loader cannot arm all of it, the game
    /// keeps 16 bases with the zones raised, which [`base_limit`](super::base_limit) tells. A
    /// world lowers the bases below the request with
    /// [`set_base_cap`](super::set_base_cap).
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, or for a value past the most the loader takes
    /// ([`BaseLimit::max_zones`](super::BaseLimit::max_zones),
    /// [`BaseLimit::max_bases`](super::BaseLimit::max_bases)).
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
    pub fn request_base_limits(&mut self, zones: u32, bases: u32) -> Result<bool, Off> {
        let host = self.host();
        armed((host.api().request_base_limits)(
            host.handle(),
            zones,
            bases,
        ))
        .map_err(|off| match super::base_limit() {
            Some(most) if zones > most.max_zones || bases > most.max_bases => Off::new(format!(
                "{zones} altar zones in {bases} bases asked for; the most is {} in {}",
                most.max_zones, most.max_bases
            )),
            _ => off,
        })
    }
}

/// Whether a base limit `status` is in force: `false` on a build whose code
/// the platform does not know.
fn armed(status: crate::abi::Status) -> Result<bool, Off> {
    if status.is_ok() {
        Ok(true)
    } else if status == crate::abi::Status::MISSING {
        Ok(false)
    } else {
        Err(Off::status("the base limit", status))
    }
}
