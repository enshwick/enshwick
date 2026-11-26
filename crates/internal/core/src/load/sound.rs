//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::game_events::NotifyEvent;
use crate::sound::{SoundChange, SoundContainerId};
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Changes the values of sound container `sound` where `change` gives
    /// them, as the client reads it from the game's archive, after the
    /// changes of mods loaded before; nothing is written to the archive or
    /// a save. On a server, which plays no sound, it checks `change` and
    /// does nothing more.
    ///
    /// # Errors
    ///
    /// [`Off`] for a value outside its bound, a least distance past the
    /// greatest, after init, and on a client whose game lacks the type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::sound::{SoundChange, SoundContainerId};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let chest = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
    ///     let mut sound_change = SoundChange::default();
    ///     sound_change.volume = Some(0.2);
    ///     load.change_sound(chest, sound_change)
    /// }
    /// ```
    pub fn change_sound(&mut self, sound: SoundContainerId, change: SoundChange)
    -> Result<(), Off>;
}

#[enshwick_macros::forward(base)]
#[crate::script(sound_events)]
impl Load<'_> {
    /// Calls `handler` on the server's game thread for each notifier and
    /// sound the server places in the world, the game's own and those a mod
    /// raises with `Effects::notify`: every event but the visual effects,
    /// type [`Vfx`](crate::game_events::NotifyEventType::Vfx), so a type
    /// this build names but the crate does not reaches it too. On a client
    /// it hears only a game that client hosts.
    ///
    /// # Errors
    ///
    /// [`Off`] after `init` or on a game build without notify events.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_notify(|notify| {
    ///         enshwick::debug!("{:?} {:?} at {:?}", notify.r#type, notify.type_guid, notify.position);
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_notify<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&NotifyEvent) + Send + Sync + 'static;
}

#[cfg(feature = "unstable")]
mod add;
mod script;
