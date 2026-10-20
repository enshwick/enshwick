//! Survivors: who answers the game's question "which entity is this
//! survivor", their beds, summoning staffs and the client's survivor menu.
//!
//! The game keeps one entity per survivor and asks for it from many systems.
//! A mod that keeps several entities of a survivor, such as one per base,
//! answers each asker with the one that suits it; the platform tells the
//! askers apart and reads what they work on. Cells are whole 0.5 m cells, as
//! in [`altars`](crate::altars).
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::survivors::{Answer, Purpose};
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     // Never move a survivor with its staff: always spawn a new one.
//!     load.on_survivor_lookup(|lookup| match lookup.purpose {
//!         Purpose::Spawn { .. } => Answer::Nobody,
//!         _ => Answer::Vanilla,
//!     })
//! }
//! ```

use core::ffi::c_void;

use crate::abi::{
    BedCheck as RawBedCheck, HostApi, Summon as RawSummon, SurvivorLookup as RawLookup,
};
use crate::content::Guid;
use crate::ecs::Entity;
use crate::{Load, Off};

/// A survivor's id, the same for every entity of it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::survivors::SurvivorId;
///
/// let blacksmith = SurvivorId(1_201_801_546);
/// assert_eq!(blacksmith.0, 1_201_801_546);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurvivorId(pub u32);

/// Who asks for a survivor's entity, with what it works on.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Purpose {
    /// The pass that keeps one entity per survivor looks at `entity`, whose
    /// home is `home`. Answer `entity` to keep it, another entity to have the
    /// game remove `entity`, or [`Answer::Nobody`] when there is no entity yet. After
    /// an answer that keeps another entity than the game's, the survivor's
    /// record stays on the game's own.
    Uniqueness {
        /// The entity looked at.
        entity: Entity,
        /// Its saved spawn cell, or where it stands.
        home: [i64; 3],
    },
    /// A summoning staff used at `target`: answer the entity to move there,
    /// or [`Answer::Nobody`] to spawn a new one. The move then reaches only
    /// the answered entity.
    Spawn {
        /// The cell the staff aims at.
        target: [i64; 3],
    },
    /// A bed at `bed_at` binding its sleeper: answer the entity that may
    /// sleep there, or [`Answer::Nobody`].
    BedOwner {
        /// The bed's cell.
        bed_at: [i64; 3],
    },
    /// A player gives `bed` at `bed_at` to the survivor with its staff:
    /// answer the entity that gets it.
    AssignBed {
        /// The bed.
        bed: Entity,
        /// Its cell.
        bed_at: [i64; 3],
    },
    /// The same assignment asks who slept in `bed` before: answer that
    /// entity, or [`Answer::Nobody`]. [`Lookup::bed_of`] tells.
    PreviousOccupant {
        /// The bed.
        bed: Entity,
        /// Its cell.
        bed_at: [i64; 3],
    },
    /// A survivor's idle behaviour: `entity` stands at `at`; answer the entity
    /// whose base it keeps to.
    Roaming {
        /// The survivor's entity.
        entity: Entity,
        /// Where it stands.
        at: [i64; 3],
    },
    /// The first binding of beds after a load, before beds bind their
    /// sleepers.
    BedBinding,
    /// The cursor measuring how far a bed lies from the survivor before it
    /// offers to assign it; [`Answer::Nobody`] skips the measure. A client
    /// alone asks this too, about its own cursor.
    Cursor,
}

/// A lookup the game makes, from [`Load::on_survivor_lookup`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Debug)]
pub struct Lookup {
    /// The survivor.
    pub survivor: SurvivorId,
    /// The game's own answer, if any.
    pub vanilla: Option<Entity>,
    /// Who asks.
    pub purpose: Purpose,
    bed_of: Option<unsafe extern "system" fn(*mut c_void, u32) -> u32>,
    token: *mut c_void,
}

impl Lookup {
    /// The bed `entity` sleeps in, during a
    /// [`Purpose::PreviousOccupant`] or another lookup that can tell.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::ecs::Entity;
    /// use enshwick_loader_api::survivors::{Answer, Lookup, Purpose};
    ///
    /// fn previous(lookup: &Lookup, copies: &[Entity]) -> Answer {
    ///     match lookup.purpose {
    ///         Purpose::PreviousOccupant { bed, .. } => copies
    ///             .iter()
    ///             .find(|&&copy| lookup.bed_of(copy) == Some(bed))
    ///             .map_or(Answer::Nobody, |&copy| Answer::Entity(copy)),
    ///         _ => Answer::Vanilla,
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn bed_of(&self, entity: Entity) -> Option<Entity> {
        let bed_of = self.bed_of?;
        // SAFETY: the loader's function and token, valid for this call.
        let bed = unsafe { bed_of(self.token, entity.0) };
        (bed != 0).then_some(Entity(bed))
    }
}

/// The answer to a [`Lookup`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Answer {
    /// The game's own answer.
    Vanilla,
    /// This entity.
    Entity(Entity),
    /// No entity.
    Nobody,
}

/// A survivor's bed the game kept, from [`Load::on_survivor_bed_check`].
///
/// # Examples
///
/// See [`Load::on_survivor_bed_check`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BedCheck {
    /// The survivor.
    pub survivor: SurvivorId,
    /// Its entity.
    pub entity: Entity,
    /// The bed.
    pub bed: Entity,
    /// Where the survivor belongs: its saved spawn cell, or where it stands.
    pub home: [i64; 3],
    /// Where the bed stands.
    pub bed_at: [i64; 3],
}

/// A summoning staff's use, from [`Load::on_summon`].
///
/// # Examples
///
/// See [`Load::on_summon`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summon {
    /// The item used.
    pub item: u32,
    /// The ammunition used, 0 for none.
    pub ammo: u32,
    /// The template it places.
    pub template: Guid,
    /// The target cell.
    pub target: [i64; 3],
}

unsafe extern "system" fn run_lookup<F>(
    call: *const RawLookup,
    user: *mut c_void,
    out: *mut u32,
) -> u8
where
    F: Fn(&Lookup) -> Answer,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return 0;
    };
    let entity = Entity(call.entity);
    let purpose = match call.purpose {
        RawLookup::UNIQUENESS => Purpose::Uniqueness {
            entity,
            home: call.cell,
        },
        RawLookup::SPAWN => Purpose::Spawn { target: call.cell },
        RawLookup::BED_OWNER => Purpose::BedOwner { bed_at: call.cell },
        RawLookup::ASSIGN_BED => Purpose::AssignBed {
            bed: entity,
            bed_at: call.cell,
        },
        RawLookup::PREVIOUS_OCCUPANT => Purpose::PreviousOccupant {
            bed: entity,
            bed_at: call.cell,
        },
        RawLookup::ROAMING => Purpose::Roaming {
            entity,
            at: call.cell,
        },
        RawLookup::BED_BINDING => Purpose::BedBinding,
        RawLookup::CURSOR => Purpose::Cursor,
        _ => return 0,
    };
    let lookup = Lookup {
        survivor: SurvivorId(call.survivor),
        vanilla: (call.vanilla != 0).then_some(Entity(call.vanilla)),
        purpose,
        bed_of: call.bed_of,
        token: call.token,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let answer = crate::host::guard(
        "panic in a survivor lookup",
        || Answer::Vanilla,
        || handler(&lookup),
    );
    let value = match answer {
        Answer::Vanilla => return 0,
        Answer::Entity(entity) => entity.0,
        Answer::Nobody => 0,
    };
    if !out.is_null() {
        // SAFETY: the loader's out pointer.
        unsafe { out.write(value) };
    }
    1
}

unsafe extern "system" fn run_bed_check<F>(check: *const RawBedCheck, user: *mut c_void) -> u8
where
    F: Fn(&BedCheck) -> bool,
{
    // SAFETY: the loader passes a valid check for the duration of the call.
    let Some(check) = (unsafe { check.as_ref() }) else {
        return 1;
    };
    let call = BedCheck {
        survivor: SurvivorId(check.survivor),
        entity: Entity(check.entity),
        bed: Entity(check.bed),
        home: check.home,
        bed_at: check.bed_at,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let keep = crate::host::guard("panic in a survivor bed check", || true, || handler(&call));
    u8::from(keep)
}

unsafe extern "system" fn run_summon<F>(summon: *const RawSummon, user: *mut c_void) -> u8
where
    F: Fn(&Summon) -> bool,
{
    // SAFETY: the loader passes a valid summon for the duration of the call.
    let Some(summon) = (unsafe { summon.as_ref() }) else {
        return 0;
    };
    let call = Summon {
        item: summon.item,
        ammo: summon.ammo,
        template: Guid(summon.template),
        target: summon.target,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let consume = crate::host::guard("panic in a summon filter", || false, || handler(&call));
    u8::from(consume)
}

unsafe extern "system" fn run_status<F>(
    survivor: u32,
    statuses: *const u8,
    count: usize,
    user: *mut c_void,
) -> u32
where
    F: Fn(SurvivorId, &[u8]) -> Option<u8>,
{
    let statuses = if statuses.is_null() || count == 0 {
        &[][..]
    } else {
        // SAFETY: `count` bytes at `statuses`, valid for the call.
        unsafe { core::slice::from_raw_parts(statuses, count) }
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let shown = crate::host::guard(
        "panic in a survivor status merge",
        || None,
        || handler(SurvivorId(survivor), statuses),
    );
    shown.map_or(0, |byte| 0x100 | u32::from(byte))
}

type Register<R> = extern "system" fn(crate::abi::ModHandle, R, *mut c_void) -> crate::abi::Status;

impl Load<'_> {
    fn register<F, R>(
        &mut self,
        field: usize,
        what: &str,
        pick: fn(&HostApi) -> Register<R>,
        run: R,
        handler: F,
    ) -> Result<(), Off> {
        let host = self.host();
        crate::load::offered(host.api(), field, what)?;
        let register = pick(host.api());
        crate::load::hand_over(handler, |user| register(host.handle(), run, user), what)
    }

    /// Makes `handler` the answer to the game's survivor lookups; one mod at
    /// a time. It runs on the server's worker threads, in the dedicated
    /// server or a hosting client, and for [`Purpose::Cursor`] on a client
    /// alone too: keep it quick and never block.
    ///
    /// Several engine systems look survivors up for every survivor on every
    /// tick, so `handler` runs many times a frame: answer from state read
    /// without contention, such as a read lock or an atomic snapshot, and
    /// never allocate on the common path.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, when another mod answers already, on a build that
    /// lacks a piece, or with a loader older than the API.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_survivor_lookup<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&Lookup) -> Answer + Send + Sync + 'static,
    {
        self.register(
            core::mem::offset_of!(HostApi, survivor_lookup),
            "the survivor lookup",
            |api| api.survivor_lookup,
            run_lookup::<F> as crate::abi::SurvivorLookupFn,
            handler,
        )
    }

    /// Runs `filter` after a survivor's own check kept its bed; returning
    /// `false` makes the game clear the bed. Server's worker threads, for
    /// every survivor with a bed on every tick: keep it quick and never
    /// block.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_survivor_lookup`], without the one-mod rule.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     // Survivors keep beds within 100 m of home.
    ///     load.on_survivor_bed_check(|check| (0..3).all(|a| (check.home[a] - check.bed_at[a]).abs() < 200))
    /// }
    /// ```
    pub fn on_survivor_bed_check<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&BedCheck) -> bool + Send + Sync + 'static,
    {
        self.register(
            core::mem::offset_of!(HostApi, survivor_bed_check),
            "the survivor bed check",
            |api| api.survivor_bed_check,
            run_bed_check::<F> as crate::abi::BedCheckFn,
            filter,
        )
    }

    /// Runs `filter` before a summoning staff spawns or moves a survivor;
    /// returning `true` makes the use take the item, in the same transaction
    /// and only when the spawn or move works. Server's game thread.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_survivor_bed_check`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     const MY_STAFF: u32 = 0x5e01;
    ///     load.on_summon(|summon| summon.item == MY_STAFF && summon.ammo == 0)
    /// }
    /// ```
    pub fn on_summon<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Summon) -> bool + Send + Sync + 'static,
    {
        self.register(
            core::mem::offset_of!(HostApi, summon_filter),
            "the summon filter",
            |api| api.summon_filter,
            run_summon::<F> as crate::abi::SummonFn,
            filter,
        )
    }

    /// Files item `item` as a summoning staff of `survivor`, so the game
    /// treats it as that survivor's staff on both sides: the cursor, the
    /// unlock check and the placement.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, on a build that lacks the registry, or with a
    /// loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::survivors::SurvivorId;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.add_summon_item(0x5e01, SurvivorId(1_201_801_546))
    /// }
    /// ```
    pub fn add_summon_item(&mut self, item: u32, survivor: SurvivorId) -> Result<(), Off> {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, summon_item),
            "summon items",
        )?;
        let status = (host.api().summon_item)(host.handle(), item, survivor.0);
        if status.is_ok() {
            Ok(())
        } else {
            Err(Off::new(format!(
                "summon item {item:#x} refused by the loader (status {})",
                status.0
            )))
        }
    }

    /// Makes `merge` decide what the client's survivor menu shows for a
    /// survivor with several entities: it gets the status byte of each and
    /// returns the byte to show, or `None` for the game's. Client only; one
    /// mod at a time. It runs every frame while the menu shows, once per such
    /// survivor: it must not allocate or lock.
    ///
    /// # Errors
    ///
    /// [`Off`] on the server, and as for [`Load::on_survivor_lookup`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     // Show only the bits every entity has.
    ///     load.on_survivor_status(|_, statuses| {
    ///         (statuses.len() >= 2).then(|| statuses.iter().fold(u8::MAX, |all, s| all & s))
    ///     })
    /// }
    /// ```
    pub fn on_survivor_status<F>(&mut self, merge: F) -> Result<(), Off>
    where
        F: Fn(SurvivorId, &[u8]) -> Option<u8> + Send + Sync + 'static,
    {
        self.register(
            core::mem::offset_of!(HostApi, survivor_status),
            "the survivor menu merge",
            |api| api.survivor_status,
            run_status::<F> as crate::abi::SurvivorStatusFn,
            merge,
        )
    }
}
