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

pub use drops::{Drop, Kinds, Roll, Source};

use crate::ecs::{Components, Ecs, Entity};
use crate::{Load, Off};

/// The system that checks every loot container each step, for
/// [`Load::system`].
pub const SYSTEM: &str = "destroy_inventory_on_loot";

/// Iterator words of [`SYSTEM`]: context, `InventoryEmptyState`,
/// `InventorySetup`, `DestroyOnLoot` and the change tracker.
const ROW_WORDS: usize = 9;

/// The last interaction a player started with a container.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Offer {
    /// The entity that last opened or looted the container; none before
    /// anyone did.
    pub guest: Entity,
    /// When, in the game's nanoseconds; changes with each interaction.
    pub accepted_at: i64,
    /// Whether the game offers no interaction now.
    pub disabled: bool,
}

/// What a loot container is and holds, read in one go.
///
/// # Examples
///
/// ```
/// use enshwick_api::ecs::Entity;
/// use enshwick_api::loot::{Container, Offer};
///
/// let corpse = Container {
///     entity: Entity(900),
///     position: [10.0, 2.0, -4.0],
///     empty: false,
///     expires: true,
///     area_loot: Some(true),
///     offer: Some(Offer { guest: Entity::NONE, accepted_at: 0, disabled: false }),
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
    pub empty: bool,
    /// Whether it despawns on its own after a while (`LifeTime`).
    pub expires: bool,
    /// Its `AreaLootOverride`: `Some(true)` when area loot may take it,
    /// `Some(false)` when excluded (items a player dropped), `None` without
    /// one.
    pub area_loot: Option<bool>,
    /// Its interaction offer; `None` without one.
    pub offer: Option<Offer>,
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
            && !self.empty
            && matches!(
                self.offer,
                Some(Offer {
                    disabled: false,
                    ..
                })
            )
    }
}

/// Whether `at` lies within `radius` metres of `center` across the ground
/// and within `height` metres above or below it.
///
/// # Examples
///
/// ```
/// use enshwick_api::loot::near;
///
/// assert!(near([0.0, 0.0, 0.0], [6.0, 3.0, 8.0], 10.0, 4.0));
/// assert!(!near([0.0, 0.0, 0.0], [6.0, 5.0, 8.0], 10.0, 4.0));
/// assert!(!near([0.0, 0.0, 0.0], [9.0, 0.0, 9.0], 12.0, 4.0));
/// ```
#[must_use]
pub fn near(center: [f64; 3], at: [f64; 3], radius: f64, height: f64) -> bool {
    let (dx, dy, dz) = (at[0] - center[0], at[1] - center[1], at[2] - center[2]);
    dx.mul_add(dx, dz * dz) <= radius * radius && dy.abs() <= height
}

/// Component hashes and field offsets, from reflection.
#[derive(Clone, Copy, Debug)]
struct Layout {
    transform: u32,
    empty_state: u32,
    destroy: u32,
    lifetime: u32,
    area_loot: u32,
    offer: u32,
    player_input: u32,
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
/// use enshwick_api::loot::{Containers, SYSTEM};
///
/// enshwick_api::detour! {
///     static CHECK: unsafe extern "system" fn(ctx: *mut u64) = check;
/// }
///
/// static CONTAINERS: std::sync::OnceLock<Containers> = std::sync::OnceLock::new();
///
/// fn check(next: enshwick_api::Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
///     // SAFETY: the original system takes its own context.
///     unsafe { next.get()(ctx) };
///     if let Some(containers) = CONTAINERS.get() {
///         let mut open = 0;
///         // SAFETY: the system's live context, on its thread.
///         unsafe { containers.each(ctx, |c| open += usize::from(c.is_open())) };
///         let _ = open;
///     }
/// }
///
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
///     let _ = CONTAINERS.set(Containers::resolve(load)?);
///     load.hook(&CHECK, load.system(SYSTEM)?)
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
        let world = load.world()?;
        let ty = |name: &str| {
            world
                .component_type(load, name)
                .map(crate::world::ComponentType::hash)
        };
        let offset = |ty: &str, field: &str| {
            load.field_offset(ty, field)
                .and_then(|v| usize::try_from(v).map_err(|_| Off::new("offset")))
        };
        let layout = Layout {
            transform: ty("keen::ecs::CurrentTransform")?,
            empty_state: ty("keen::ecs::InventoryEmptyState")?,
            destroy: ty("keen::ecs::DestroyOnLoot")?,
            lifetime: ty("keen::ecs::LifeTime")?,
            area_loot: ty("keen::ecs::AreaLootOverride")?,
            offer: ty("keen::ecs::InteractionOfferRuntime")?,
            player_input: ty("keen::ecs::PlayerInput")?,
            is_empty: offset("keen::ecs::InventoryEmptyState", "isEmpty")?,
            exclude: offset("keen::ecs::AreaLootOverride", "exclude")?,
            accept_time: offset("keen::ecs::InteractionOfferRuntime", "lastAcceptTime")?,
            accept_guest: offset("keen::ecs::InteractionOfferRuntime", "lastAcceptData")?
                + offset("keen::ecs::InteractionAcceptData", "guestId")?,
            disabled: offset("keen::ecs::InteractionOfferRuntime", "isDisabled")?,
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
    /// `ctx` is the live context of [`SYSTEM`], used on its thread during
    /// its hook.
    ///
    /// # Examples
    ///
    /// See [`Containers`].
    pub unsafe fn each(&self, ctx: *mut u64, mut f: impl FnMut(&Container)) {
        // SAFETY: the system's live context, as the caller vouches.
        let saved = unsafe { self.ecs.row(ctx) };
        // SAFETY: as above.
        unsafe { self.ecs.set_row(ctx, 0) };
        // SAFETY: as above, with the system's iterator size.
        let mut query = unsafe { self.ecs.query::<ROW_WORDS>(ctx) };
        while query.next().is_some() {
            // SAFETY: as above, on the current row.
            let entity = unsafe { self.ecs.entity(ctx) };
            // SAFETY: as above.
            if let Some(container) = unsafe { self.container(ctx, entity) } {
                f(&container);
            }
        }
        // SAFETY: as above.
        unsafe { self.ecs.set_row(ctx, saved) };
    }

    /// `entity` read as a loot container; `None` when it is not one (it has
    /// no `DestroyOnLoot`, inventory state or position) or is gone.
    ///
    /// # Safety
    ///
    /// `ctx` is the live context of a hooked system, used on its thread.
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn still_full(
    ///     containers: &enshwick_api::loot::Containers,
    ///     ctx: *mut u64,
    ///     corpse: enshwick_api::ecs::Entity,
    /// ) -> bool {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { containers.container(ctx, corpse) }.is_some_and(|c| !c.empty)
    /// }
    /// ```
    #[must_use]
    pub unsafe fn container(&self, ctx: *mut u64, entity: Entity) -> Option<Container> {
        let layout = &self.layout;
        // SAFETY: forwarded from the caller, for each read below.
        let read = |hash| unsafe { self.components.component(ctx, entity, hash) };
        read(layout.destroy)?;
        let state = read(layout.empty_state)?;
        let position = self.position_of(read(layout.transform)?);
        // SAFETY: a live InventoryEmptyState holds its flag here.
        let empty = unsafe { read_at::<u8>(state, layout.is_empty) } != 0;
        let area_loot = read(layout.area_loot).map(|data| {
            // SAFETY: a live AreaLootOverride holds its flag here.
            unsafe { read_at::<u8>(data, layout.exclude) == 0 }
        });
        let offer = read(layout.offer).map(|data| Offer {
            // SAFETY: a live InteractionOfferRuntime holds these fields.
            guest: Entity(unsafe { read_at(data, layout.accept_guest) }),
            // SAFETY: as above.
            accepted_at: unsafe { read_at(data, layout.accept_time) },
            // SAFETY: as above.
            disabled: unsafe { read_at::<u8>(data, layout.disabled) } != 0,
        });
        Some(Container {
            entity,
            position,
            empty,
            expires: read(layout.lifetime).is_some(),
            area_loot,
            offer,
        })
    }

    /// `entity`'s position in metres.
    ///
    /// # Safety
    ///
    /// As for [`Containers::container`].
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn at(c: &enshwick_api::loot::Containers, ctx: *mut u64, e: enshwick_api::ecs::Entity) -> bool {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { c.position(ctx, e) }.is_some()
    /// }
    /// ```
    #[must_use]
    pub unsafe fn position(&self, ctx: *mut u64, entity: Entity) -> Option<[f64; 3]> {
        // SAFETY: forwarded from the caller.
        let data = unsafe {
            self.components
                .component(ctx, entity, self.layout.transform)
        }?;
        Some(self.position_of(data))
    }

    /// Whether `entity` is a player character.
    ///
    /// # Safety
    ///
    /// As for [`Containers::container`].
    ///
    /// # Examples
    ///
    /// ```
    /// unsafe fn by_player(c: &enshwick_api::loot::Containers, ctx: *mut u64, e: enshwick_api::ecs::Entity) -> bool {
    ///     // SAFETY: called from a system's hook with its context.
    ///     unsafe { c.is_player(ctx, e) }
    /// }
    /// ```
    #[must_use]
    pub unsafe fn is_player(&self, ctx: *mut u64, entity: Entity) -> bool {
        // SAFETY: forwarded from the caller.
        unsafe {
            self.components
                .component(ctx, entity, self.layout.player_input)
        }
        .is_some()
    }

    #[expect(clippy::unused_self, reason = "kept beside the other reads")]
    fn position_of(&self, transform: *mut u8) -> [f64; 3] {
        // SAFETY: a live CurrentTransform starts with x, y and z in 32.32
        // fixed point.
        let fixed = unsafe { transform.cast::<[i64; 3]>().read_unaligned() };
        crate::ecs::metres(fixed)
    }
}

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// A readable `T` lies there.
unsafe fn read_at<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpse() -> Container {
        Container {
            entity: Entity(900),
            position: [0.0; 3],
            empty: false,
            expires: true,
            area_loot: Some(true),
            offer: Some(Offer {
                guest: Entity::NONE,
                accepted_at: 0,
                disabled: false,
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
            empty: true,
            ..corpse()
        };
        assert!(looted.is_enemy_loot() && !looted.is_open());
        let closed = Container {
            offer: Some(Offer {
                disabled: true,
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

    #[test]
    fn near_measures_ground_and_height_apart() {
        let c = [100.0, 50.0, -20.0];
        assert!(near(c, c, 0.0, 0.0));
        assert!(near(c, [112.0, 50.0, -20.0], 12.0, 4.0));
        assert!(!near(c, [112.1, 50.0, -20.0], 12.0, 4.0));
        assert!(near(c, [100.0, 46.0, -20.0], 12.0, 4.0));
        assert!(!near(c, [100.0, 54.5, -20.0], 12.0, 4.0));
        assert!(!near(c, [109.0, 50.0, -11.0], 12.0, 4.0));
    }
}
