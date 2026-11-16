//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::buffs::Buffs;
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Buffs`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::buffs).
    #[must_use]
    pub fn buffs(&mut self) -> Buffs;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(buffs_add)]
impl Load<'_> {
    /// Adds the buff type `new` to the game, on the server and the client
    /// alike, and returns its id: a copy of a vanilla type in the game's
    /// buff collection with the mod's name, description, icon and timing.
    /// Give it modifiers with [`Load::add_buff_modifiers`] and apply it with
    /// [`Buffs::apply`]. Call it during `init`. Unstable until its in-game
    /// check passes.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) for a mod of side server, a life time past `i64`
    /// nanoseconds, a slot past a byte, a build without the buff layout or
    /// without the buff collection, a `like` the game lacks, an id the game
    /// has or the mod added already, or a content id the loader refuses (see
    /// [`Load::declare`]). The mod decides whether to go on without the type.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::time::Duration;
    /// use enshwick::buffs::{BuffTypeId, NewBuff};
    /// use enshwick::content::id;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let lucky = load.add_buff(
    ///         NewBuff::new(id!("lucky"), BuffTypeId(0xf9b3_64f6))
    ///             .default_life_time(Duration::from_mins(5)),
    ///     )?;
    ///     enshwick::info!("lucky is buff {lucky:?}");
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_buff(
        &mut self,
        new: crate::buffs::NewBuff,
    ) -> Result<crate::buffs::BuffTypeId, crate::Off>;
}
