//! Host functions for services, mod events, extension points and content ids.

use std::sync::atomic::Ordering;

use enshwick::abi as api_frame;
use enshwick::abi::{Event, EventFn, Header, Level, ModHandle, Status, Str, TickHandle};

use crate::host::join::keep;
use crate::host::memory::{get_headed, put, put_headed, text};
use crate::host::{Loader, Subscription, caller, caller_index, calling, entry, mark_handled};
use crate::locks::Relock;

impl crate::points::Mods for Loader {
    fn find(&self, id: &str) -> Option<usize> {
        self.find_mod(id).map(|(index, _)| index)
    }
}

impl crate::services::Names for Loader {
    fn name(&self, index: usize) -> (&str, &str) {
        self.mods
            .get(index)
            .map_or(("?", "?"), |m| (m.id.as_str(), m.version.as_str()))
    }

    fn off(&self, index: usize) -> bool {
        self.mods
            .get(index)
            .is_none_or(|m| m.off.load(Ordering::Acquire))
    }
}

/// The name and layout of a service or event spec the safe layer passed.
///
/// # Safety
///
/// - `name` is borrowed and unchanged for the call.
/// - `layout` points at `count` fields readable for the call, each name
///   borrowed and unchanged for the call, as a spec the safe layer passed.
pub(crate) unsafe fn spec_parts(
    name: Str,
    layout: &api_frame::Layout,
) -> Option<(&str, crate::services::Fields)> {
    let name = text(name)?;
    // SAFETY: the caller guarantees `layout`'s fields and their names readable
    // and unchanged for the call.
    let fields = unsafe { enshwick::services::read_layout(layout) }?;
    Some((name, fields))
}

pub(crate) extern "system" fn publish(
    me: ModHandle,
    spec: *const api_frame::ServiceSpec,
) -> Status {
    entry("publish", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a mod passes null or its version of the spec, and all of
        // the spec is required.
        let Some(spec) = (unsafe { get_headed(spec, size_of::<api_frame::ServiceSpec>()) }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass the spec's name and layout, with its
        // fields and their names, readable and unchanged for the call.
        let Some((name, fields)) = (unsafe { spec_parts(spec.name, &spec.layout) }) else {
            return Status::BAD_ARGS;
        };
        if spec.table.is_null() {
            return Status::BAD_ARGS;
        }
        let published = crate::services::registry().publish(
            index,
            name,
            spec.major,
            fields,
            spec.table.expose_provenance(),
            loader,
        );
        match published {
            Ok(()) => Status::OK,
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn service(
    me: ModHandle,
    wanted: *const api_frame::ServiceSpec,
    out: *mut api_frame::ServiceFound,
) -> Status {
    entry("service", || {
        let loader = match caller(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: a mod passes null or its version of the spec, and all of
        // the spec is required.
        let Some(wanted) = (unsafe { get_headed(wanted, size_of::<api_frame::ServiceSpec>()) })
        else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass the spec's name and layout, with its
        // fields and their names, readable and unchanged for the call.
        let Some((name, fields)) = (unsafe { spec_parts(wanted.name, &wanted.layout) }) else {
            return Status::BAD_ARGS;
        };
        let found = crate::services::registry().find(name, wanted.major, &fields, loader);
        let mut answer = api_frame::ServiceFound::EMPTY;
        let status = match found {
            Ok(found) => {
                let (id, version) = crate::services::Names::name(loader, found.owner.get());
                answer.table = core::ptr::with_exposed_provenance(found.table);
                answer.fields = found.fields;
                answer.owner = Str::new(keep(id));
                answer.owner_version = Str::new(keep(version));
                Status::OK
            }
            Err((status, reason)) => {
                answer.reason = Str::new(keep(&reason));
                status
            }
        };
        // SAFETY: `HostApi::service` has the mod pass `out` null or pointing at
        // its version of `ServiceFound`, writable for the size its header
        // states, that no other thread touches during the call.
        match unsafe { put_headed(out, answer) } {
            Status::OK => status,
            refused => refused,
        }
    })
}

pub(crate) extern "system" fn define_event(
    me: ModHandle,
    spec: *const api_frame::EventSpec,
    out: *mut u32,
) -> Status {
    entry("define_event", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if crate::services::sealed() {
            return Status::TOO_LATE;
        }
        // SAFETY: a mod passes null or its version of the spec, and all of
        // the spec is required.
        let Some(spec) = (unsafe { get_headed(spec, size_of::<api_frame::EventSpec>()) }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass the spec's name and layout, with its
        // fields and their names, readable and unchanged for the call.
        let Some((name, fields)) = (unsafe { spec_parts(spec.name, &spec.layout) }) else {
            return Status::BAD_ARGS;
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        let defined = crate::services::registry().define(
            index,
            &record.names(),
            name,
            fields,
            spec.layout.size,
        );
        match defined {
            // SAFETY: `HostApi::define_event` has the mod pass `out` null or
            // pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(id) => unsafe { put(out, id) },
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn subscribe_event(
    me: ModHandle,
    spec: *const api_frame::EventSpec,
    callback: api_frame::ModEventFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("subscribe_event", || {
        let index = match caller_index(me) {
            Ok((_, index)) => index,
            Err(status) => return status,
        };
        if crate::services::sealed() {
            return Status::TOO_LATE;
        }
        // SAFETY: a mod passes null or its version of the spec, and all of
        // the spec is required.
        let Some(spec) = (unsafe { get_headed(spec, size_of::<api_frame::EventSpec>()) }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass the spec's name and layout, with its
        // fields and their names, readable and unchanged for the call.
        let Some((name, fields)) = (unsafe { spec_parts(spec.name, &spec.layout) }) else {
            return Status::BAD_ARGS;
        };
        crate::services::registry().subscribe(
            index,
            name,
            fields,
            spec.layout.size,
            callback,
            user.expose_provenance(),
        );
        Status::OK
    })
}

/// The declaration a point spec carries; `None` for a bad one.
///
/// # Safety
///
/// - `spec` is null or points at a mod's version of the spec, readable for
///   the size its header gives, with no concurrent writer.
/// - Its name and both layouts, with their fields and names, are readable
///   and unchanged for the call.
pub(crate) unsafe fn point_parts(
    spec: *const api_frame::PointSpec,
) -> Option<crate::points::Declared> {
    // SAFETY: the caller guarantees `spec` null or readable for the size its
    // header gives, with no concurrent writer; all of the spec is required.
    let spec = unsafe { get_headed(spec, size_of::<api_frame::PointSpec>()) }?;
    // SAFETY: the caller guarantees the spec's name and layouts readable and
    // unchanged for the call; `spec` is a copy holding the same pointers.
    unsafe { declared(&spec) }
}

/// The declaration of a point spec read through its header.
///
/// # Safety
///
/// - The spec's name is borrowed and unchanged for the call.
/// - Its `input` and `output` layouts point at fields readable for the call,
///   each name borrowed and unchanged for the call.
pub(crate) unsafe fn declared(spec: &api_frame::PointSpec) -> Option<crate::points::Declared> {
    // SAFETY: the caller guarantees the spec's name and its `input` layout
    // readable and unchanged for the call.
    let (name, input) = unsafe { spec_parts(spec.name, &spec.input) }?;
    // SAFETY: the caller guarantees the spec's `output` layout, its fields and
    // their names, readable and unchanged for the call.
    let output = unsafe { enshwick::services::read_layout(&spec.output) }?;
    Some(crate::points::Declared {
        name: name.to_owned(),
        input,
        output,
        mode: spec.mode,
        refuse: spec.flags.contains(api_frame::PointFlags::REFUSE),
        input_size: spec.input.size,
        output_size: spec.output.size,
    })
}

/// The ids a link names.
///
/// # Safety
///
/// - `raw` is null or points at `count` aligned `Str`s readable for the call.
/// - Each string's bytes are borrowed and unchanged for the call.
pub(crate) unsafe fn point_ids(raw: *const Str, count: u32) -> Option<Vec<String>> {
    if count == 0 {
        return Some(Vec::new());
    }
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null and `count` nonzero, checked just before; the
    // caller guarantees `count` aligned `Str`s there, readable for the call.
    let ids = unsafe { core::slice::from_raw_parts(raw, count as usize) };
    ids.iter().map(|id| text(*id).map(str::to_owned)).collect()
}

pub(crate) extern "system" fn define_point(
    me: ModHandle,
    spec: *const api_frame::PointSpec,
    out: *mut api_frame::PointHandle,
) -> Status {
    entry("define_point", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if crate::points::sealed() {
            return Status::TOO_LATE;
        }
        // SAFETY: a mod passes null or its version of the spec, and all of
        // the spec is required.
        let Some(spec) = (unsafe { get_headed(spec, size_of::<api_frame::PointSpec>()) }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass the spec's name and layouts, with
        // their fields and names, readable and unchanged for the call.
        let Some(declared) = (unsafe { declared(&spec) }) else {
            return Status::BAD_ARGS;
        };
        // Checked before the point is defined: a point whose handle is lost
        // could never be asked, nor defined again.
        // SAFETY: a non-null out pointer from a mod starts with its header.
        let room = (!out.is_null()).then(|| unsafe { out.cast::<Header>().read_unaligned() }.size);
        if room.is_none_or(|room| (room as usize) < size_of::<api_frame::PointHandle>()) {
            return Status::BAD_ARGS;
        }
        let defined = crate::points::registry().define(index, &record.names(), declared);
        match defined {
            // SAFETY: `HostApi::define_point` has the mod pass `out` null or
            // pointing at its version of `PointHandle`, writable for the size
            // its header states, that no other thread touches during the call.
            Ok((id, live)) => unsafe {
                put_headed(
                    out,
                    api_frame::PointHandle {
                        header: Header::of::<api_frame::PointHandle>(),
                        id,
                        live,
                    },
                )
            },
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn attach_point(
    me: ModHandle,
    link: *const api_frame::PointLink,
    callback: api_frame::PointFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("attach_point", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if crate::points::sealed() {
            return Status::TOO_LATE;
        }
        // SAFETY: a mod passes null or its version of the link, and all of
        // the link is required.
        let Some(link) = (unsafe { get_headed(link, size_of::<api_frame::PointLink>()) }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass `link.point` null or its version of
        // the spec, with its name and layouts readable and unchanged for the
        // call.
        let declared = unsafe { point_parts(link.point) };
        // SAFETY: the ABI has the mod pass `link.before` null or pointing at
        // `before_count` strings readable and unchanged for the call.
        let before = unsafe { point_ids(link.before, link.before_count) };
        // SAFETY: the ABI has the mod pass `link.after` null or pointing at
        // `after_count` strings readable and unchanged for the call.
        let after = unsafe { point_ids(link.after, link.after_count) };
        let parts = (declared, before, after);
        let (Some(declared), Some(before), Some(after)) = parts else {
            return Status::BAD_ARGS;
        };
        let attached = crate::points::registry().attach(crate::points::Pending {
            owner: index,
            declared,
            priority: link.priority,
            before,
            after,
            callback,
            user: user.expose_provenance(),
        });
        match attached {
            Ok(()) => Status::OK,
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn run_point(
    me: ModHandle,
    point: u32,
    input: *const core::ffi::c_void,
    value: *mut core::ffi::c_void,
    verdict: *mut api_frame::PointVerdict,
) -> Status {
    entry("run_point", || {
        let index = match caller_index(me) {
            Ok((_, index)) => index,
            Err(status) => return status,
        };
        match crate::points::run(index.get(), point, input, value) {
            // SAFETY: `HostApi::run_point` has the mod pass `verdict` null or
            // pointing at a writable, aligned `PointVerdict` that no other
            // thread touches during the call.
            Ok(answer) => unsafe { put(verdict, answer) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn declare_content(
    me: ModHandle,
    id: Str,
    out: *mut [u8; 16],
) -> Status {
    entry("declare_content", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.delivery.get().is_some() {
            return Status::TOO_LATE;
        }
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        match crate::ids::declare_content(&record.names(), id) {
            // SAFETY: `HostApi::declare_content` has the mod pass `out` null or
            // pointing at a writable, aligned `[u8; 16]` that no other thread
            // touches during the call.
            Ok(guid) => unsafe { put(out, guid.0) },
            Err(why) => {
                loader.turn_off(index, &why);
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn send_event(
    me: ModHandle,
    event: u32,
    data: *const core::ffi::c_void,
) -> Status {
    entry("send_event", || match caller_index(me) {
        Ok((_, index)) => crate::services::send(index.get(), event, data),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn subscribe(
    me: ModHandle,
    event: Event,
    callback: EventFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("subscribe", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !(Event::SETTINGS_CHANGED.0..=Event::PLAYER_DIED.0).contains(&event.0) {
            return Status::MISSING;
        }
        let subscription = Subscription {
            owner: index,
            event,
            callback,
            user: user.expose_provenance(),
            generation: crate::reload::generation(index.get()),
        };
        if loader.delivery.get().is_some() {
            if crate::reload::reinitialising(index.get()) {
                mark_handled(event);
                crate::reload::subscribe_late(subscription);
                return Status::OK;
            }
            return Status::TOO_LATE;
        }
        let mut subscriptions = loader.subscriptions.relock();
        subscriptions.push(subscription);
        Status::OK
    })
}

pub(crate) extern "system" fn subscribe_tick(
    me: ModHandle,
    every_nanos: u64,
    callback: EventFn,
    user: *mut core::ffi::c_void,
    out: *mut TickHandle,
) -> Status {
    entry("subscribe_tick", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        if loader.delivery.get().is_some() && !crate::reload::reinitialising(index.get()) {
            return Status::TOO_LATE;
        }
        let generation = crate::reload::generation(index.get());
        let Some(handle) = crate::ticks::TICKS.add(index, generation, every_nanos, callback, user)
        else {
            return Status::REFUSED;
        };
        // SAFETY: `out` is non-null by the check before it and, by
        // `HostApi::subscribe_tick`, the caller's and writable for a handle.
        unsafe { out.write_unaligned(handle) };
        Status::OK
    })
}

pub(crate) extern "system" fn set_tick_every(
    me: ModHandle,
    handle: TickHandle,
    every_nanos: u64,
) -> Status {
    entry("set_tick_every", || match calling(me) {
        Ok((_, index, _)) => crate::ticks::TICKS.set_every(
            handle,
            index,
            crate::reload::generation(index.get()),
            every_nanos,
        ),
        Err(status) => status,
    })
}
