//! The fake's answers about the mods: logs, faults, settings and what is loaded.

use core::ffi::c_void;

use crate::abi::{ConfigCheckFn, Level, MigrationSet, ModHandle, ModInfo, Status, Str};
use crate::test::table::{found, put, text, with};

pub(crate) extern "system" fn fault(_me: ModHandle, reason: Str) {
    let reason = text(reason);
    // The loader logs every turn-off with its reason.
    let line = format!("off: {}", reason.replace('\t', " "));
    with((), |s| {
        s.faults.push(reason);
        s.logs.push((Level::ERROR, line));
    });
}

pub(crate) extern "system" fn log(_me: ModHandle, level: Level, message: Str) {
    let message = text(message);
    with((), |s| s.logs.push((level, message)));
}

pub(crate) extern "system" fn settings(_me: ModHandle, defaults: Str, out: *mut Str) -> Status {
    let defaults = text(defaults);
    let served = with(None, |s| {
        let served: Box<str> = s
            .host
            .settings
            .clone()
            .unwrap_or_else(|| defaults.clone())
            .into();
        s.defaults.push(defaults);
        // The heap text stays where it is while the box lives in the state.
        let view = Str::new(&served);
        s.settings.push(served);
        Some(view)
    });
    found(served, out)
}

pub(crate) extern "system" fn migrate(_me: ModHandle, set: *const MigrationSet) -> Status {
    // SAFETY: the safe layer passes a valid set.
    let Some(set) = (unsafe { set.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    let Some(map) = set.map else {
        return Status::BAD_ARGS;
    };
    let mut migrations = Vec::new();
    for index in 0..set.count as usize {
        // SAFETY: the set holds `count` migrations.
        let migration = unsafe { set.migrations.add(index) };
        // SAFETY: `index` is below the set's `count`, so `migration` points at
        // one of its live, aligned entries.
        let migration = unsafe { &*migration };
        let steps = (0..migration.count as usize)
            .map(|i| {
                // SAFETY: the migration holds `count` steps.
                let step = unsafe { migration.steps.add(i) };
                // SAFETY: `i` is below the migration's `count`, so `step`
                // points at one of its live, aligned steps.
                let step = unsafe { &*step };
                (step.kind, text(step.from), text(step.to))
            })
            .collect();
        migrations.push((text(migration.id), steps));
    }
    with(Status::MISSING, |s| {
        if s.migrations.is_some() {
            return Status::REFUSED;
        }
        s.migrations = Some(crate::test::FakeMigrations {
            list: migrations,
            map,
            user: set.user.expose_provenance(),
        });
        Status::OK
    })
}

pub(crate) extern "system" fn mods(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.mods.clone());
    list(&mods, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn join_missing(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.join_missing.clone());
    list(&mods, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn replaced(
    _me: ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mods = with(Vec::new(), |s| s.host.replaced.clone());
    list(&mods, out, entry_size, capacity, count)
}

/// Copies up to `capacity` of `items` to `out`, `entry_size` bytes apart, and
/// writes how many there are; refuses a non-null `out` whose entries are
/// shorter than a header, as the loader does.
pub(crate) fn list<T: Copy>(
    items: &[T],
    out: *mut T,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let stride = usize::try_from(entry_size).unwrap_or(0);
    let shared = stride.min(size_of::<T>());
    let capacity = usize::try_from(capacity).unwrap_or(0);
    if !out.is_null() {
        if stride < size_of::<crate::abi::Header>() {
            return Status::BAD_ARGS;
        }
        let size = u32::try_from(shared).unwrap_or(u32::MAX);
        for (i, info) in items.iter().take(capacity).enumerate() {
            let entry = out.cast::<u8>().wrapping_add(i * stride);
            // SAFETY: the safe layer passes room for `capacity` entries of
            // `stride` bytes; the fake fills what both sides know.
            unsafe {
                core::ptr::copy_nonoverlapping(core::ptr::from_ref(info).cast(), entry, shared);
            }
            // SAFETY: the header's `size` is the entry's first field, inside
            // its `stride` bytes, at least a header as checked above.
            unsafe { entry.cast::<u32>().write_unaligned(size) };
        }
    }
    put(count, u32::try_from(items.len()).unwrap_or(u32::MAX))
}

pub(crate) extern "system" fn sync(_me: ModHandle, synced: Str, out: *mut Str) -> Status {
    let synced = text(synced);
    let served = with(Str::EMPTY, |s| {
        s.synced.push(synced);
        let served: Box<str> = s.host.server_synced.clone().unwrap_or_default().into();
        // The heap text stays where it is while the box lives in the state.
        let view = Str::new(&served);
        s.sync_out.push(served);
        view
    });
    put(out, served)
}

pub(crate) extern "system" fn config_schema(_me: ModHandle, schema: Str) -> Status {
    // SAFETY: the mod passes text valid for the call.
    let Some(text) = (unsafe { schema.as_str() }) else {
        return Status::BAD_ARGS;
    };
    if crate::config::Schema::parse(text).is_err() {
        return Status::BAD_ARGS;
    }
    with(Status::REFUSED, |s| {
        s.config_schema = Some(text.to_owned());
        Status::OK
    })
}

pub(crate) extern "system" fn config_check(
    _me: ModHandle,
    callback: ConfigCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.config_check.is_some() {
            return Status::REFUSED;
        }
        s.config_check = Some((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn texts(_me: ModHandle, toml: Str) -> Status {
    let toml = text(toml);
    with(Status::MISSING, |s| {
        s.published_texts = Some(toml);
        Status::OK
    })
}

pub(crate) extern "system" fn language(_me: ModHandle) -> u32 {
    with(0, |s| s.host.language)
}
