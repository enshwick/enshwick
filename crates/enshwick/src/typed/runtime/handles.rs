//! The handles that resolve a generated component, event or resource on the
//! running build and check its shape: a core per role that knows only the
//! type's id, and the typed handle a Rust mod holds over it.

use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};

use super::{Component, Event, Resource, ShapeError, check, sample};
use crate::content::Guid;
use crate::resources::{Registry, Type, Value};
use crate::world::{World, WritableType};
use crate::{Entity, Host, Load, Off};

/// The running build's type id and reflected type for the id `id`, its
/// shape checked: a [`sample`] of the build's type must pass `check`, so a
/// field the build renamed, removed, or retyped to a value the generated
/// type cannot hold, such as a wider integer, at any depth, turns off only
/// that type. For a type read `live` from an entity, a field whose data a
/// read never reaches, such as a designer data kind's on the heap, turns it
/// off too, by name.
fn resolve(
    load: &Load<'_>,
    id: &'static str,
    check: &dyn Fn(&Value) -> Result<(), ShapeError>,
    live: bool,
) -> Result<(crate::ecs::ComponentType, &'static Type<'static>), Off> {
    let host = load.host();
    let ecs =
        crate::ecs::ComponentType::resolve(load, id).map_err(|e| off(id, host, &e.to_string()))?;
    let registry = Registry::game()?;
    let ty = registry
        .hashed(ecs.hash())
        .ok_or_else(|| off(id, host, "the type registry lacks it"))?;
    if live && let Some(field) = super::outside::outside_field(registry, ty) {
        return Err(off(
            id,
            host,
            &format!(
                "its field {field} keeps its data outside the component's set-up data, where a read of a live component does not reach"
            ),
        ));
    }
    let value = sample(registry, ty.qualified_name())
        .ok_or_else(|| off(id, host, "its value does not read"))?;
    check(&value).map_err(|e| off(id, host, &e.to_string()))?;
    Ok((ecs, ty))
}

/// Turns the type `id` off on the running build, naming the build and why.
fn off(id: &str, host: Host, why: &str) -> Off {
    Off::new(format!(
        "typed {id}: on game build {}, {why}",
        host.build().fingerprint.age
    ))
}

/// A component type resolved on the running build by its id: reads and
/// writes of an entity's component as the codec's [`Value`].
#[derive(Clone, Copy)]
pub(crate) struct RawComponent {
    id: &'static str,
    ty: crate::ecs::ComponentType,
    name: &'static str,
    size: usize,
    /// Whether a field's data follows the component, which a read reaches
    /// past the component's own bytes.
    blob: bool,
    writable: Option<WritableType>,
}

/// How far past a component a read of its blob data reaches at most. On
/// server 26430253 every blob of a live component lay within 0xc4 bytes of
/// it; the cap keeps a damaged offset or count from reading on.
const BLOB_REACH: usize = 1 << 20;

impl RawComponent {
    /// The component type `id` on the running build, its shape held to
    /// `check`.
    pub(crate) fn resolve(
        load: &Load<'_>,
        id: &'static str,
        check: &dyn Fn(&Value) -> Result<(), ShapeError>,
    ) -> Result<Self, Off> {
        let (ecs, ty) = resolve(load, id, check, true)?;
        let blob = super::outside::blob_field(Registry::game()?, ty).is_some();
        Ok(Self {
            id,
            ty: ecs,
            name: ty.qualified_name(),
            size: usize::try_from(ty.size()).unwrap_or(usize::MAX),
            blob,
            writable: WritableType::resolve(load, id).ok(),
        })
    }

    /// `entity`'s component on the game thread; `None` when the entity has
    /// none or off the game thread.
    ///
    /// A type with blob fields reads from the template's set-up data the
    /// component lies in, whose blobs follow it (server 26430253): the
    /// decode runs over the readable bytes from the component on, at most
    /// [`BLOB_REACH`], and refuses an offset or count past them.
    pub(crate) fn read(&self, world: World, entity: Entity) -> Result<Option<Value>, Off> {
        let Some(data) = world.component(entity, self.ty) else {
            return Ok(None);
        };
        let registry = Registry::game()?;
        if !self.blob {
            let mut bytes = vec![0; self.size];
            // SAFETY: `World::component` returned the entity's live component,
            // valid until the game thread moves on and read at once; `size` is
            // the reflected size of its type on the running build.
            unsafe { crate::raw::read_bytes_at(data.cast(), 0, &mut bytes) };
            return Ok(Some(registry.decode(self.name, &bytes)?));
        }
        let host = crate::host().ok_or_else(|| self.unread(registry))?;
        let mut len = 0;
        let status = (host.api().readable)(host.handle(), data.addr(), BLOB_REACH, &raw mut len);
        if status != crate::abi::Status::OK || len < self.size {
            return Err(self.unread(registry));
        }
        // SAFETY: the loader found `len` bytes from the component on
        // committed and readable, in one run of pages; they are the
        // template's read-only set-up data (server 26430253), which no thread
        // writes or frees while the game thread is here, and the borrow ends
        // with this call.
        let bytes = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), len) };
        registry
            .decode(self.name, bytes)
            .map(Some)
            .map_err(|why| Off::new(format!("typed {}: {why}", self.id)))
    }

    /// Why a component with blob fields does not read, naming the first.
    #[cold]
    fn unread(&self, registry: &Registry<'_>) -> Off {
        let field = registry
            .named(self.name)
            .and_then(|ty| super::outside::blob_field(registry, ty))
            .unwrap_or_default();
        Off::new(format!(
            "typed {}: the component does not read as far as its field {field}",
            self.id
        ))
    }

    /// Writes `value` to `entity`'s component, marked changed the game's
    /// way, keeping the bytes of what `value` leaves out.
    pub(crate) fn write(&self, world: World, entity: Entity, value: &Value) -> Result<(), Off> {
        let id = self.id;
        let writable = self
            .writable
            .ok_or_else(|| Off::new(format!("typed {id}: mods may not write it")))?;
        let name = self.name;
        world
            .update(entity, writable, |bytes| {
                let written = Registry::game()?.encode(name, value, Some(bytes))?;
                if written.len() != bytes.len() {
                    return Err(Off::new(format!(
                        "typed {id}: a value of {} bytes, not {}",
                        written.len(),
                        bytes.len()
                    )));
                }
                bytes.copy_from_slice(&written);
                Ok(())
            })
            .map_err(|why| Off::new(format!("typed {id}: {why}")))?
    }
}

/// A game event type resolved on the running build by its id.
#[derive(Clone, Copy)]
pub(crate) struct RawEvent {
    id: &'static str,
    ty: crate::game_events::EventType,
    name: &'static str,
}

impl RawEvent {
    /// The event type `id` on the running build, its shape held to `check`.
    pub(crate) fn resolve(
        load: &Load<'_>,
        id: &'static str,
        check: &dyn Fn(&Value) -> Result<(), ShapeError>,
    ) -> Result<Self, Off> {
        let (_, ty) = resolve(load, id, check, false)?;
        Ok(Self {
            id,
            ty: load.game_event_type(id)?,
            name: ty.qualified_name(),
        })
    }

    /// Calls `handler` with each event of the type the game raises, as the
    /// codec's [`Value`]; an event that does not read, or that `handler`
    /// refuses, is logged once and skipped.
    pub(crate) fn on<F>(self, host: Host, handler: F) -> Result<(), Off>
    where
        F: Fn(&Value) -> Result<(), Off> + Send + Sync + 'static,
    {
        let Self { id, ty, name } = self;
        // One warning per handler: a `static` here would be shared by every
        // event type.
        let warned = AtomicBool::new(false);
        crate::load::game_events_of(host, ty.hash(), ty.name(), move |raw| {
            let outcome = Registry::game()
                .and_then(|registry| Ok(registry.decode(name, raw.bytes())?))
                .and_then(|value| handler(&value));
            if let Err(why) = outcome
                && !warned.swap(true, Ordering::Relaxed)
            {
                crate::warn!("typed {id}: an event does not read: {why}");
            }
        })
    }
}

/// A resource type resolved on the running build by its id: the keys of
/// its resources, their game values and changes served in load order.
#[derive(Clone, Copy)]
pub(crate) struct RawResource {
    id: &'static str,
    hash: u32,
    name: &'static str,
}

impl RawResource {
    /// The resource type `id` on the running build, its shape held to
    /// `check`.
    pub(crate) fn resolve(
        load: &Load<'_>,
        id: &'static str,
        check: &dyn Fn(&Value) -> Result<(), ShapeError>,
    ) -> Result<Self, Off> {
        let (_, ty) = resolve(load, id, check, false)?;
        Ok(Self {
            id,
            hash: ty.qualified_hash(),
            name: ty.qualified_name(),
        })
    }

    /// The key of the resource of this type with GUID `guid`, part 0.
    pub(crate) fn key(&self, guid: Guid) -> crate::abi::ResourceKey {
        guid.key_by_hash(self.hash)
    }

    /// Refuses `key` when it is of another type.
    fn own(&self, key: crate::abi::ResourceKey) -> Result<(), Off> {
        if key.type_hash == self.hash {
            Ok(())
        } else {
            Err(Off::new(format!(
                "typed {}: a key of type {:#010x}",
                self.id, key.type_hash
            )))
        }
    }

    /// The game's own resource `key`, as the archive holds it.
    pub(crate) fn game_value(&self, key: crate::abi::ResourceKey) -> Result<Value, Off> {
        self.own(key)?;
        let host = crate::host().ok_or_else(|| Off::new("typed resources: outside the game"))?;
        host.game_value(key)
    }

    /// Serves the resource `key`, made from the resource `base` by
    /// `change`, the first time the game reads it: `change` gets the base's
    /// value and returns the value to write over the base's bytes, or
    /// `None` to keep them. A change that fails or does not fit the type
    /// is logged and the base is served unchanged.
    pub(crate) fn serve<F>(
        &self,
        host: Host,
        key: crate::abi::ResourceKey,
        base: crate::abi::ResourceKey,
        change: F,
    ) -> Result<(), Off>
    where
        F: Fn(Value) -> Result<Option<Value>, Off> + Send + Sync + 'static,
    {
        self.own(key)?;
        self.own(base)?;
        let Self { id, name, .. } = *self;
        crate::content::serve(host, key, base, move |bytes| {
            let changed = Registry::game().and_then(|registry| {
                Ok(match change(registry.decode(name, bytes)?)? {
                    Some(value) => registry.encode(name, &value, Some(bytes))?,
                    None => bytes.to_vec(),
                })
            });
            Some(changed.unwrap_or_else(|why| {
                host.log(
                    crate::abi::Level::WARN,
                    &format!("typed {id}: served unchanged: {why}"),
                );
                bytes.to_vec()
            }))
        })
    }
}

/// A generated component type resolved on the running build: reads and
/// writes of an entity's component as its generated struct.
///
/// A field whose data follows the component, such as a `BlobArray`, reads
/// from the template's set-up data the component lies in, through a check
/// that the memory reads; a blob past it fails that read, naming the field.
/// A type with a field whose data lies elsewhere, at any depth, such as a
/// designer data kind, does not resolve: its refusal names the field.
///
/// # Examples
///
/// ```
/// use enshwick::typed::{Component, ComponentType};
///
/// fn watch<T: Component + std::fmt::Debug>(
///     load: &mut enshwick::Load<'_>,
///     entity: enshwick::Entity,
/// ) -> Result<(), enshwick::Off> {
///     let ty = ComponentType::<T>::resolve(load)?;
///     let world = load.world()?;
///     enshwick::info!("{:?}", ty.read(world, entity)?);
///     Ok(())
/// }
/// ```
pub struct ComponentType<T> {
    raw: RawComponent,
    of: PhantomData<fn() -> T>,
}

impl<T> Clone for ComponentType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ComponentType<T> {}

impl<T> core::fmt::Debug for ComponentType<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ComponentType")
            .field("name", &self.raw.name)
            .field("writable", &self.raw.writable.is_some())
            .finish_non_exhaustive()
    }
}

impl<T: Component> ComponentType<T> {
    /// `T` on the running build, its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks the type, its shape differs from `T`,
    /// or a field's data lies where a read does not reach, such as a
    /// designer data kind's, naming the build.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            raw: RawComponent::resolve(load, T::ID, &check::<T>)?,
            of: PhantomData,
        })
    }

    /// `entity`'s component on the game thread; `None` when the entity has
    /// none or off the game thread.
    ///
    /// # Errors
    ///
    /// [`Off`] when the component's bytes, or the blob data that follows
    /// them, do not read as `T`, naming the field.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    pub fn read(&self, world: World, entity: Entity) -> Result<Option<T>, Off> {
        match self.raw.read(world, entity)? {
            Some(value) => Ok(Some(T::from_value(&value)?)),
            None => Ok(None),
        }
    }

    /// Writes `value` to `entity`'s component, marked changed the game's
    /// way, keeping the bytes of what `T` does not hold. On the game thread
    /// only.
    ///
    /// # Errors
    ///
    /// [`Off`] for a type the loader does not let mods write, a value that
    /// does not fit the type, or as [`World::update`] fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::typed::{Component, ComponentType};
    ///
    /// fn copy<T: Component>(
    ///     ty: ComponentType<T>,
    ///     world: enshwick::world::World,
    ///     from: enshwick::Entity,
    ///     to: enshwick::Entity,
    /// ) -> Result<(), enshwick::Off> {
    ///     match ty.read(world, from)? {
    ///         Some(value) => ty.write(world, to, &value),
    ///         None => Ok(()),
    ///     }
    /// }
    /// ```
    pub fn write(&self, world: World, entity: Entity, value: &T) -> Result<(), Off> {
        self.raw.write(world, entity, &value.to_value())
    }
}

/// A generated game event type resolved on the running build, to handle
/// its events as the generated struct.
///
/// # Examples
///
/// ```
/// use enshwick::typed::{Event, EventType};
///
/// fn log<T: Event + std::fmt::Debug>(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     EventType::<T>::resolve(load)?.on(load, |event| enshwick::info!("{event:?}"))
/// }
/// ```
pub struct EventType<T> {
    raw: RawEvent,
    of: PhantomData<fn() -> T>,
}

impl<T> Clone for EventType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for EventType<T> {}

impl<T> core::fmt::Debug for EventType<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EventType")
            .field("name", &self.raw.name)
            .finish_non_exhaustive()
    }
}

impl<T: Event> EventType<T> {
    /// `T` on the running build, its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks the type or its shape differs from
    /// `T`, naming the build.
    ///
    /// # Examples
    ///
    /// See [`EventType`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            raw: RawEvent::resolve(load, T::ID, &check::<T>)?,
            of: PhantomData,
        })
    }

    /// Calls `handler` with each event of the type the game raises, read
    /// as `T`; an event whose bytes do not read as `T` is logged once and
    /// skipped.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the handler, such as after init.
    ///
    /// # Examples
    ///
    /// See [`EventType`].
    pub fn on<F>(self, load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.raw.on(load.host(), move |value| {
            handler(&T::from_value(value)?);
            Ok(())
        })
    }
}

/// A generated resource type resolved on the running build: the keys of
/// its resources, their game values and changes served in load order.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::typed::{Resource, ResourceType};
///
/// fn keep<T: Resource>(load: &mut enshwick::Load<'_>, guid: Guid) -> Result<(), enshwick::Off> {
///     let ty = ResourceType::<T>::resolve(load)?;
///     let key = ty.key(guid);
///     ty.serve(load, key, key, |_value| {})
/// }
/// ```
pub struct ResourceType<T> {
    raw: RawResource,
    of: PhantomData<fn() -> T>,
}

impl<T> Clone for ResourceType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ResourceType<T> {}

impl<T> core::fmt::Debug for ResourceType<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResourceType")
            .field("name", &self.raw.name)
            .finish_non_exhaustive()
    }
}

impl<T: Resource> ResourceType<T> {
    /// `T` on the running build, its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks the type or its shape differs from
    /// `T`, naming the build.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            raw: RawResource::resolve(load, T::ID, &check::<T>)?,
            of: PhantomData,
        })
    }

    /// The key of the resource of this type with GUID `guid`, part 0.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    #[must_use]
    pub fn key(&self, guid: Guid) -> crate::abi::ResourceKey {
        self.raw.key(guid)
    }

    /// The game's own resource `key`, as the archive holds it.
    ///
    /// # Errors
    ///
    /// [`Off`] for a key of another type, outside the game, for a key the
    /// archive lacks, and when its bytes do not read as `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::typed::{Resource, ResourceType};
    ///
    /// fn read<T: Resource>(ty: ResourceType<T>, guid: Guid) -> Result<T, enshwick::Off> {
    ///     ty.game_value(ty.key(guid))
    /// }
    /// ```
    pub fn game_value(&self, key: crate::abi::ResourceKey) -> Result<T, Off> {
        Ok(T::from_value(&self.raw.game_value(key)?)?)
    }

    /// Serves the resource `key`, made from the resource `base` read as
    /// `T` and changed by `change`, the first time the game reads it; the
    /// bytes of what `T` does not hold stay as `base` has them. Mods that
    /// serve one key change it in load order, each on the one before's
    /// result, as [`Load::serve`] does. A base that does not read as `T`,
    /// or a change that does not fit the type, is logged and the base is
    /// served unchanged.
    ///
    /// # Errors
    ///
    /// [`Off`] for a key or base of another type, and when the loader
    /// refuses it, such as after init.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    pub fn serve<F>(
        &self,
        load: &mut Load<'_>,
        key: crate::abi::ResourceKey,
        base: crate::abi::ResourceKey,
        change: F,
    ) -> Result<(), Off>
    where
        F: Fn(&mut T) + Send + Sync + 'static,
    {
        self.raw.serve(load.host(), key, base, move |value| {
            let mut typed = T::from_value(&value)?;
            change(&mut typed);
            Ok(Some(typed.to_value()))
        })
    }
}
