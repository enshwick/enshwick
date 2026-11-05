//! The handles that resolve a generated component, event or resource on the
//! running build and check its shape: a core per role that knows only the
//! type's id, and the typed handle a Rust mod holds over it.

use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};

use super::{Component, Event, Named, Reflected, Resource, ShapeError, check, sample};
use crate::content::Guid;
use crate::resources::{Registry, Type, Value};
use crate::world::{World, WritableType};
use crate::{Entity, Host, Load, Off};

/// The running build's type for the id `id`, its shape checked: a
/// [`sample`] of the build's type must pass `check`, so a field the build
/// renamed, removed, or retyped to a value the generated type cannot hold,
/// such as a wider integer, at any depth, turns off only that type.
fn resolve(
    load: &Load<'_>,
    id: &'static str,
    check: fn(&Value) -> Result<(), ShapeError>,
) -> Result<&'static Type<'static>, Off> {
    let host = load.host();
    let hash = crate::ecs::ComponentType::resolve(load, id)
        .map_err(|e| off(id, host, &e.to_string()))?
        .hash();
    let registry = Registry::game()?;
    let ty = registry
        .hashed(hash)
        .ok_or_else(|| off(id, host, "the type registry lacks it"))?;
    let value = sample(registry, ty.qualified_name())
        .ok_or_else(|| off(id, host, "its value does not read"))?;
    check(&value).map_err(|e| off(id, host, &e.to_string()))?;
    Ok(ty)
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
    writable: Option<WritableType>,
}

impl RawComponent {
    /// The component type `id` on the running build, its shape held to
    /// `check`.
    pub(crate) fn resolve(
        load: &Load<'_>,
        id: &'static str,
        check: fn(&Value) -> Result<(), ShapeError>,
    ) -> Result<Self, Off> {
        let ty = resolve(load, id, check)?;
        Ok(Self {
            id,
            ty: crate::ecs::ComponentType::resolve(load, id)?,
            name: ty.qualified_name(),
            size: usize::try_from(ty.size()).unwrap_or(usize::MAX),
            writable: WritableType::resolve(load, id).ok(),
        })
    }

    /// `entity`'s component on the game thread; `None` when the entity has
    /// none or off the game thread.
    pub(crate) fn read(&self, world: World, entity: Entity) -> Result<Option<Value>, Off> {
        let Some(data) = world.component(entity, self.ty) else {
            return Ok(None);
        };
        let mut bytes = vec![0; self.size];
        // SAFETY: `World::component` returned the entity's live component,
        // valid until the game thread moves on and read at once; `size` is
        // the reflected size of its type on the running build.
        unsafe { crate::raw::read_bytes_at(data.cast(), 0, &mut bytes) };
        Ok(Some(Registry::game()?.decode(self.name, &bytes)?))
    }

    /// Writes `value` to `entity`'s component, marked changed the game's
    /// way, keeping the bytes of what `value` leaves out.
    pub(crate) fn write(&self, world: World, entity: Entity, value: Value) -> Result<(), Off> {
        let id = self.id;
        let writable = self
            .writable
            .ok_or_else(|| Off::new(format!("typed {id}: mods may not write it")))?;
        let name = self.name;
        world
            .update(entity, writable, |bytes| {
                let written = Registry::game()?.encode(name, &value, Some(bytes))?;
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
        check: fn(&Value) -> Result<(), ShapeError>,
    ) -> Result<Self, Off> {
        let ty = resolve(load, id, check)?;
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
            let handled = Registry::game()
                .and_then(|registry| Ok(registry.decode(name, raw.bytes())?))
                .and_then(|value| handler(&value));
            if let Err(why) = handled
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
        check: fn(&Value) -> Result<(), ShapeError>,
    ) -> Result<Self, Off> {
        let ty = resolve(load, id, check)?;
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

    /// The type's qualified name on the running build.
    pub(crate) const fn name(&self) -> &'static str {
        self.name
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
    /// [`Off`] when the build lacks the type or its shape differs from
    /// `T`, naming the build.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            raw: RawComponent::resolve(load, T::ID, check::<T>)?,
            of: PhantomData,
        })
    }

    /// `entity`'s component on the game thread; `None` when the entity has
    /// none or off the game thread.
    ///
    /// # Errors
    ///
    /// [`Off`] when the component's bytes do not read as `T`.
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
        self.raw.write(world, entity, value.to_value())
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
            raw: RawEvent::resolve(load, T::ID, check::<T>)?,
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
            raw: RawResource::resolve(load, T::ID, check::<T>)?,
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
