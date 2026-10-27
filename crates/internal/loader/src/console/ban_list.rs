//! The server's ban list, for an account that is not online: the game's
//! account action handler bans only players online, so the loader calls
//! the list's own add, which the handler uses, and the dedicated server
//! writes the list to `bannedAccounts` of `enshrouded_server.json`.

use std::time::{SystemTime, UNIX_EPOCH};

use enshwick::abi::Status;

use crate::host::Loader;
use crate::pinned;

use super::STATE;
use super::accounts::{BanFields, ban_entry};

/// The game's ban entry in its live list, which the add copies (server 399,
/// client 426, read in the add and its copy). The reflected
/// `keen::g38_config::AccountBan` is the file's record with other strings,
/// not this layout.
const LIVE_ENTRY: BanFields = BanFields {
    size: 0xd0,
    account: 0,
    display_name: 0x8,
    character_name: 0x68,
    date: 0xc8,
};

/// Where the config keeps its list of live entries: the pointer and the
/// count, a `u64` (server 399, client 426, read in the add). Only the game
/// thread changes the count.
const LIST_ENTRIES: usize = 0x38;
const LIST_COUNT: usize = 0x40;
/// The most entries [`holds`] reads, far past any ban list a server keeps.
const LIST_MAX: u64 = 1 << 20;

/// Where the add steps through its list by the entry's size, and the
/// instruction there, `add rax, 0xd0`: the check that this build's entry
/// is [`LIVE_ENTRY`]'s size (server 399, client 426).
const ADD_STRIDE_AT: usize = 0x4c;
const ADD_STRIDE: [u8; 6] = [0x48, 0x05, 0xd0, 0x00, 0x00, 0x00];

/// The game's add: the config object that holds the list, and an entry it
/// copies. It takes the list's lock, leaves an account already listed
/// alone, and marks the config changed (server 399, client 426).
type AddFn = unsafe extern "system" fn(config: *mut u8, entry: *const u64) -> u8;

/// The add and the entry's layout on this build.
#[derive(Debug)]
pub(super) struct BanList {
    add: AddFn,
    /// Where the server keeps its config object, a pointer.
    config: usize,
    /// Where the server keeps its flag that sends the changed list to the
    /// administrators online at the next step, a byte.
    changed: usize,
}

/// The ban list on this build, or what this build lacks, logged once.
fn ban_list(loader: &Loader) -> Option<&'static BanList> {
    STATE
        .ban_list
        .get_or_init(|| {
            let found = find(loader);
            if let Err(lacks) = &found {
                crate::log!(
                    WARN,
                    "bans of accounts not online off: {} lacks {lacks}",
                    loader.build.name
                );
            }
            found
        })
        .as_ref()
        .ok()
}

/// The add, the config's place and the entry's layout, or what is missing.
fn find(loader: &Loader) -> Result<BanList, String> {
    let config = loader
        .offset(pinned::server::CONFIG)
        .ok_or_else(|| format!("the table entry {}", pinned::server::CONFIG.id()))?;
    let changed = loader
        .offset(pinned::server::BAN_LIST_CHANGED)
        .ok_or_else(|| format!("the table entry {}", pinned::server::BAN_LIST_CHANGED.id()))?;
    let address = loader
        .address(pinned::server::BAN_ADD)
        .filter(|_| loader.code_holds(pinned::server::BAN_ADD))
        .and_then(|address| usize::try_from(address).ok())
        .ok_or_else(|| {
            format!(
                "the table entry {} with code that matches it",
                pinned::server::BAN_ADD.id()
            )
        })?;
    let mut stride = [0u8; ADD_STRIDE.len()];
    // SAFETY: `address` is the table's `server.ban_add`, its head checked by
    // `code_holds` above; the function runs well past `ADD_STRIDE_AT` plus
    // six bytes on every build its pattern matched, inside the game's
    // image, which stays mapped and readable.
    unsafe { crate::memory::read_bytes(address, ADD_STRIDE_AT, &mut stride) };
    if stride != ADD_STRIDE {
        return Err(format!(
            "the ban entry size its add steps by ({stride:02x?} at +{ADD_STRIDE_AT:#x})"
        ));
    }
    Ok(BanList {
        // SAFETY: the table's `server.ban_add` is the game's ban list add on
        // this build, its code checked against the table's pattern by
        // `code_holds` above (servers 399, client 426); its signature is
        // `AddFn`.
        add: unsafe { core::mem::transmute::<usize, AddFn>(address) },
        config,
        changed,
    })
}

/// Puts `account` on the server's ban list, with today's date and no
/// names, on the game thread.
///
/// # Errors
///
/// [`Status::MISSING`] when this build lacks the add, the config or the
/// entry's layout, no config is set up, or the game's list did not grow; [`Status::WRONG_THREAD`] off
/// the game thread.
pub(crate) fn add(loader: &Loader, account: u64) -> Result<(), Status> {
    const EMPTY: &[u8; 1] = b"\0";
    let list = ban_list(loader).ok_or(Status::MISSING)?;
    let server = crate::world::server()?;
    // SAFETY: `crate::world::server` returned the live server on its game
    // thread; it keeps its config object's pointer at the table's
    // `server.config`.
    let config: usize = unsafe { crate::memory::read(server, list.config) };
    if config == 0 {
        return Err(Status::MISSING);
    }
    let date = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    let empty = u64::try_from(EMPTY.as_ptr().expose_provenance()).map_err(|_| Status::MISSING)?;
    let entry = ban_entry(&LIVE_ENTRY, account, date, empty).ok_or(Status::MISSING)?;
    // SAFETY: on the game thread with the live server's non-null config
    // object (checked above), the one the game's own handler passes; the
    // add takes the config's lock itself and only reads `entry`, which
    // holds the live entry's size, aligned to 8, with both names empty
    // and pointing at a live byte, and outlives the call.
    let listed = unsafe {
        (list.add)(
            core::ptr::with_exposed_provenance_mut(config),
            entry.as_ptr(),
        )
    };
    if listed == 0 {
        crate::log!(
            WARN,
            "account {account:016x} not put on the ban list: the game's list did not grow"
        );
        return Err(Status::MISSING);
    }
    // SAFETY: the live server on its game thread, which alone writes this
    // flag; the game's own handler sets the byte at the table's
    // `server.ban_list_changed` to 1 after a ban the same way.
    unsafe { crate::memory::write(server, list.changed, 1u8) };
    super::remember_offline_ban(account);
    crate::log!(INFO, "account {account:016x} put on the ban list");
    Ok(())
}

/// Whether the game's ban list holds `account` now, on the game thread;
/// `None` when it does not read.
pub(crate) fn holds(loader: &Loader, account: u64) -> Option<bool> {
    let list = ban_list(loader)?;
    let server = crate::world::reader().ok()?;
    // SAFETY: as in `add`: the live server on its game thread, with its
    // config object's pointer at the table's `server.config`.
    let config: usize = unsafe { crate::memory::read(server, list.config) };
    if config == 0 {
        return None;
    }
    // SAFETY: the non-null config holds its list's entry pointer and count at
    // `LIST_ENTRIES` and `LIST_COUNT` (server 399, client 426); only the game
    // thread, this one, changes them.
    let entries: usize = unsafe { crate::memory::read(config, LIST_ENTRIES) };
    // SAFETY: as for `entries`.
    let count: u64 = unsafe { crate::memory::read(config, LIST_COUNT) };
    if entries == 0 || count > LIST_MAX {
        return (count == 0).then_some(false);
    }
    let count = usize::try_from(count).ok()?;
    Some((0..count).any(|i| {
        // SAFETY: entry `i` of `count` lies at `i * size` in the list the
        // config owns, each starting with its `u64` account (`LIVE_ENTRY`,
        // whose size the add's stride check confirmed on this build).
        let key: u64 =
            unsafe { crate::memory::read(entries, i * LIVE_ENTRY.size + LIVE_ENTRY.account) };
        key == account
    }))
}
