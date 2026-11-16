//! The notify event: a notifier, a sound or an effect the server raises at a
//! place, which every client in range plays.

use crate::content::Guid;
use crate::engine_enum::Values;
use crate::engine_ids as ids;
use crate::game_events::values::{NotifyEventType, SoundVolumeHint};
use crate::game_events::{EventType, Field, RawGameEvent, Scalar};
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
        load.on_raw_game_event(layout.ty, move |event| {
            if let Some(notify) = layout.decode(event) {
                handler(&notify);
            }
        })
    }
}

/// [`NotifyEvent`]'s layout on the running build: it reads the events the
/// game raises and writes the ones a mod raises. The crates above reach it
/// through `__internal`; a mod raises a notifier with `Effects::notify`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct NotifyLayout {
    ty: EventType,
    /// The `w` of the turn inside the event's `ecs::Offset`.
    turn_w: Field<f32>,
    /// The first of the transform's three fixed-point coordinates.
    position: u64,
    /// The `w` of the transform's orientation.
    orientation_w: Field<f32>,
    /// The first of the transform's three scale factors.
    scale: u64,
    source: Field<Entity>,
    guid: Field<Guid>,
    kind: Field<u8>,
    hint: Field<u8>,
    types: Values<NotifyEventType, 3>,
    hints: Values<SoundVolumeHint, 7>,
}

#[doc(hidden)]
impl NotifyLayout {
    /// The layout on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] on a build without the event or one of its fields.
    pub fn resolve(load: &BaseLoad) -> Result<Self, Off> {
        let ty = load.game_event_type(ids::NOTIFY_EVENT)?;
        let at = |base: &str, field: &str| -> Result<u64, Off> {
            Ok(load.constant(base)? + load.constant(field)?)
        };
        let field = |offset: u64| {
            ty.field_at(offset)
                .ok_or_else(|| Off::new("notify events: a field past the event's end"))
        };
        // The `w` of a quaternion follows its `x`, `y` and `z`.
        let w = 12;
        Ok(Self {
            ty,
            turn_w: field(at(ids::NOTIFY_EVENT_OFFSET, ids::OFFSET_ORIENTATION_OFFSET)? + w)?,
            position: at(ids::NOTIFY_EVENT_TRANSFORM, ids::WORLD_TRANSFORM_POSITION)?,
            orientation_w: field(
                at(
                    ids::NOTIFY_EVENT_TRANSFORM,
                    ids::WORLD_TRANSFORM_ORIENTATION,
                )? + w,
            )?,
            scale: at(ids::NOTIFY_EVENT_TRANSFORM, ids::WORLD_TRANSFORM_SCALE)?,
            source: ty.field_by_id(load, ids::NOTIFY_EVENT_SOURCE_ENTITY_ID)?,
            guid: ty.field_by_id(load, ids::NOTIFY_EVENT_TYPE_GUID)?,
            kind: ty.field_by_id(load, ids::NOTIFY_EVENT_TYPE)?,
            hint: ty.field_by_id(load, ids::NOTIFY_EVENT_SOUND_VOLUME_HINT)?,
            types: Values::resolve(load, NotifyEventType::IDS),
            hints: Values::resolve(load, SoundVolumeHint::IDS),
        })
    }

    /// The event type.
    #[must_use]
    pub const fn ty(&self) -> EventType {
        self.ty
    }

    /// The typed event of `event`; `None` for another type or bytes too few.
    #[must_use]
    pub fn decode(&self, event: &RawGameEvent<'_>) -> Option<NotifyEvent> {
        let source = event.get(self.source)?;
        let fixed = [0, 8, 16].map(|axis| {
            usize::try_from(self.position + axis)
                .ok()
                .and_then(|at| event.read::<i64>(at))
        });
        let [Some(x), Some(y), Some(z)] = fixed else {
            return None;
        };
        let kind = event.get(self.kind)?;
        let hint = event.get(self.hint)?;
        Some(NotifyEvent {
            source_entity_id: (!source.is_none()).then_some(source),
            position: crate::ecs::metres([x, y, z]),
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

    /// The bytes of a notify of `kind` naming `guid`, on `source` or at
    /// `position` in metres, as the game's own systems lay one out: the turn
    /// and orientation unturned, the scale 1; `None` when the build lacks
    /// the value of its type or hint.
    #[must_use]
    pub fn encode(
        &self,
        kind: NotifyEventType,
        guid: Guid,
        source: Option<Entity>,
        position: [f64; 3],
        hint: SoundVolumeHint,
    ) -> Option<Vec<u8>> {
        let kind = match kind {
            NotifyEventType::Other(raw) => raw,
            known => u8::try_from(self.types.value(known)?).ok()?,
        };
        let hint = match hint {
            SoundVolumeHint::Other(raw) => raw,
            known => u8::try_from(self.hints.value(known)?).ok()?,
        };
        let mut bytes = vec![0u8; usize::try_from(self.ty.size()).ok()?];
        let mut written = put(&mut bytes, self.turn_w, 1.0)
            && put(&mut bytes, self.orientation_w, 1.0)
            && put(&mut bytes, self.guid, guid)
            && put(&mut bytes, self.kind, kind)
            && put(&mut bytes, self.hint, hint)
            && put(&mut bytes, self.source, source.unwrap_or(Entity::NONE));
        let fixed = crate::ecs::fixed(position);
        for (step, axis) in (0u64..).zip(fixed) {
            written &= put_at(&mut bytes, self.position + 8 * step, axis);
        }
        for step in 0..3 {
            written &= put_at(&mut bytes, self.scale + 4 * step, 1.0f32);
        }
        written.then_some(bytes)
    }
}

/// Writes `value` at `field`; false past the bytes.
fn put<T: Scalar>(bytes: &mut [u8], field: Field<T>, value: T) -> bool {
    put_at(bytes, u64::from(field.offset()), value)
}

/// Writes `value` at `at`; false past the bytes.
fn put_at<T: Scalar>(bytes: &mut [u8], at: u64, value: T) -> bool {
    usize::try_from(at)
        .ok()
        .and_then(|at| bytes.get_mut(at..))
        .is_some_and(|out| value.to_bytes(out))
}
