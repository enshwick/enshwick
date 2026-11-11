//! The typed API as a script reaches it: a handle per role, opened by the
//! generated type's id at start, whose values cross as tables by the
//! generated items' Rust names, read and written through the type's
//! [`Shape`](super::Shape), which takes and refuses what the generated Rust
//! type does.

use super::{RawComponent, RawEvent, RawResource, Role, Row};
use crate::__load::*;
use crate::content::Guid;
use crate::resources::{Registry, Resources, Value as Data};
use crate::script::{Class, Error, Value};
use crate::world::World;
use crate::{Entity, Host, Off};
use enshwick_typed::typed::runtime::rows::Fields;

/// A generated component type on the running build, as a script holds it.
#[derive(Clone)]
pub struct ComponentType {
    row: &'static Row,
    raw: RawComponent,
    /// The world, or why the build lacks it: only reads and writes need it.
    world: Result<World, Off>,
}

impl ComponentType {
    fn world(&self) -> Result<World, Off> {
        self.world
            .clone()
            .map_err(|why| Off::new(format!("typed {}: {why}", self.row.id)))
    }
}

#[crate::script(reads)]
impl ComponentType {
    /// `entity`'s component as a table by the generated struct's field
    /// names, on the game thread; nil when the entity has none, or off the
    /// game thread.
    ///
    /// # Errors
    ///
    /// [`Off`] when the component's bytes, or the blob data that follows
    /// them, do not read as the generated struct, naming the field, or on a
    /// build that lacks the world's reads.
    #[script]
    pub(crate) fn read(&self, entity: Entity) -> Result<Option<Fields>, Off> {
        self.raw
            .read(self.world()?, entity)?
            .map(|value| self.row.shown(&value))
            .transpose()
    }

    /// Writes `value`, a table with every field of the generated struct,
    /// to `entity`'s component, marked changed the game's way, keeping the
    /// bytes of what the struct does not hold. On the game thread only.
    ///
    /// # Errors
    ///
    /// [`Off`] for a type the loader does not let mods write or with blob
    /// fields, a value that does not fit the type, off the game thread, or
    /// on a build that lacks
    /// the world's reads.
    #[script]
    pub(crate) fn write(&self, entity: Entity, value: Fields) -> Result<(), Off> {
        self.raw
            .write(self.world()?, entity, &self.row.taken(value)?)
    }
}

impl crate::script::Object for ComponentType {
    const CLASS: &'static Class = &Class::of::<Self>(
        "ComponentType",
        "A generated component type on the running build: entities' components read and written as tables.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_READS];
}

/// A generated game event type on the running build, as a script holds it.
#[derive(Clone)]
pub struct EventType {
    row: &'static Row,
    raw: RawEvent,
    host: Host,
}

#[crate::script(handlers)]
impl EventType {
    /// Calls `handler` with each event of the type the game raises, as a
    /// table by the generated struct's field names, on the server's game
    /// thread; an event that does not read is logged once and skipped.
    /// Subscribe at the top level of main.luau.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the handler, such as after the start.
    #[script]
    pub(crate) fn on(&self, handler: impl Fn(Fields) + Send + Sync + 'static) -> Result<(), Off> {
        let row = self.row;
        self.raw.on(self.host, move |value| {
            handler(row.shown(value)?);
            Ok(())
        })
    }
}

impl crate::script::Object for EventType {
    const CLASS: &'static Class = &Class::of::<Self>(
        "EventType",
        "A generated game event type on the running build: its events handled as tables.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_HANDLERS];
}

/// A generated resource type on the running build, as a script holds it.
#[derive(Clone)]
pub struct ResourceType {
    row: &'static Row,
    raw: RawResource,
    host: Host,
}

#[crate::script(values)]
impl ResourceType {
    /// The key of the resource of this type with GUID `guid`, part 0, as
    /// `{ guid, type, part }`.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game.
    #[script]
    pub(crate) fn key(&self, guid: Guid) -> Result<Data, Off> {
        Ok(Resources::key_data(Registry::game()?, self.raw.key(guid)))
    }

    /// The game's own resource `key`, `{ guid, type, part }`, as a table by
    /// the generated struct's field names, read from its archive.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key or one of another type, a key the archive
    /// lacks, and a resource whose bytes do not read as the generated
    /// struct.
    #[script]
    pub(crate) fn game_value(&self, key: &Data) -> Result<Fields, Off> {
        self.row
            .shown(&self.raw.game_value(Resources::key_of(key)?)?)
    }

    /// Serves the resource `key`, made from the resource `base` the first
    /// time the game reads it: `change` gets the base's value as a table by
    /// the generated struct's field names and returns the value to serve,
    /// every field given, or nil to serve the base unchanged; the bytes of
    /// what the struct does not hold stay as the base has them. A change
    /// that fails or does not fit the type is logged, and the base is
    /// served unchanged. Allowed while the mod starts.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key, a key or base of another type, and when
    /// the loader refuses the serve, such as after the start.
    #[script]
    pub(crate) fn serve(
        &self,
        key: &Data,
        base: &Data,
        change: impl Fn(Fields) -> Result<Fields, Error> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let row = self.row;
        let (key, base) = (Resources::key_of(key)?, Resources::key_of(base)?);
        self.raw.serve(self.host, key, base, move |value| {
            match change(row.shown(&value)?)? {
                Fields(Value::Nil) => Ok(None),
                changed => row.taken(changed).map(Some),
            }
        })
    }
}

impl crate::script::Object for ResourceType {
    const CLASS: &'static Class = &Class::of::<Self>(
        "ResourceType",
        "A generated resource type on the running build: its resources read and served as tables.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_VALUES];
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::typed_component_type`.
    #[doc(hidden)]
    fn typed_component_type(&mut self, id: &str) -> Result<ComponentType, Off>;

    /// As `Load::typed_event_type`.
    #[doc(hidden)]
    fn typed_event_type(&mut self, id: &str) -> Result<EventType, Off>;

    /// As `Load::typed_resource_type`.
    #[doc(hidden)]
    fn typed_resource_type(&mut self, id: &str) -> Result<ResourceType, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn typed_component_type(&mut self, id: &str) -> Result<ComponentType, Off> {
        let row = Row::find(id, Role::Component)?;
        Ok(ComponentType {
            row,
            raw: RawComponent::resolve(self, row.id, &|value| row.check(value))?,
            world: self.world(),
        })
    }

    fn typed_event_type(&mut self, id: &str) -> Result<EventType, Off> {
        let row = Row::find(id, Role::Event)?;
        Ok(EventType {
            row,
            raw: RawEvent::resolve(self, row.id, &|value| row.check(value))?,
            host: self.host(),
        })
    }

    fn typed_resource_type(&mut self, id: &str) -> Result<ResourceType, Off> {
        let row = Row::find(id, Role::Resource)?;
        Ok(ResourceType {
            row,
            raw: RawResource::resolve(self, row.id, &|value| row.check(value))?,
            host: self.host(),
        })
    }
}
