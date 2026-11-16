//! Notifiers a mod raises: a sound, an effect and a camera shake every
//! client in range plays.

use super::Effects;
use crate::game_events::{NotifyEventType, SoundVolumeHint};
use crate::{Entity, WorldError};

/// The most notifiers one mod raises in one server step; the loader
/// refuses more, since every client in range plays each one.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick::effects::MAX_NOTIFIES_PER_STEP, 32);
/// ```
pub const MAX_NOTIFIES_PER_STEP: u32 = 32;

/// Where a notifier plays, for [`Notify`].
///
/// # Examples
///
/// ```
/// use enshwick::effects::NotifyAt;
///
/// let here = NotifyAt::Position([10.0, 50.0, -3.0]);
/// assert_ne!(here, NotifyAt::Entity(enshwick::Entity(7)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub enum NotifyAt {
    /// On an entity: the clients place it where the entity is.
    Entity(Entity),
    /// At a point in the world, in metres.
    Position([f64; 3]),
}

/// A notifier the server raises, for [`Effects::notify`]: the clients in
/// range play its sound, effect and camera shake, as for a notifier the
/// game raises.
///
/// A notifier is a game resource, named by its GUID (a
/// `keen::NotifierResource`). A notifier that misleads players in a fight,
/// such as an enemy's attack where none is, does not belong in a mod.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::effects::{Notify, NotifyAt};
/// use enshwick::game_events::SoundVolumeHint;
///
/// let chime = Notify::new(Guid([7; 16]), NotifyAt::Position([0.0, 100.0, 0.0]))
///     .sound_volume_hint(SoundVolumeHint::FriendlyNpc);
/// assert_eq!(chime.sound_volume_hint, SoundVolumeHint::FriendlyNpc);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Notify {
    /// The notifier's GUID.
    pub notifier: crate::content::Guid,
    /// Where it plays.
    pub at: NotifyAt,
    /// Whose sound it is, which the clients' volume settings group it by.
    #[script(default)]
    pub sound_volume_hint: SoundVolumeHint,
}

impl Notify {
    /// The notifier `notifier` at `at`, as any sound effect.
    ///
    /// # Examples
    ///
    /// See [`Notify`].
    #[must_use]
    pub const fn new(notifier: crate::content::Guid, at: NotifyAt) -> Self {
        Self {
            notifier,
            at,
            sound_volume_hint: SoundVolumeHint::GenericSfx,
        }
    }

    /// Group its sound as `hint`'s in the clients' volume settings.
    ///
    /// # Examples
    ///
    /// See [`Notify`].
    #[must_use]
    pub const fn sound_volume_hint(mut self, hint: SoundVolumeHint) -> Self {
        self.sound_volume_hint = hint;
        self
    }
}

#[crate::script(notifiers)]
impl Effects {
    /// Raises the notifier `notify.notifier` at a place or on an entity, as
    /// the game's own systems do for a hit or a pickup: every client in
    /// range, a client without mods too, plays its sound and effect. The
    /// clients play it after the next step; a mod hears it in
    /// [`Load::on_notify`](../struct.Load.html#method.on_notify) then.
    ///
    /// At most [`MAX_NOTIFIES_PER_STEP`] per mod in one server step.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] past the step's bound or when the step holds
    /// no room for more events, [`WorldError::WrongThread`] off the game
    /// thread and [`WorldError::Unavailable`] on a build without notifiers.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::effects::{Effects, Notify, NotifyAt};
    ///
    /// fn chime(effects: &Effects, notifier: Guid, at: [f64; 3]) {
    ///     if let Err(why) = effects.notify(&Notify::new(notifier, NotifyAt::Position(at))) {
    ///         enshwick::debug!("no chime: {why}");
    ///     }
    /// }
    /// ```
    #[script]
    pub fn notify(&self, notify: &Notify) -> Result<(), WorldError> {
        let layout = self.notify.ok_or(WorldError::Unavailable)?;
        let (source, position) = match notify.at {
            NotifyAt::Entity(entity) => (Some(entity), [0.0; 3]),
            NotifyAt::Position(at) => (None, at),
        };
        let bytes = layout
            .encode(
                NotifyEventType::Normal,
                notify.notifier,
                source,
                position,
                notify.sound_volume_hint,
            )
            .ok_or(WorldError::Unavailable)?;
        self.emit_raw(layout.ty(), &bytes)
    }
}
