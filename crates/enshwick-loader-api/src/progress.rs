//! Character progress: skill node checks, the comfort furniture gives a base,
//! and the experience the server applies each tick.
//!
//! # Examples
//!
//! ```
//! use enshwick_loader_api::progress::{SkillGate, AIRBORNE_ARCHITECT};
//!
//! fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
//!     // Fly mode for everyone, the skill tree untouched.
//!     load.on_skill(AIRBORNE_ARCHITECT, |check| (check.gate == SkillGate::FlyMode).then_some(true))
//! }
//! ```

use core::ffi::c_void;

use crate::abi::{
    Comfort as RawComfort, ExperiencePass, ExperienceRow, HostApi, SkillCheck as RawSkillCheck,
};
use crate::ecs::Entity;
use crate::{Load, Off};

/// One character's experience before the server applies it, from
/// [`Load::on_experience`]: a filter may change `gained`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::ecs::Entity;
/// use enshwick_loader_api::progress::Experience;
///
/// let row = Experience { entity: Entity(4096), level: 3, progress: 10, need: 100, gained: 5 };
/// assert_eq!(row.need - row.progress, 90);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Experience {
    /// The character.
    pub entity: Entity,
    /// Its level.
    pub level: u32,
    /// Experience inside the level.
    pub progress: u32,
    /// What the level needs.
    pub need: u32,
    /// Gained experience the game applies next, at most one level a tick.
    pub gained: u32,
}

// The loader's rows are read in place as `Experience`.
const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    assert!(size_of::<Experience>() == size_of::<ExperienceRow>());
    assert!(align_of::<Experience>() == align_of::<ExperienceRow>());
    assert!(offset_of!(Experience, entity) == offset_of!(ExperienceRow, entity));
    assert!(offset_of!(Experience, level) == offset_of!(ExperienceRow, level));
    assert!(offset_of!(Experience, progress) == offset_of!(ExperienceRow, progress));
    assert!(offset_of!(Experience, need) == offset_of!(ExperienceRow, need));
    assert!(offset_of!(Experience, gained) == offset_of!(ExperienceRow, gained));
};

/// The Airborne Architect node, which opens the building fly mode.
pub const AIRBORNE_ARCHITECT: u32 = 0x4475_e8dd;

/// Which game check asks about a skill node.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SkillGate {
    /// The building fly mode gate, on both sides.
    FlyMode,
    /// A check the platform does not name.
    Other,
}

/// The game asking whether a character unlocked a skill node.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillCheck {
    /// The node.
    pub node: u32,
    /// Who asks.
    pub gate: SkillGate,
    /// The game's own answer.
    pub unlocked: bool,
}

/// A comfort provider in a base registering its comfort; a base keeps the
/// best per category and a player's comfort is the sum.
///
/// # Examples
///
/// See [`Load::on_comfort`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comfort {
    /// The comfort category.
    pub category: u32,
    /// The comfort it registers, after the filters before this one.
    pub value: i32,
    /// Where the provider stands, in metres.
    pub position: [f32; 3],
}

/// The game's level table, during an experience pass.
///
/// # Examples
///
/// See [`Load::on_experience`].
#[derive(Debug)]
pub struct Levels {
    cap: u32,
    need_for: Option<unsafe extern "system" fn(*mut c_void, u32) -> u32>,
    token: *mut c_void,
}

impl Levels {
    /// The highest level.
    ///
    /// # Examples
    ///
    /// See [`Load::on_experience`].
    #[must_use]
    pub const fn cap(&self) -> u32 {
        self.cap
    }

    /// What `level` needs, 0 for level 0 or past the cap.
    ///
    /// # Examples
    ///
    /// See [`Load::on_experience`].
    #[must_use]
    pub fn need(&self, level: u32) -> u32 {
        match self.need_for {
            Some(need_for) if level != 0 && level <= self.cap => {
                // SAFETY: the loader's function and token, valid for this pass.
                unsafe { need_for(self.token, level) }
            }
            _ => 0,
        }
    }
}

unsafe extern "system" fn run_skill<F>(check: *const RawSkillCheck, user: *mut c_void) -> u8
where
    F: Fn(&SkillCheck) -> Option<bool>,
{
    // SAFETY: the loader passes a valid check for the duration of the call.
    let Some(check) = (unsafe { check.as_ref() }) else {
        return 0;
    };
    let call = SkillCheck {
        node: check.node,
        gate: if check.gate == RawSkillCheck::FLY_MODE {
            SkillGate::FlyMode
        } else {
            SkillGate::Other
        },
        unlocked: check.unlocked != 0,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let answer = crate::host::guard("panic in a skill override", || None, || handler(&call));
    match answer {
        None => RawSkillCheck::VANILLA,
        Some(true) => RawSkillCheck::UNLOCKED,
        Some(false) => RawSkillCheck::LOCKED,
    }
}

unsafe extern "system" fn run_comfort<F>(comfort: *const RawComfort, user: *mut c_void) -> i32
where
    F: Fn(&Comfort) -> i32,
{
    // SAFETY: the loader passes a valid registration for the call.
    let Some(comfort) = (unsafe { comfort.as_ref() }) else {
        return 0;
    };
    let call = Comfort {
        category: comfort.category,
        value: comfort.value,
        position: comfort.position,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    crate::host::guard(
        "panic in a comfort filter",
        || call.value,
        || handler(&call),
    )
}

unsafe extern "system" fn run_experience<F>(pass: *mut ExperiencePass, user: *mut c_void)
where
    F: Fn(&mut [Experience], &Levels),
{
    // SAFETY: the loader passes a valid pass for the duration of the call.
    let Some(pass) = (unsafe { pass.as_mut() }) else {
        return;
    };
    let rows = if pass.rows.is_null() || pass.row_count == 0 {
        &mut [][..]
    } else {
        // SAFETY: `row_count` writable rows, valid for the call; an
        // `Experience` has the layout of an `ExperienceRow`.
        unsafe { core::slice::from_raw_parts_mut(pass.rows.cast::<Experience>(), pass.row_count) }
    };
    let levels = Levels {
        cap: pass.level_cap,
        need_for: pass.need_for,
        token: pass.token,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    crate::host::guard(
        "panic in an experience filter",
        || (),
        || handler(rows, &levels),
    );
}

impl Load<'_> {
    /// Answers the game's checks of skill node `node`: `Some(true)` unlocks
    /// it for that check, `Some(false)` locks it, `None` keeps the game's
    /// answer. It runs on both sides, on the checking thread, and only for
    /// `node`.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, on a build that lacks the check, or with a loader
    /// older than the API.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn on_skill<F>(&mut self, node: u32, handler: F) -> Result<(), Off>
    where
        F: Fn(&SkillCheck) -> Option<bool> + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, skill_override),
            "skill overrides",
        )?;
        crate::load::hand_over(
            handler,
            |user| (host.api().skill_override)(host.handle(), node, run_skill::<F>, user),
            "the skill override",
        )
    }

    /// Changes the comfort each provider registers in a base: `filter`
    /// returns the value to register. Server's worker threads, on the hot
    /// path of every base: keep it to arithmetic.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_skill`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.on_comfort(|comfort| comfort.value.saturating_mul(2))
    /// }
    /// ```
    pub fn on_comfort<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Comfort) -> i32 + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, comfort_filter),
            "comfort filters",
        )?;
        crate::load::hand_over(
            filter,
            |user| (host.api().comfort_filter)(host.handle(), run_comfort::<F>, user),
            "the comfort filter",
        )
    }

    /// Runs `filter` on every spawned character's experience before the
    /// server applies it; changes to `gained` are written back and sent to
    /// the players. Server's worker threads, once per batch.
    ///
    /// The engine runs its experience system several times a tick, one batch
    /// of characters each, so `filter` runs on every tick, most often with
    /// nothing gained: return early when no row gained anything, and never
    /// allocate or lock on the way.
    ///
    /// # Errors
    ///
    /// As for [`Load::on_skill`].
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     // Double every gain, never past the end of the level.
    ///     load.on_experience(|rows, _levels| {
    ///         for row in rows {
    ///             let room = row.need.saturating_sub(row.progress);
    ///             row.gained = row.gained.saturating_mul(2).min(room.max(row.gained));
    ///         }
    ///     })
    /// }
    /// ```
    pub fn on_experience<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&mut [Experience], &Levels) + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::offered(
            host.api(),
            core::mem::offset_of!(HostApi, experience_filter),
            "experience filters",
        )?;
        crate::load::hand_over(
            filter,
            |user| (host.api().experience_filter)(host.handle(), run_experience::<F>, user),
            "the experience filter",
        )
    }
}
