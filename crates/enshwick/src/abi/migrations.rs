//! Migrations of a mod's settings and stores.

use core::ffi::c_void;

use crate::abi::{Header, Status, Str};

/// One step of a mod migration, as [`HostApi::migrate`](super::HostApi::migrate) takes it.
///
/// Frozen: written as an array.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{MigrationStep, Str};
///
/// let step = MigrationStep {
///     kind: MigrationStep::RENAME_SETTING,
///     from: Str::new("cost"),
///     to: Str::new("cost_multiplier"),
///     from_guid: [0; 16],
///     to_guid: [0; 16],
/// };
/// assert_eq!(step.kind, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MigrationStep {
    /// What the step does: one of the constants below.
    pub kind: u32,
    /// The setting or store the step reads, or renames from.
    pub from: Str,
    /// The setting or store a rename writes to; empty otherwise.
    pub to: Str,
    /// The GUID content had, for [`MigrationStep::RENAME_CONTENT`].
    pub from_guid: [u8; 16],
    /// The GUID content has now, for [`MigrationStep::RENAME_CONTENT`].
    pub to_guid: [u8; 16],
}

impl MigrationStep {
    /// Renames the setting `from` to `to`, value and comments kept.
    pub const RENAME_SETTING: u32 = 1;
    /// Changes the value of the setting `from` through the mod's
    /// [`MigrateFn`], which gets and returns a TOML value.
    pub const MAP_SETTING: u32 = 2;
    /// Removes the setting `from`.
    pub const REMOVE_SETTING: u32 = 3;
    /// Renames the world store `from` to `to`.
    pub const RENAME_STORE: u32 = 4;
    /// Changes the world store `from` through the mod's [`MigrateFn`], which
    /// gets and returns the whole TOML file.
    pub const MAP_STORE: u32 = 5;
    /// Reads content saved under `from_guid` as the content now under
    /// `to_guid`.
    pub const RENAME_CONTENT: u32 = 6;
    /// Sets the setting `to` from the setting another mod keeps, `from`
    /// written `<mod id>:<key>`, in its settings file and in each world's
    /// copy; nothing where that mod has no such value.
    pub const TAKE_SETTING: u32 = 7;
}

/// One mod migration: an id and its steps, in order.
///
/// Frozen: written as an array.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Migration, Str};
///
/// let migration = Migration { id: Str::new("2"), steps: core::ptr::null(), count: 0 };
/// assert_eq!(migration.count, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Migration {
    /// The migration's id, unique in the mod; the record keeps it.
    pub id: Str,
    /// Its steps.
    pub steps: *const MigrationStep,
    /// How many `steps` points at.
    pub count: u32,
}

/// Changes a value for a migration step.
///
/// `migration` and `step` index the set, `input` is a TOML value
/// ([`MigrationStep::MAP_SETTING`]) or a whole TOML file
/// ([`MigrationStep::MAP_STORE`]), and the answer goes to `out`, valid until
/// the next call; an error status with the reason in `out` fails the migration.
///
/// # Examples
///
/// See [`MigrationSet`], whose `map` has its signature.
pub type MigrateFn = unsafe extern "system" fn(
    user: *mut c_void,
    migration: u32,
    step: u32,
    input: Str,
    out: *mut Str,
) -> Status;

/// Every migration of a mod, oldest first, for [`HostApi::migrate`](super::HostApi::migrate).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, MigrationSet};
///
/// assert_eq!(Header::of::<MigrationSet>().size as usize, size_of::<MigrationSet>());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct MigrationSet {
    /// Size and version of this struct.
    pub header: Header,
    /// The migrations, oldest first.
    pub migrations: *const Migration,
    /// How many `migrations` points at.
    pub count: u32,
    /// Changes values for the steps that map them, a [`MigrateFn`]; the
    /// loader refuses a set without it. Spelled out so C sees a nullable
    /// function pointer.
    pub map: Option<
        unsafe extern "system" fn(
            user: *mut c_void,
            migration: u32,
            step: u32,
            input: Str,
            out: *mut Str,
        ) -> Status,
    >,
    /// The pointer `map` takes.
    pub user: *mut c_void,
}

impl_headed!(MigrationSet);
