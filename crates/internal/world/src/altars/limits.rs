//! The requests that raise the altar zones and bases the game groups.

use crate::Off;

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::raise_base_limit`.
    #[doc(hidden)]
    fn raise_base_limit(&mut self) -> Result<bool, Off>;

    /// As `Load::request_base_limits`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn request_base_limits(&mut self, zones: u32, bases: u32) -> Result<bool, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn raise_base_limit(&mut self) -> Result<bool, Off> {
        let host = self.host();
        armed((host.api().raise_base_limit)(host.handle()))
    }

    #[cfg(feature = "unstable")]
    fn request_base_limits(&mut self, zones: u32, bases: u32) -> Result<bool, Off> {
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
        Err(crate::load::off_status("the base limit", status))
    }
}
