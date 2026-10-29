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

/// The most entries [`holds`] reads, far past any ban list a server keeps.
const LIST_MAX: u64 = 1 << 20;
/// The largest ban entry the loader builds, far past the game's 0xd0 bytes
/// on server 399 and client 426.
const ENTRY_MAX: usize = 0x1000; // bound, not a game offset: a sanity bound

/// The game's add: the config object that holds the list, and an entry it
/// copies. It takes the list's lock, leaves an account already listed
/// alone, and marks the config changed (server 399, client 426).
type AddFn = unsafe extern "system" fn(config: *mut u8, entry: *const u64) -> u8;

/// The add, the list's place and the entry's layout on this build.
#[derive(Debug)]
pub(super) struct BanList {
    add: AddFn,
    /// The game's ban entry in its live list, which the add copies. The
    /// reflected `keen::g38_config::AccountBan` is the file's record with
    /// other strings, not this layout.
    entry: BanFields,
    /// Where the config keeps its list of live entries, a pointer, and their
    /// count, a `u64`. Only the game thread changes the count.
    entries: usize,
    count: usize,
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
    let offset = |id: pinned::Constant| {
        loader
            .offset(id)
            .ok_or_else(|| format!("the table entry {}", id.id()))
    };
    let config = offset(pinned::server::CONFIG)?;
    let changed = offset(pinned::server::BAN_LIST_CHANGED)?;
    let entries = offset(pinned::server::BAN_LIST)?;
    let count = offset(pinned::server::BAN_COUNT)?;
    let entry = BanFields {
        size: offset(pinned::server::BAN_ENTRY_SIZE)?,
        account: offset(pinned::server::BAN_ENTRY_ACCOUNT)?,
        display_name: offset(pinned::server::BAN_ENTRY_DISPLAY_NAME)?,
        character_name: offset(pinned::server::BAN_ENTRY_CHARACTER_NAME)?,
        date: offset(pinned::server::BAN_ENTRY_DATE)?,
        name_text: offset(pinned::server::BAN_NAME_TEXT)?,
        name_length: offset(pinned::server::BAN_NAME_LENGTH)?,
    };
    if entry.size > ENTRY_MAX || ban_entry(&entry, 1, 0, 0).is_none() {
        return Err(format!("a ban entry its fields fit in ({entry:x?})"));
    }
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
    Ok(BanList {
        // SAFETY: the table's `server.ban_add` is the game's ban list add on
        // this build, its code checked against the table's pattern by
        // `code_holds` above (servers 399, client 426); its signature is
        // `AddFn`.
        add: unsafe { core::mem::transmute::<usize, AddFn>(address) },
        entry,
        entries,
        count,
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
/// entry's layout, or no config is set up; [`Status::REFUSED`] when the
/// game's list did not grow; [`Status::WRONG_THREAD`] off the game thread.
/// An account on the list already is no error.
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
    if holds(loader, account) == Some(true) {
        super::remember_offline_ban(account);
        crate::log!(INFO, "account {account:016x} is on the ban list already");
        return Ok(());
    }
    let date = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    let empty = u64::try_from(EMPTY.as_ptr().expose_provenance()).map_err(|_| Status::MISSING)?;
    let entry = ban_entry(&list.entry, account, date, empty).ok_or(Status::MISSING)?;
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
        return Err(Status::REFUSED);
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
    // SAFETY: the non-null config holds its list's entry pointer at the
    // table's `server.ban_list`, which the add reads; only the game thread,
    // this one, changes it.
    let entries: usize = unsafe { crate::memory::read(config, list.entries) };
    // SAFETY: the non-null config holds its list's `u64` count at the table's
    // `server.ban_count`, which the add reads, on the game thread, which
    // alone changes it.
    let count: u64 = unsafe { crate::memory::read(config, list.count) };
    if entries == 0 || count > LIST_MAX {
        return (count == 0).then_some(false);
    }
    let count = usize::try_from(count).ok()?;
    Some((0..count).any(|i| {
        // SAFETY: entry `i` of `count` lies at `i * size` in the list the
        // config owns, each with its `u64` account at the table's
        // `server.ban_entry_account`; the size is the stride the add steps
        // by, `server.ban_entry_size`, inside `ENTRY_MAX`.
        let key: u64 =
            unsafe { crate::memory::read(entries, i * list.entry.size + list.entry.account) };
        key == account
    }))
}
