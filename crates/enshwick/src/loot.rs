//! Loot containers on the server: the corpses and loot bags a dead enemy
//! leaves, and other containers that dissolve once emptied.
//!
//! The game checks every such container each step in its system [`SYSTEM`],
//! on a worker thread. Hook that system and walk its rows with
//! [`Containers::each`] from the hook; [`Containers::container`] reads any
//! entity the same way. Take items out with
//! [`Inventories`](crate::inventory::Inventories): the containers walked here
//! all have an `InventorySetup`, so `move_all` empties one.
//!
//! On a client that hosts a game the system also runs in the client's own
//! world; [`Worlds::is_server`](crate::ecs::Worlds::is_server) tells the two
//! apart.
//!
//! [`Load::add_loot`] puts a mod item into random loot with an exact chance,
//! filtered by [`Source`]: corpses, bosses, chests, locked chests, loot labels.

mod drops;

pub use drops::{Drop, Kinds, LootContainerId, MeterRoll, Roll, Source, replace_drops};
pub(crate) use drops::{replace_group, watch_rolls};

use crate::Entity;
use crate::ecs::{ComponentType, Components, Ecs};
use crate::engine_ids as ids;
use crate::raw::read_at;
use crate::{Load, Off};

/// The system that checks every loot container each step, for
/// [`Load::system`].
pub const SYSTEM: &str = "destroy_inventory_on_loot";

/// Iterator words of [`SYSTEM`]: context, `InventoryEmptyState`,
/// `InventorySetup`, `DestroyOnLoot` and the change tracker.
const ROW_WORDS: usize = 9;

/// The last interaction a player started with a container.
///
/// # Examples
///
/// See [`Container`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InteractionOfferRuntime {
    /// The entity that last opened or looted the container; `None` before
    /// anyone did.
    pub guest_id: Option<Entity>,
    /// When, in the game's nanoseconds; changes with each interaction.
    pub last_accept_time: i64,
    /// Whether the game offers no interaction now.
    pub is_disabled: bool,
}

/// What a loot container is and holds, read in one go.
///
/// The name is enshwick's: several of the game's components read together.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
/// use enshwick::loot::{Container, InteractionOfferRuntime};
///
/// let corpse = Container {
///     entity: Entity(900),
///     position: [10.0, 2.0, -4.0],
///     is_empty: false,
///     expires: true,
///     area_loot: Some(true),
///     offer: Some(InteractionOfferRuntime { guest_id: None, last_accept_time: 0, is_disabled: false }),
/// };
/// assert!(corpse.is_enemy_loot());
/// assert!(corpse.is_open());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Container {
    /// The container.
    pub entity: Entity,
    /// Where it stands, in metres; y is up.
    pub position: [f64; 3],
    /// Whether it holds nothing, as the game last recorded.
    pub is_empty: bool,
    /// Whether it despawns on its own after a while (`LifeTime`).
    pub expires: bool,
    /// Its `AreaLootOverride`: `Some(true)` when area loot may take it,
    /// `Some(false)` when excluded (items a player dropped), `None` without
    /// one.
    pub area_loot: Option<bool>,
    /// Its interaction offer; `None` without one.
    pub offer: Option<InteractionOfferRuntime>,
}

impl Container {
    /// Whether it is loot an enemy left: it dissolves when looted, despawns
    /// on its own and is open to area loot. Player chests, world chests and
    /// quest containers are not.
    ///
    /// # Examples
    ///
    /// See [`Container`].
    #[must_use]
    pub const fn is_enemy_loot(&self) -> bool {
        self.expires && matches!(self.area_loot, Some(true))
    }

    /// Whether it is enemy loot a player could take from now: it holds
    /// something and offers its interaction.
    ///
    /// # Examples
    ///
    /// See [`Container`].
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.is_enemy_loot()
            && !self.is_empty
            && matches!(
                self.offer,
                Some(InteractionOfferRuntime {
                    is_disabled: false,
                    ..
                })
            )
    }
}

/// Component types and field offsets, from reflection.
#[derive(Clone, Copy, Debug)]
struct Layout {
    transform: ComponentType,
    empty_state: ComponentType,
    destroy: ComponentType,
    lifetime: ComponentType,
    area_loot: ComponentType,
    offer: ComponentType,
    player_input: ComponentType,
    is_empty: usize,
    exclude: usize,
    accept_time: usize,
    accept_guest: usize,
    disabled: usize,
}

/// Reads of loot containers from inside a hooked system; get it with
/// [`Containers::resolve`].
///
/// # Examples
///
/// ```
/// use enshwick::loot::{Containers, SYSTEM};
///
/// enshwick::detour! {
///     static CHECK: unsafe extern "system" fn(ctx: *mut u64) = check;
/// }
///
/// static CONTAINERS: std::sync::OnceLock<Containers> = std::sync::OnceLock::new();
///
/// fn check(next: enshwick::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
///     // SAFETY: the original system takes its own context.
///     unsafe { next.get()(ctx) };
///     if let Some(containers) = CONTAINERS.get() {
///         let mut open = 0;
///         // SAFETY: `ctx` is the live context of `SYSTEM`, inside its hook.
///         unsafe { containers.each(ctx, |c| open += usize::from(c.is_open())) };
///         let _ = open;
///     }
/// }
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let _ = CONTAINERS.set(Containers::resolve(load)?);
///     let system = load.system(SYSTEM)?;
///     // SAFETY: the reflected system takes its context, as `CHECK` does.
///     unsafe { load.hook(&CHECK, system) }
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Containers {
    ecs: Ecs,
    components: Components,
    layout: Layout,
}

impl Containers {
    /// Resolves the reads for the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader or the game's reflection lacks a piece.
    ///
    /// # Examples
    ///
    /// See [`Containers`].
    pub fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        let ty = |id: &str| ComponentType::resolve(load, id);
        let offset = |id: &str| {
            load.constant(id)
                .and_then(|v| usize::try_from(v).map_err(|_| Off::new(format!("offset {id}"))))
        };
        let layout = Layout {
            transform: ty(ids::CURRENT_TRANSFORM)?,
            empty_state: ty(ids::INVENTORY_EMPTY_STATE)?,
            destroy: ty(ids::DESTROY_ON_LOOT)?,
            lifetime: ty(ids::LIFETIME)?,
            area_loot: ty(ids::AREA_LOOT)?,
            offer: ty(ids::INTERACTION_OFFER)?,
            player_input: ty(ids::PLAYER_INPUT)?,
            is_empty: offset(ids::INVENTORY_EMPTY_STATE_EMPTY)?,
            exclude: offset(ids::AREA_LOOT_EXCLUDE)?,
            accept_time: offset(ids::INTERACTION_OFFER_ACCEPT_TIME)?,
            accept_guest: offset(ids::INTERACTION_OFFER_ACCEPT_GUEST)?,
            disabled: offset(ids::INTERACTION_OFFER_DISABLED)?,
        };
        Ok(Self {
            ecs: Ecs::resolve(load)?,
            components: Components::resolve(load)?,
            layout,
        })
    }

    /// Calls `f` with every loot container [`SYSTEM`] checks this step, in
    /// its order, and leaves the system's row where it was.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context [`SYSTEM`] was called with, used on
    ///   its thread during its hook.
    ///
    /// # Examples
    ///
    /// See [`Containers`].
    pub unsafe fn each(&self, ctx: *mut u64, mut f: impl FnMut(&Container)) {
        // SAFETY: the caller guarantees `ctx` is the live context of
        // `SYSTEM`, used on its thread during its hook.
        let saved = unsafe { self.ecs.row(ctx) };
        // SAFETY: `ctx` is the live context of `SYSTEM` on its thread, as the
        // caller guarantees, and 0 is the row before the first.
        unsafe { self.ecs.set_row(ctx, 0) };
        // SAFETY: `ctx` is the live context of `SYSTEM` on its thread, as the
        // caller guarantees; `ROW_WORDS` is its iterator's word count, and
        // `query` is dropped before this function returns.
        let mut query = unsafe { self.ecs.query::<ROW_WORDS>(ctx) };
        while query.next().is_some() {
            // SAFETY: `ctx` is the live context of `SYSTEM` on its thread, as
            // the caller guarantees, now on the row `next` moved to.
            let Some(entity) = (unsafe { self.ecs.entity(ctx) }) else {
                continue;
            };
            // SAFETY: `ctx` is the live context of `SYSTEM` on its thread, as
            // the caller guarantees.
            if let Some(container) = unsafe { self.container(ctx, entity) } {
                f(&container);
            }
        }
        // SAFETY: `ctx` is the live context of `SYSTEM` on its thread, as the
        // caller guarantees, and `saved` is the row it held on entry.
        unsafe { self.ecs.set_row(ctx, saved) };
    }

    /// `entity` read as a loot container; `None` when it is not one (it has
    /// no `DestroyOnLoot`, inventory state or position) or is gone.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context a hooked system was called with,
    ///   used on that system's thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn still_full(
    ///     containers: &enshwick::loot::Containers,
    ///     ctx: *mut u64,
    ///     corpse: enshwick::Entity,
    /// ) -> bool {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { containers.container(ctx, corpse) }.is_some_and(|c| !c.is_empty)
    /// }
    /// ```
    #[must_use]
    pub unsafe fn container(&self, ctx: *mut u64, entity: Entity) -> Option<Container> {
        let layout = &self.layout;
        // SAFETY: the caller guarantees `ctx` is a hooked system's live
        // context on its thread, which is the contract of `component`.
        let read = |hash| unsafe { self.components.component(ctx, entity, hash) };
        read(layout.destroy)?;
        let state = read(layout.empty_state)?;
        // SAFETY: `component` returned the entity's live CurrentTransform,
        // read on the system's thread during the hook.
        let position = unsafe { crate::ecs::transform_position(read(layout.transform)?) };
        // SAFETY: `state` is the entity's live InventoryEmptyState, whose
        // `isEmpty` byte sits at the reflected offset `is_empty`.
        let empty = unsafe { read_at::<u8>(state, layout.is_empty) } != 0;
        let area_loot = read(layout.area_loot).map(|data| {
            // SAFETY: `data` is the entity's live AreaLootOverride, whose
            // `exclude` byte sits at the reflected offset `exclude`.
            unsafe { read_at::<u8>(data, layout.exclude) == 0 }
        });
        let offer = read(layout.offer).map(|data| InteractionOfferRuntime {
            // SAFETY: `data` is the entity's live InteractionOfferRuntime,
            // whose `lastAcceptData.guestId` u32 sits at the reflected offset
            // `accept_guest`.
            guest_id: Entity::from_raw(unsafe { read_at(data, layout.accept_guest) }),
            // SAFETY: `data` is the entity's live InteractionOfferRuntime,
            // whose `lastAcceptTime` i64 sits at the reflected offset
            // `accept_time`.
            last_accept_time: unsafe { read_at(data, layout.accept_time) },
            // SAFETY: `data` is the entity's live InteractionOfferRuntime,
            // whose `isDisabled` byte sits at the reflected offset `disabled`.
            is_disabled: unsafe { read_at::<u8>(data, layout.disabled) } != 0,
        });
        Some(Container {
            entity,
            position,
            is_empty: empty,
            expires: read(layout.lifetime).is_some(),
            area_loot,
            offer,
        })
    }

    /// `entity`'s position in metres.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context a hooked system was called with,
    ///   used on that system's thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn at(c: &enshwick::loot::Containers, ctx: *mut u64, e: enshwick::Entity) -> bool {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { c.position(ctx, e) }.is_some()
    /// }
    /// ```
    #[must_use]
    pub unsafe fn position(&self, ctx: *mut u64, entity: Entity) -> Option<[f64; 3]> {
        // SAFETY: the caller guarantees `ctx` is a hooked system's live
        // context on its thread, which is the contract of `component`.
        let data = unsafe {
            self.components
                .component(ctx, entity, self.layout.transform)
        }?;
        // SAFETY: `component` returned the entity's live CurrentTransform,
        // read on the system's thread during the hook.
        Some(unsafe { crate::ecs::transform_position(data) })
    }

    /// Whether `entity` is a player character.
    ///
    /// # Safety
    ///
    /// - `ctx` is the live query context a hooked system was called with,
    ///   used on that system's thread during the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn by_player(c: &enshwick::loot::Containers, ctx: *mut u64, e: enshwick::Entity) -> bool {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { c.is_player(ctx, e) }
    /// }
    /// ```
    #[must_use]
    pub unsafe fn is_player(&self, ctx: *mut u64, entity: Entity) -> bool {
        // SAFETY: the caller guarantees `ctx` is a hooked system's live
        // context on its thread, which is the contract of `component`.
        unsafe {
            self.components
                .component(ctx, entity, self.layout.player_input)
        }
        .is_some()
    }
}

/// The loot containers of one run of [`SYSTEM`], for [`Load::on_loot_pass`];
/// valid only during the call.
///
/// # Examples
///
/// See [`Load::on_loot_pass`].
#[derive(Debug)]
pub struct LootPass<'a> {
    containers: &'a Containers,
    ctx: *mut u64,
}

impl LootPass<'_> {
    /// Calls `f` with every loot container this run checks, in its order.
    ///
    /// # Examples
    ///
    /// See [`Load::on_loot_pass`].
    pub fn each(&self, f: impl FnMut(&Container)) {
        // SAFETY: only `after_check` makes a `LootPass`, from the live context
        // of `SYSTEM` on its thread, and lends it to the handler for that call
        // alone; the pass is neither `Clone` nor `Copy`.
        unsafe { self.containers.each(self.ctx, f) };
    }

    /// `entity` read as a loot container; `None` when it is not one or gone.
    ///
    /// # Examples
    ///
    /// ```
    /// fn full(pass: &enshwick::loot::LootPass<'_>, e: enshwick::Entity) -> bool {
    ///     pass.container(e).is_some_and(|c| !c.is_empty)
    /// }
    /// ```
    #[must_use]
    pub fn container(&self, entity: Entity) -> Option<Container> {
        // SAFETY: only `after_check` makes a `LootPass`, from the live context
        // of `SYSTEM` on its thread, and lends it to the handler for that call
        // alone; the pass is neither `Clone` nor `Copy`.
        unsafe { self.containers.container(self.ctx, entity) }
    }

    /// `entity`'s position in metres.
    ///
    /// # Examples
    ///
    /// ```
    /// fn here(pass: &enshwick::loot::LootPass<'_>, e: enshwick::Entity) -> bool {
    ///     pass.position(e).is_some()
    /// }
    /// ```
    #[must_use]
    pub fn position(&self, entity: Entity) -> Option<[f64; 3]> {
        // SAFETY: only `after_check` makes a `LootPass`, from the live context
        // of `SYSTEM` on its thread, and lends it to the handler for that call
        // alone; the pass is neither `Clone` nor `Copy`.
        unsafe { self.containers.position(self.ctx, entity) }
    }

    /// Whether `entity` is a player character.
    ///
    /// # Examples
    ///
    /// ```
    /// fn by_player(pass: &enshwick::loot::LootPass<'_>, e: enshwick::Entity) -> bool {
    ///     pass.is_player(e)
    /// }
    /// ```
    #[must_use]
    pub fn is_player(&self, entity: Entity) -> bool {
        // SAFETY: only `after_check` makes a `LootPass`, from the live context
        // of `SYSTEM` on its thread, and lends it to the handler for that call
        // alone; the pass is neither `Clone` nor `Copy`.
        unsafe { self.containers.is_player(self.ctx, entity) }
    }
}

type PassHandler = Box<dyn Fn(&LootPass<'_>) + Send + Sync>;

static PASS: crate::fresh::Fresh<(Containers, crate::ecs::Worlds, PassHandler)> =
    crate::fresh::Fresh::new();

crate::detour! {
    static CHECK: unsafe extern "system" fn(ctx: *mut u64) = after_check;
}

fn after_check(next: crate::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    // SAFETY: `ctx` is the context the game called `SYSTEM` with on this
    // thread, passed on unchanged, so the rest of the chain gets what the
    // game's own call gave it.
    unsafe { next.get()(ctx) };
    if let Some((containers, worlds, handler)) = PASS.get()
        // SAFETY: `ctx` is the live context the game passed to `SYSTEM`, on
        // its thread, during the hook.
        && unsafe { worlds.is_server(ctx) }
    {
        // Guarded here: the detour's own guard would run the system again.
        crate::host::guard(
            "panic in a loot pass handler",
            || (),
            || handler(&LootPass { containers, ctx }),
        );
    }
}

impl Load<'_> {
    /// Runs `handler` after every run of the game's loot container check in
    /// the server's world, on the worker thread that runs it, with that run's
    /// containers. One handler per mod.
    ///
    /// # Errors
    ///
    /// [`Off`] for a second handler, or when the loader or the game lacks a
    /// piece.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_loot_pass(|pass| {
    ///         let mut emptied = 0;
    ///         pass.each(|container| emptied += usize::from(container.is_empty));
    ///         let _ = emptied;
    ///     })
    /// }
    /// ```
    pub fn on_loot_pass<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&LootPass<'_>) + Send + Sync + 'static,
    {
        let containers = Containers::resolve(self)?;
        let worlds = crate::ecs::Worlds::resolve(self)?;
        let system = self.system(SYSTEM)?;
        if PASS.set((containers, worlds, Box::new(handler))).is_err() {
            return Err(Off::new("one loot pass handler per mod"));
        }
        // SAFETY: `system` is the reflected update function of `SYSTEM`,
        // which takes one context pointer, the signature `CHECK` declares.
        unsafe { self.hook(&CHECK, system) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpse() -> Container {
        Container {
            entity: Entity(900),
            position: [0.0; 3],
            is_empty: false,
            expires: true,
            area_loot: Some(true),
            offer: Some(InteractionOfferRuntime {
                guest_id: None,
                last_accept_time: 0,
                is_disabled: false,
            }),
        }
    }

    #[test]
    fn only_open_enemy_loot_passes() {
        assert!(corpse().is_open());
        let chest = Container {
            expires: false,
            ..corpse()
        };
        assert!(!chest.is_enemy_loot());
        let dropped = Container {
            area_loot: Some(false),
            ..corpse()
        };
        assert!(!dropped.is_enemy_loot());
        let quest = Container {
            area_loot: None,
            ..corpse()
        };
        assert!(!quest.is_enemy_loot());
        let looted = Container {
            is_empty: true,
            ..corpse()
        };
        assert!(looted.is_enemy_loot() && !looted.is_open());
        let closed = Container {
            offer: Some(InteractionOfferRuntime {
                is_disabled: true,
                ..corpse().offer.unwrap()
            }),
            ..corpse()
        };
        assert!(!closed.is_open());
        let silent = Container {
            offer: None,
            ..corpse()
        };
        assert!(!silent.is_open());
    }
}
