//! The fake's answers about the mods: their manifests, logs, faults,
//! settings and what is loaded.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use super::io::list;
use crate::test::table::{Registered, State, TABLE, found, text, with};
use enshwick_core::abi::{ConfigCheckFn, HostApi, Level, ModHandle, ModInfo, Side, Status, Str};

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

/// Whether the caller `me` runs on both sides, as the loader reads it from
/// the caller's own record: a pack named by
/// [`FakeHost::pack`](crate::test::FakeHost::pack) by its side, the mod
/// under test by its manifest (both when it declared none), a mod
/// [`FakeHost::other_mod`](crate::test::FakeHost::other_mod) named as both,
/// and the loader's own copy as the server only.
pub(crate) fn joins(s: &State, me: ModHandle) -> bool {
    if let Some(&(_, side)) = s.host.base.pack_sides.iter().find(|(pack, _)| *pack == me) {
        side == Side::BOTH
    } else if me == s.host.base.handle {
        s.base
            .manifest
            .as_ref()
            .is_none_or(|manifest| manifest.joins)
    } else {
        s.host.base.other_mods.contains(&me)
    }
}

impl ManifestFacts {
    /// The facts of `manifest`.
    fn of(manifest: &enshwick_core::abi::Manifest) -> Self {
        let dependencies = if manifest.dependencies.is_null() {
            &[][..]
        } else {
            // SAFETY: a manifest points at `dependency_count` dependencies,
            // `declare_mod!`'s static array, alive for the process.
            unsafe {
                core::slice::from_raw_parts(
                    manifest.dependencies,
                    manifest.dependency_count as usize,
                )
            }
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
    let offered = s.host.base.exports.get(owner).cloned().unwrap_or_default();
    s.base.logs.push((
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
        unsafe { core::slice::from_raw_parts(manifest.exports, manifest.export_count as usize) }
            .iter()
            .map(|name| text(*name))
            .collect()
    };
    if let Some(bad) = enshwick_core::__fake::rules::bad_export(&exports) {
        return Some(format!(
            "the export {bad:?} lacks its kind or its local name: write <kind>:<name>"
        ));
    }
    with((), |s| s.base.manifest = Some(ManifestFacts::of(manifest)));
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
            s.base
                .logs
                .push((Level::WARN, format!("skipped: {reason}")));
            s.base.faults.push(reason);
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
            if s.host.base.developer_tools {
                return false;
            }
            let reason = enshwick_core::mods::DEVELOPER_TOOL_OFF;
            s.base.faults.push(reason.to_owned());
            s.base.logs.push((Level::ERROR, off_line(reason)));
            true
        })
}

/// The line the loader logs when it turns a mod off for `reason`.
fn off_line(reason: &str) -> String {
    enshwick_base::log_files::off(reason, enshwick_core::__fake::MOD_OFF)
}

/// Turns `owner` off for `reason` as the loader does on its own: the fault
/// recorded, the line logged and sent to the mod, its callbacks silenced.
pub(crate) fn loader_turns_off(owner: ModHandle, reason: String) {
    let line = off_line(&reason);
    with((), |s| {
        s.base.faults.push(reason);
        s.base.sent.push((owner, Level::ERROR, line.clone()));
        s.base.logs.push((Level::ERROR, line));
    });
    super::registered::turn_off(owner);
}

entries! {
    pub(crate) fn fault(me as AnyMod, reason: Str) -> Status {
        let reason = text(reason);
        // The loader logs every turn-off with its reason.
        let line = off_line(&reason.replace('\t', " "));
        with((), |s| {
            s.base.faults.push(reason);
            s.base.sent.push((me, Level::ERROR, line.clone()));
            s.base.logs.push((Level::ERROR, line));
        });
        super::registered::turn_off(me);
        Status::OK
    }

    pub(crate) fn log(me as Writer, level: Level, message: Str as args::Text) -> Status {
        let message = text(message);
        with((), |s| {
            s.base.sent.push((me, level, message.clone()));
            s.base.logs.push((level, message));
        });
        Status::OK
    }

    pub(crate) fn settings(
        me as AnyMod,
        file: Str as args::Text,
        out: *mut Str as args::Out,
    ) -> Status {
        let file = text(file);
        let served = with(None, |s| {
            if !s.base.config_schemas.contains_key(&file) {
                return None;
            }
            let served: Box<str> = s
                .host
                .base
                .settings
                .get(&file)
                .cloned()
                .unwrap_or_default()
                .into();
            // The heap text stays where it is while the box lives in the state.
            let view = Str::new(&served);
            s.base.settings.push(served);
            Some(view)
        });
        found(served, out)
    }

    pub(crate) fn mods(
        me as AnyMod,
        [out: *mut ModInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let mods = with(Vec::new(), |s| s.host.base.mods.clone());
        list(&mods, out, entry_size, capacity, count)
    }

    pub(crate) fn loader_off(
        me as AnyMod,
        [out: *mut enshwick_core::abi::LoaderOff, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let off = with(Vec::new(), |s| s.base.loader_off.clone());
        list(&off, out, entry_size, capacity, count)
    }

    pub(crate) fn join_missing(
        me as AnyMod,
        [out: *mut ModInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let mods = with(Vec::new(), |s| s.host.base.join_missing.clone());
        list(&mods, out, entry_size, capacity, count)
    }

    pub(crate) fn config_schema(
        me as AnyMod,
        file: Str as args::Text,
        schema: Str as args::Text,
        _template: Str as args::Text,
    ) -> Status {
        let (file, text) = (text(file), text(schema));
        if !enshwick_core::config::__file_name(&file)
            || enshwick_core::config::Schema::parse(&text).is_err()
        {
            return Status::BAD_ARGS;
        }
        with(Status::REFUSED, |s| {
            if s.base
                .config_schemas
                .get(&file)
                .is_some_and(|known| *known != text)
            {
                return Status::REFUSED;
            }
            s.base.config_schemas.insert(file, text);
            Status::OK
        })
    }

    pub(crate) fn config_check(
        me as Mod,
        file: Str as args::Text,
        callback: Option<ConfigCheckFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        let file = text(file);
        crate::test::table::with_open(|s| {
            if s.base.config_checks.contains_key(&file) {
                return Status::REFUSED;
            }
            s.base
                .config_checks
                .insert(file, Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn set_config(
        me as Mod,
        file: Str as args::Text,
        key: Str as args::Text,
        value: Str as args::Text,
    ) -> Status {
        // As the loader: a mod without settings files has nothing to write,
        // only a running world's server writes, and a file the mod does not
        // have is missing, before nesting counts. Only the mod under test
        // has settings files here.
        let file = text(file);
        let placed = with(Some(Status::MISSING), |s| {
            if me != s.host.base.handle || s.base.config_schemas.is_empty() {
                Some(Status::MISSING)
            } else if !s.host.base.serving() || s.host.base.world_key.is_none() {
                Some(Status::UNAVAILABLE)
            } else if !file.is_empty() && !s.base.config_schemas.contains_key(&file) {
                Some(Status::MISSING)
            } else {
                None
            }
        });
        if let Some(refused) = placed {
            return refused;
        }
        let entered = with(false, |s| {
            let refused = s.base.config_writes.iter().any(|&checking| checking)
                || s.base.config_writes.len() >= enshwick_core::config::MAX_NESTED_WRITES;
            if !refused {
                s.base.config_writes.push(false);
            }
            !refused
        });
        if !entered {
            return Status::REFUSED;
        }
        let status = write_setting(file, &text(key), &text(value));
        with((), |s| {
            s.base.config_writes.pop();
        });
        status
    }
}

/// Marks whether the innermost settings write runs the mod's check.
fn checking(on: bool) {
    with((), |s| {
        if let Some(last) = s.base.config_writes.last_mut() {
            *last = on;
        }
    });
}

/// Writes `value` as `key` of the settings file `file`, as the loader does.
fn write_setting(file: String, key: &str, value: &str) -> Status {
    let prepared = with(Err(Status::MISSING), |s| {
        let serving = s.host.base.serving();
        if !serving || s.host.base.world_key.is_none() {
            return Err(Status::UNAVAILABLE);
        }
        // shortcut: no list element keys (`per_level[2]`), which the loader
        // takes; add them when a test needs one.
        let schema = s
            .base
            .config_schemas
            .get(&file)
            .and_then(|text| enshwick_core::config::Schema::parse(text).ok())
            .ok_or(Status::MISSING)?;
        let field = schema.field(key).ok_or(Status::MISSING)?;
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
            .base
            .settings
            .get(&file)
            .map_or_else(
                || Ok(toml::Table::new()),
                |text| text.parse::<toml::Table>(),
            )
            .map_err(|_| Status::REFUSED)?;
        if !set_dotted(&mut table, key, parsed) {
            return Err(Status::REFUSED);
        }
        Ok(table.to_string())
    });
    let new = match prepared {
        Ok(new) => new,
        Err(status) => return status,
    };
    let check = with(None, |s| s.base.config_checks.get(&file).copied());
    if let Some(check) = check {
        let mut why = Str::EMPTY;
        checking(true);
        // SAFETY: the mod's check with its own pointer; the text lives for
        // the call.
        let status =
            check.run(|callback, user| unsafe { callback(Str::new(&new), &raw mut why, user) });
        checking(false);
        if !status.is_ok() {
            return Status::REFUSED;
        }
    }
    with((), |s| {
        s.host.base.settings.insert(file, new);
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

entries! {
    pub(crate) fn texts(me as Mod, toml: Str as args::Text) -> Status {
        let toml = text(toml);
        with(Status::MISSING, |s| {
            s.base.published_texts = Some(toml);
            Status::OK
        })
    }
}

std::thread_local! {
    /// The language of the player the loader calls the mod about on this
    /// thread, inside [`for_reader`].
    static READER: core::cell::Cell<Option<u32>> = const { core::cell::Cell::new(None) };
}

/// Runs `f` with `language` as the reader's, as the loader does around a
/// call about one player; `None` keeps the reader around the call. Puts the
/// one before back, also when `f` panics.
pub(crate) fn for_reader<R>(language: Option<u32>, f: impl FnOnce() -> R) -> R {
    /// Puts the reader before back.
    struct Restore(Option<u32>);
    impl Drop for Restore {
        fn drop(&mut self) {
            READER.set(self.0);
        }
    }
    let _restore = Restore(READER.replace(language.or(READER.get())));
    f()
}

/// The language the game of the player whose character is `entity`
/// named, or the game's own for the hosting client's own player, as the
/// loader scopes a building event: `0`, the fallback, for a player whose
/// game named none; `None` for no player's character.
pub(crate) fn language_of_character(entity: enshwick_core::Entity) -> Option<u32> {
    with(None, |s| {
        let key = s.host.base.players.iter().find(|p| p.entity == entity)?.key;
        Some(s.host.base.language_of(key))
    })
}

entries! {
    pub(crate) fn language(me as Writer, out: *mut u32 as args::Out) -> Status {
        crate::test::table::put(
            out,
            READER
                .get()
                .unwrap_or_else(|| with(0, |s| s.host.base.language)),
        )
    }
}
