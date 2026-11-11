//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::HostApi;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_icons)]
impl Load<'_> {
    /// Serves `icon`'s UI image, built from the vanilla one it starts from
    /// when the client first shows it. [`Load::add_item`] does it for an
    /// item's icon.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the icon's id
    /// (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, Id, id};
    /// use enshwick::content::icons::{Icon, Pixels, Tint};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_icon(Icon {
    ///         id: id!("ghostly-staff.icon"),
    ///         like: Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78"),
    ///         pixels: Pixels::Tint(Tint { colour: [120, 170, 255], amount: 60 }),
    ///     })
    /// }
    /// ```
    #[script]
    pub fn add_icon(&mut self, icon: Icon) -> Result<(), Off>;
}
