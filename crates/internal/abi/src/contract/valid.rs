//! [`VALID`]: entries that need no running game, each called with arguments
//! a host takes, so the hosts are held to each other on the paths that
//! succeed as on the refusals.

use core::ffi::c_void;
use core::ptr;

use bytemuck::Zeroable as _;

use super::Entry;
use crate::abi::{
    AffixSpec, ChangeId, ChangeOp, DataChange, DataField, EntityDataFlags, Event, EventSpec, Guid,
    Headed as _, Layout, Level, PointFlags, PointHandle, PointMode, PointSpec, StatRef, StatSpec,
    Str, TickHandle,
};

/// No fields.
const NO_LAYOUT: Layout = Layout {
    fields: ptr::null(),
    count: 0,
    size: 0,
};

/// A handler that ignores its event.
///
/// # Safety
///
/// None: it reads no argument.
unsafe extern "system" fn ignore_event(_event: Event, _data: *const c_void, _user: *mut c_void) {}

/// A status source that writes no line.
///
/// # Safety
///
/// None: it reads no argument.
unsafe extern "system" fn no_status(_user: *mut c_void, _out: *mut Str) {}

/// Entries that need no game, each by its field name and in field order,
/// called with arguments a host takes.
///
/// A log line, the level, the mod's folder, language and settings, a text,
/// the counts of the mods and of the loader's parts that are off, an id of
/// each content registry in the namespace of [`MOD_ID`](super::MOD_ID), a
/// tick, a status source, a feature and a stack size; and a stat and an
/// affix, each headed but otherwise blank, whose definitions the hosts read
/// only after the phase.
pub const VALID: &[(&str, Entry)] = &[
    ("log", |api, me| {
        (api.log)(me, Level::INFO, Str::new("contract"))
    }),
    ("settings", |api, me| {
        let mut out = Str::EMPTY;
        (api.settings)(me, Str::new("settings"), &raw mut out)
    }),
    ("log_level", |api, me| {
        let mut out = Level::INFO;
        (api.log_level)(me, &raw mut out)
    }),
    ("mods", |api, me| {
        let mut count = 0;
        (api.mods)(me, ptr::null_mut(), 0, 0, &raw mut count)
    }),
    ("status", |api, me| {
        (api.status)(me, Some(no_status), ptr::null_mut())
    }),
    ("texts", |api, me| {
        (api.texts)(me, Str::new("[en]\nhello = \"Hello\"\n"))
    }),
    ("define_event", |api, me| {
        let spec = crate::headed!(EventSpec {
            name: Str::new("test.fake:rang"),
            layout: NO_LAYOUT,
        });
        let mut out = 0;
        (api.define_event)(me, &raw const spec, &raw mut out)
    }),
    ("mod_dir", |api, me| {
        let mut out = Str::EMPTY;
        (api.mod_dir)(me, &raw mut out)
    }),
    ("language", |api, me| {
        let mut out = 0;
        (api.language)(me, &raw mut out)
    }),
    ("define_point", |api, me| {
        let spec = crate::headed!(PointSpec {
            name: Str::new("test.fake:limit"),
            mode: PointMode::OBSERVE,
            flags: PointFlags::NONE,
            input: NO_LAYOUT,
            output: NO_LAYOUT,
        });
        let mut out = PointHandle::EMPTY;
        (api.define_point)(me, &raw const spec, &raw mut out)
    }),
    ("feature", |api, me| {
        (api.feature)(me, Str::new("glow"), Str::EMPTY)
    }),
    ("loader_off", |api, me| {
        let mut count = 0;
        (api.loader_off)(me, ptr::null_mut(), 0, 0, &raw mut count)
    }),
    ("subscribe_tick", |api, me| {
        let mut out = TickHandle(0);
        (api.subscribe_tick)(
            me,
            1_000_000_000,
            Some(ignore_event),
            ptr::null_mut(),
            &raw mut out,
        )
    }),
    ("add_text", |api, me| {
        (api.add_text)(me, 7, 0, Str::new("Locker"))
    }),
    ("define_item_data", |api, me| {
        let mut out = 0;
        (api.define_item_data)(me, Str::new("test.fake:charge"), &raw mut out)
    }),
    ("declare_content", |api, me| {
        let mut out = Guid::zeroed();
        (api.declare_content)(me, Str::new("test.fake:locker"), &raw mut out)
    }),
    ("define_entity_data", |api, me| {
        let mut out = 0;
        (api.define_entity_data)(
            me,
            Str::new("test.fake:mark"),
            EntityDataFlags::NONE,
            &raw mut out,
        )
    }),
    ("change_data", |api, me| {
        let change = crate::headed!(DataChange {
            entry: Guid([0x3a; 16]),
            field: DataField::MAX_STACK,
            op: ChangeOp::SET,
            value: 999.0,
            ..DataChange::blank()
        });
        (api.change_data)(me, &raw const change, ptr::null_mut::<ChangeId>())
    }),
    ("define_stat", |api, me| {
        let spec = StatSpec::blank();
        let mut out = StatRef::zeroed();
        (api.define_stat)(
            me,
            Str::new("test.fake:grit"),
            &raw const spec,
            &raw mut out,
        )
    }),
    ("define_affix", |api, me| {
        let spec = AffixSpec::blank();
        let mut out = StatRef::zeroed();
        (api.define_affix)(
            me,
            Str::new("test.fake:sharp"),
            &raw const spec,
            &raw mut out,
        )
    }),
];
