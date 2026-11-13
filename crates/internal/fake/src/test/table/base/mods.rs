//! The fake's answers about the mods: their manifests, logs, faults,
//! settings and what is loaded.

use core::ffi::c_void;

use crate::test::table::{Registered, State, TABLE, found, put, text, with};
use enshwick_core::abi::{
    ConfigCheckFn, ContentRename, HostApi, Level, ModHandle, ModInfo, Side, Status, Str,
};

/// What the fake reads from the mod's manifest, as the loader does.
#[derive(Debug)]
pub(crate) struct ManifestFacts {
    /// The mod's id.
    pub(crate) id: String,
    /// Whether the mod runs on both sides, so it takes part in the join
    /// check.
    pub(crate) joins: bool,
    /// The ids of the mods it requires.
    pub(crate) requires: Vec<String>,
}

impl ManifestFacts {
    /// The facts of `manifest`.
    fn of(manifest: &enshwick_core::abi::Manifest) -> Self {
        let dependencies = if manifest.dependencies.is_null() {
            &[][..]
        } else {
            // SAFETY: a manifest points at `dependency_count` dependencies,
            // `declare_mod!`'s static array, alive for the process.
            unsafe { core::slice::from_raw_parts(manifest.dependencies, manifest.dependency_count) }
        };
        Self {
            id: text(manifest.id),
            joins: manifest.side == Side::BOTH,
            requires: dependencies
                .iter()
                .filter(|d| d.relation == enshwick_core::abi::Relation::REQUIRES)
                .map(|d| text(d.id))
                .collect(),
        }
    }

    /// Whether the `kind` `name`, `<id>:<what>`, is of another mod whose
    /// exports [`FakeHost::exports`](crate::test::FakeHost::exports) gave and
    /// lack `<kind>:<what>`, or for content and stats also the name it is a
    /// part of, which the loader refuses the mod.
    pub(crate) fn unexported(
        &self,
        kind: enshwick_core::schema_section::Kind,
        name: &str,
        exports: &rustc_hash::FxHashMap<String, Vec<String>>,
    ) -> bool {
        name.split_once(':').is_some_and(|(owner, what)| {
            owner != self.id
                && exports.get(owner).is_some_and(|names| {
                    !enshwick_core::__fake::rules::exports_name(names, kind, what)
                })
        })
    }

    /// Whether the mod requires the mod side of `name`, `<id>:<what>`.
    pub(crate) fn requires_owner_of(&self, name: &str) -> bool {
        name.split_once(':')
            .is_some_and(|(owner, _)| self.requires.iter().any(|id| id == owner))
    }
}

/// Logs the loader's warning for a mod refused the `kind` `id`,
/// `<owner>:<what>`, whose owner does not export it.
pub(crate) fn warn_unexported(s: &mut State, kind: enshwick_core::schema_section::Kind, id: &str) {
    let owner = id.split_once(':').map_or(id, |(owner, _)| owner);
    let offered = s.host.exports.get(owner).cloned().unwrap_or_default();
    s.logs.push((
        Level::WARN,
        enshwick_core::__fake::rules::unexported_reason(kind, id, &offered),
    ));
}

/// Notes the manifest of the mod whose init runs on `api`, when `api` is the
/// fake's table, so the fake refuses by the mod's names and side.
/// Why the fake refuses the manifest, as the loader refuses one with an
/// export that lacks its kind or its local name; `None` when it takes it.
pub(crate) fn note_manifest(
    api: *const HostApi,
    manifest: &enshwick_core::abi::Manifest,
) -> Option<String> {
    if !core::ptr::eq(api, &raw const TABLE) {
        return None;
    }
    let exports: Vec<String> = if manifest.exports.is_null() {
        Vec::new()
    } else {
        // SAFETY: a manifest points at `export_count` names that live as
        // long as it, which outlives the mod's init.
        unsafe { core::slice::from_raw_parts(manifest.exports, manifest.export_count) }
            .iter()
            .map(|name| text(*name))
            .collect()
    };
    if let Some(bad) = enshwick_core::__fake::rules::bad_export(&exports) {
        return Some(format!(
            "the export {bad:?} lacks its kind or its local name: write <kind>:<name>"
        ));
    }
    with((), |s| s.manifest = Some(ManifestFacts::of(manifest)));
    None
}

/// Whether the fake, when `api` is its table, refuses the mod of `manifest`
/// before its init, as the loader skips a mod whose side is not server or
/// both; records the reason as a fault.
pub(crate) fn refuses_side(api: *const HostApi, manifest: &enshwick_core::abi::Manifest) -> bool {
    use enshwick_core::abi::Side;
    core::ptr::eq(api, &raw const TABLE)
        && !matches!(manifest.side, Side::SERVER | Side::BOTH)
        && with(false, |s| {
            let reason = format!(
                "side {} is not a mod's side: a mod runs on the server or on both",
                manifest.side.0
            );
            s.logs.push((Level::WARN, format!("skipped: {reason}")));
            s.faults.push(reason);
            true
        })
}

/// Whether the fake, when `api` is its table, turns the mod of `manifest`
/// off before its init, as the loader does a mod with `developer: true`
/// while developer tools are off; records the off reason as the loader
/// does when it does.
pub(crate) fn refuses_developer(
    api: *const HostApi,
    manifest: &enshwick_core::abi::Manifest,
) -> bool {
    core::ptr::eq(api, &raw const TABLE)
        && manifest
            .flags
            .contains(enshwick_core::abi::ManifestFlags::DEVELOPER)
        && with(false, |s| {
            if s.host.developer_tools {
                return false;
            }
            let reason = enshwick_core::mods::DEVELOPER_TOOL_OFF;
            s.faults.push(reason.to_owned());
            s.logs.push((
                Level::ERROR,
                format!("off: {reason} ({})", enshwick_core::__fake::MOD_OFF),
            ));
            true
        })
}

pub(crate) extern "system" fn fault(me: ModHandle, reason: Str) {
    let reason = text(reason);
    // The loader logs every turn-off with its reason.
    let line = format!("off: {}", reason.replace('\t', " "));
    with((), |s| {
        s.faults.push(reason);
        s.sent.push((me, Level::ERROR, line.clone()));
        s.logs.push((Level::ERROR, line));
    });
}

pub(crate) extern "system" fn log(me: ModHandle, level: Level, message: Str) {
    let message = text(message);
    with((), |s| {
        s.sent.push((me, level, message.clone()));
        s.logs.push((level, message));
    });
}

pub(crate) extern "system" fn settings(_me: ModHandle, file: Str, out: *mut Str) -> Status {
    let file = text(file);
    let served = with(None, |s| {
        if !s.config_schemas.contains_key(&file) {
            return None;
        }
        let served: Box<str> = s
            .host
            .settings
            .get(&file)
            .cloned()
            .unwrap_or_default()
            .into();
        // The heap text stays where it is while the box lives in the state.
        let view = Str::new(&served);
        s.settings.push(served);
        Some(view)
    });
    found(served, out)
}

pub(crate) extern "system" fn rename_content(
    _me: ModHandle,
    renames: *const ContentRename,
    count: u32,
) -> Status {
    let renames: Vec<([u8; 16], [u8; 16])> = match count {
        0 => Vec::new(),
        _ if renames.is_null() => return Status::BAD_ARGS,
        // SAFETY: `renames` is non-null, checked just before, and the ABI has
        // the mod keep `count` aligned entries there for the call.
        _ => unsafe { core::slice::from_raw_parts(renames, count as usize) }
            .iter()
            .map(|r| (r.from, r.to))
            .collect(),
    };
    if renames.iter().any(|(from, to)| from == to) {
        return Status::REFUSED;
    }
    with(Status::MISSING, |s| {
        if s.content_renames.is_some() {
            return Status::REFUSED;
        }
        s.content_renames = Some(renames);
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
        if stride < size_of::<enshwick_core::abi::Header>() {
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

pub(crate) extern "system" fn config_schema(
    _me: ModHandle,
    file: Str,
    schema: Str,
    _template: Str,
) -> Status {
    let (file, text) = (text(file), text(schema));
    if !enshwick_core::config::__file_name(&file)
        || enshwick_core::config::Schema::parse(&text).is_err()
    {
        return Status::BAD_ARGS;
    }
    with(Status::REFUSED, |s| {
        if s.config_schemas
            .get(&file)
            .is_some_and(|known| *known != text)
        {
            return Status::REFUSED;
        }
        s.config_schemas.insert(file, text);
        Status::OK
    })
}

pub(crate) extern "system" fn config_check(
    me: ModHandle,
    file: Str,
    callback: ConfigCheckFn,
    user: *mut c_void,
) -> Status {
    let file = text(file);
    with(Status::MISSING, |s| {
        if s.config_checks.contains_key(&file) {
            return Status::REFUSED;
        }
        s.config_checks
            .insert(file, Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn config_set(
    _me: ModHandle,
    file: Str,
    key: Str,
    value: Str,
) -> Status {
    let (file, key, value) = (text(file), text(key), text(value));
    let prepared = with(Err(Status::MISSING), |s| {
        let serving = s.host.side == Side::SERVER || s.host.hosting;
        if !serving || s.host.world_key.is_none() {
            return Err(Status::UNAVAILABLE);
        }
        // shortcut: no list element keys (`per_level[2]`), which the loader
        // takes; add them when a test needs one.
        let schema = s
            .config_schemas
            .get(&file)
            .and_then(|text| enshwick_core::config::Schema::parse(text).ok())
            .ok_or(Status::MISSING)?;
        let field = schema.field(&key).ok_or(Status::MISSING)?;
        let mut parsed = format!("v = {value}")
            .parse::<toml::Table>()
            .ok()
            .and_then(|mut table| table.remove("v"))
            .ok_or(Status::BAD_ARGS)?;
        if let (toml::Value::Integer(n), Some(enshwick_core::config::Kind::Float)) =
            (&parsed, field.kind)
        {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a whole number for a number setting, as the loader reads it"
            )]
            let float = *n as f64;
            parsed = toml::Value::Float(float);
        }
        field.check(&parsed).map_err(|_| Status::BAD_ARGS)?;
        let mut table = s
            .host
            .settings
            .get(&file)
            .map_or_else(
                || Ok(toml::Table::new()),
                |text| text.parse::<toml::Table>(),
            )
            .map_err(|_| Status::REFUSED)?;
        if !set_dotted(&mut table, &key, parsed) {
            return Err(Status::REFUSED);
        }
        Ok(table.to_string())
    });
    let new = match prepared {
        Ok(new) => new,
        Err(status) => return status,
    };
    let check = with(None, |s| s.config_checks.get(&file).copied());
    if let Some(check) = check {
        let mut why = Str::EMPTY;
        // SAFETY: the mod's check with its own pointer; the text lives for
        // the call.
        let status =
            check.run(|callback, user| unsafe { callback(Str::new(&new), &raw mut why, user) });
        if !status.is_ok() {
            return Status::REFUSED;
        }
    }
    with((), |s| {
        s.host.settings.insert(file, new);
    });
    // SAFETY: the settings change carries no data.
    let _ran = unsafe {
        crate::test::base::events::dispatch(
            enshwick_core::abi::Event::SETTINGS_CHANGED,
            core::ptr::null(),
        )
    };
    Status::OK
}

/// Sets the dotted `key` in `table`, making the tables on its way; `false`
/// when a part of the way is no table.
fn set_dotted(table: &mut toml::Table, key: &str, value: toml::Value) -> bool {
    let mut parts = key.split('.').peekable();
    let mut here = table;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            here.insert(part.to_owned(), value);
            return true;
        }
        let next = here
            .entry(part.to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(next) = next else {
            return false;
        };
        here = next;
    }
    false
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
