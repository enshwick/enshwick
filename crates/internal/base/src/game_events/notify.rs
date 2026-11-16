//! The notify event: a notifier, a sound or an effect the server raises at a
//! place, which every client in range plays.

use crate::content::Guid;
use crate::engine_enum::Values;
use crate::engine_ids as ids;
use crate::game_events::values::{NotifyEventType, SoundVolumeHint};
use crate::game_events::{EventType, Field, RawGameEvent};
use crate::{BaseLoad, Entity, Off};

/// A notifier, a sound or an effect the server raised at a place or on an
/// entity, which every client in range plays: a hit's sound, a pickup's
/// chime, a survivor arriving.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::{NotifyEvent, NotifyEventType};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on::<NotifyEvent>(|notify| {
///         if notify.r#type == NotifyEventType::Normal {
///             enshwick::debug!("notifier {:?} at {:?}", notify.type_guid, notify.position);
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct NotifyEvent {
    /// The entity it plays on; `None` for one at a place alone.
    pub source_entity_id: Option<Entity>,
    /// Where it plays, in metres. The game leaves it at the world's origin
    /// for one it places on its source entity alone.
    pub position: [f64; 3],
    /// What [`type_guid`](Self::type_guid) names.
    pub r#type: NotifyEventType,
    /// The notifier for [`NotifyEventType::Normal`], the sound container
    /// for [`NotifyEventType::Sfx`], the effect for [`NotifyEventType::Vfx`].
    pub type_guid: Guid,
    /// Whose sound it is, which the client's volume settings group it by.
    pub sound_volume_hint: SoundVolumeHint,
}

impl crate::events::Kind for NotifyEvent {
    type Data = Self;

    fn subscribe<F>(load: &mut BaseLoad, handler: F) -> Result<(), Off>
    where
        F: Fn(&Self) + Send + Sync + 'static,
    {
        let layout = NotifyLayout::resolve(load)?;
        super::kinds::on(load, layout.ty, move |event| layout.decode(event), handler)
    }
}

/// [`NotifyEvent`]'s fields on the running build, for reading the events
/// the game raises and writing the ones a mod raises. The crates above
/// reach it through `__internal`; a mod raises a notifier with
/// `Effects::notify`, which resolves the transform it writes itself.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct NotifyLayout {
    /// The event type.
    pub ty: EventType,
    /// The transform's three fixed-point coordinates.
    position: [Field<i64>; 3],
    /// The entity it plays on.
    pub source: Field<Entity>,
    /// What it names: a notifier, a sound container or an effect.
    pub guid: Field<Guid>,
    /// Its [`NotifyEventType`], as the engine's value.
    pub kind: Field<u8>,
    /// Its [`SoundVolumeHint`], as the engine's value.
    pub hint: Field<u8>,
    types: Values<NotifyEventType, 3>,
    hints: Values<SoundVolumeHint, 7>,
}

#[doc(hidden)]
impl NotifyLayout {
    /// The fields on the running build that reading an event needs.
    ///
    /// # Errors
    ///
    /// [`Off`] on a build without the event or one of those fields.
    pub fn resolve(load: &BaseLoad) -> Result<Self, Off> {
        let ty = load.game_event_type(ids::NOTIFY_EVENT)?;
        let position = load.constant(ids::NOTIFY_EVENT_TRANSFORM)?
            + load.constant(ids::WORLD_TRANSFORM_POSITION)?;
        let axis = |at: u64| {
            ty.field_at(position + at)
                .ok_or_else(|| Off::new("notify events: the position past the event's end"))
        };
        Ok(Self {
            ty,
            position: [axis(0)?, axis(8)?, axis(16)?],
            source: ty.field_by_id(load, ids::NOTIFY_EVENT_SOURCE_ENTITY_ID)?,
            guid: ty.field_by_id(load, ids::NOTIFY_EVENT_TYPE_GUID)?,
            kind: ty.field_by_id(load, ids::NOTIFY_EVENT_TYPE)?,
            hint: ty.field_by_id(load, ids::NOTIFY_EVENT_SOUND_VOLUME_HINT)?,
            types: Values::resolve(load, NotifyEventType::IDS),
            hints: Values::resolve(load, SoundVolumeHint::IDS),
        })
    }

    /// The typed event of `event`; `None` for another type or bytes too few.
    #[must_use]
    pub fn decode(&self, event: &RawGameEvent<'_>) -> Option<NotifyEvent> {
        let [x, y, z] = self.position.map(|axis| event.get(axis));
        let kind = event.get(self.kind)?;
        let hint = event.get(self.hint)?;
        Some(NotifyEvent {
            source_entity_id: super::kinds::some(event.get(self.source)?),
            position: crate::ecs::metres([x?, y?, z?]),
            r#type: self
                .types
                .variant(u64::from(kind))
                .unwrap_or(NotifyEventType::Other(kind)),
            type_guid: event.get(self.guid)?,
            sound_volume_hint: self
                .hints
                .variant(u64::from(hint))
                .unwrap_or(SoundVolumeHint::Other(hint)),
        })
    }

    /// The engine's value of `kind`; `None` when the build lacks it.
    #[must_use]
    pub fn kind_value(&self, kind: NotifyEventType) -> Option<u8> {
        match kind {
            NotifyEventType::Other(raw) => Some(raw),
            known => u8::try_from(self.types.value(known)?).ok(),
        }
    }

    /// The engine's value of `hint`; `None` when the build lacks it.
    #[must_use]
    pub fn hint_value(&self, hint: SoundVolumeHint) -> Option<u8> {
        match hint {
            SoundVolumeHint::Other(raw) => Some(raw),
            known => u8::try_from(self.hints.value(known)?).ok(),
        }
    }
}
