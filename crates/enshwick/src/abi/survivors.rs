//! Survivors: lookups, beds, summons and their status.

use core::ffi::c_void;

use crate::abi::Header;

/// A game system asking which entity is a survivor, as
/// [`HostApi::survivor_lookup`](super::HostApi::survivor_lookup) hands it to the mod that answers.
///
/// The game keeps one entity per survivor; a mod that keeps several answers
/// each asker with the one that suits it. Cells are
/// whole 0.5 m cells.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, SurvivorLookup};
///
/// let lookup = SurvivorLookup {
///     header: Header::of::<SurvivorLookup>(),
///     survivor: 0x47a1_5e4a,
///     vanilla: 4096,
///     purpose: SurvivorLookup::SPAWN,
///     entity: 0,
///     cell: [10, 0, 10],
///     bed_of: None,
///     token: core::ptr::null_mut(),
/// };
/// assert_eq!(lookup.purpose, SurvivorLookup::SPAWN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct SurvivorLookup {
    /// Size and version of this struct.
    pub header: Header,
    /// The survivor's id.
    pub survivor: u32,
    /// The game's own answer, 0 for none.
    pub vanilla: u32,
    /// Who asks; one of the constants on this type.
    pub purpose: u32,
    /// The entity the asker works on, as its purpose says; 0 for none.
    pub entity: u32,
    /// The cell its purpose names.
    pub cell: [i64; 3],
    /// The bed an entity sleeps in, 0 for none; valid during the call with
    /// `token`.
    pub bed_of: Option<unsafe extern "system" fn(token: *mut c_void, entity: u32) -> u32>,
    /// The argument of `bed_of`.
    pub token: *mut c_void,
}

impl_headed!(SurvivorLookup);

impl SurvivorLookup {
    /// The pass that keeps one entity per survivor: `entity` is the one it
    /// looks at, `cell` its home. Answer `entity` to keep it, or another
    /// entity, which makes the game remove `entity`.
    pub const UNIQUENESS: u32 = 1;
    /// A summoning staff used at `cell`: answer the entity to move there, or
    /// 0 to spawn a new one.
    pub const SPAWN: u32 = 2;
    /// A bed at `cell` binding its sleeper: answer the entity that may sleep
    /// there, 0 for none.
    pub const BED_OWNER: u32 = 3;
    /// A player giving bed `entity` at `cell` to the survivor with its staff:
    /// answer the entity that gets it.
    pub const ASSIGN_BED: u32 = 4;
    /// The same assignment asking who slept in bed `entity` at `cell` before:
    /// answer that entity, 0 for none.
    pub const PREVIOUS_OCCUPANT: u32 = 5;
    /// A survivor's idle behaviour: `entity` is the survivor, `cell` where it
    /// stands; answer the entity whose base it keeps to.
    pub const ROAMING: u32 = 6;
    /// The first binding of beds after a load, before beds bind their
    /// sleepers.
    pub const BED_BINDING: u32 = 7;
    /// The cursor measuring how far a bed lies from the survivor before it
    /// offers to assign it; 0 skips the measure.
    pub const CURSOR: u32 = 8;
}

/// The mod's survivor lookup: the call and the pointer given at
/// registration; returns 1 with the answer in `out`, or 0 for the game's own.
pub type SurvivorLookupFn =
    unsafe extern "system" fn(call: *const SurvivorLookup, user: *mut c_void, out: *mut u32) -> u8;

/// A survivor's own check that its bed is still its, after the game kept it,
/// as [`HostApi::survivor_bed_check`](super::HostApi::survivor_bed_check) hands it on. Cells are whole 0.5 m
/// cells.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BedCheck, Header};
///
/// let check = BedCheck { header: Header::of::<BedCheck>(), survivor: 7, entity: 4096, bed: 4100, home: [0; 3], bed_at: [4, 0, 4] };
/// assert_eq!(check.bed, 4100);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct BedCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The survivor's id.
    pub survivor: u32,
    /// The survivor's entity.
    pub entity: u32,
    /// The bed.
    pub bed: u32,
    /// Where the survivor belongs: its saved spawn cell, or where it stands.
    pub home: [i64; 3],
    /// Where the bed stands.
    pub bed_at: [i64; 3],
}

impl_headed!(BedCheck);

/// A filter of survivors' beds: returns 0 to make the game clear the bed.
pub type BedCheckFn = unsafe extern "system" fn(check: *const BedCheck, user: *mut c_void) -> u8;

/// A summoning staff's use about to spawn or move a survivor, as
/// [`HostApi::summon_filter`](super::HostApi::summon_filter) hands it on, and once it worked, as
/// [`HostApi::summoned`](super::HostApi::summoned) does.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Summon};
///
/// let summon = Summon {
///     header: Header::of::<Summon>(),
///     item: 0x5e01,
///     ammo: 0,
///     template: [0; 16],
///     target: [0; 3],
///     slot: 0x3_0000_0041,
///     pide: 900,
///     meter: [2, 3],
///     consumed: 0,
///     reserved: [0; 3],
/// };
/// assert_eq!(summon.meter, [2, 3]);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct Summon {
    /// Size and version of this struct.
    pub header: Header,
    /// The item used.
    pub item: u32,
    /// The ammunition it used, 0 for none.
    pub ammo: u32,
    /// The template GUID it places, in file order.
    pub template: [u8; 16],
    /// The target cell.
    pub target: [i64; 3],
    /// The slot the used item lies in, `sub-inventory | index << 32`.
    /// Since minor 37.
    pub slot: u64,
    /// The used item's own entity, 0 for an item without one. Since minor
    /// 37.
    pub pide: u32,
    /// The used item's meter as clients see it, current and maximum;
    /// `[0, 0]` for an item without one. Since minor 37.
    pub meter: [u16; 2],
    /// After the use: 1 when it took the item. Since minor 37.
    pub consumed: u8,
    /// Zero.
    pub reserved: [u8; 3],
}

impl_headed!(Summon);

impl Summon {
    /// A filter's answer: the use takes the item.
    pub const TAKE: u8 = 1;
    /// A filter's answer: the use is refused; nothing spawns, moves or is
    /// taken. Since minor 42.
    pub const REFUSE: u8 = 2;
}

/// A summon filter: returns [`Summon::TAKE`] to make the game take the
/// item in the same transaction, and only when the spawn or move works,
/// [`Summon::REFUSE`] to refuse the use, or 0 to leave it.
pub type SummonFn = unsafe extern "system" fn(summon: *const Summon, user: *mut c_void) -> u8;

/// The client's survivor menu merging the statuses of every entity of one
/// survivor: the survivor, `count` status bytes at `statuses`, valid for the
/// call, and the pointer given at registration; returns
/// [`SURVIVOR_STATUS_SET`] or-ed with the byte to show, or 0 to leave the
/// game's.
pub type SurvivorStatusFn = unsafe extern "system" fn(
    survivor: u32,
    statuses: *const u8,
    count: usize,
    user: *mut c_void,
) -> u32;

/// The bit of a [`SurvivorStatusFn`] answer that says its low byte is the
/// status to show.
pub const SURVIVOR_STATUS_SET: u32 = 0x100;
