//! The fake's services, mod events and content ids.

use core::ffi::c_void;

use crate::test::table::{Registered, put, text, with};
use enshwick_core::abi::{
    Event, EventFn, EventSpec, ModEventFn, ModHandle, ServiceFound, ServiceSpec, Status, Str,
};

/// The `(name, hash)` fields of a layout the safe layer passed.
pub(crate) fn layout_of(layout: &enshwick_core::abi::Layout) -> Option<Vec<(String, u64)>> {
    // SAFETY: the safe layer passes the layout of a declared type.
    unsafe { enshwick_core::services::read_layout(layout) }
}

/// Borrowed `(name, hash)` pairs.
pub(crate) fn view(fields: &[(String, u64)]) -> Vec<(&str, u64)> {
    fields.iter().map(|(n, h)| (n.as_str(), *h)).collect()
}

pub(crate) extern "system" fn publish(_me: ModHandle, spec: *const ServiceSpec) -> Status {
    let Some(spec) = crate::test::table::get_headed(spec) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let taken = s
            .published
            .iter()
            .any(|p| p.name == name && p.major == spec.major)
            || s.host
                .services
                .iter()
                .any(|p| p.name == name && p.major == spec.major);
        if taken {
            return Status::REFUSED;
        }
        s.published.push(crate::test::FakeService {
            name,
            major: spec.major,
            fields,
            table: spec.table.expose_provenance(),
        });
        Status::OK
    })
}

pub(crate) extern "system" fn service(
    _me: ModHandle,
    wanted: *const ServiceSpec,
    out: *mut ServiceFound,
) -> Status {
    let Some(wanted) = crate::test::table::get_headed(wanted) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(wanted.name), layout_of(&wanted.layout)) else {
        return Status::BAD_ARGS;
    };
    let answer = with(Err("no fake installed".to_owned()), |s| {
        if s.manifest.as_ref().is_some_and(|manifest| {
            manifest.unexported(
                enshwick_core::schema_section::Kind::Service,
                &name,
                &s.host.exports,
            )
        }) {
            return Err("its mod does not export it".to_owned());
        }
        let offered = s
            .host
            .services
            .iter()
            .chain(&s.published)
            .find(|p| p.name == name && p.major == wanted.major)
            .ok_or_else(|| "no running mod publishes it".to_owned())?;
        enshwick_core::services::compare(&view(&fields), &view(&offered.fields))
            .map_err(|e| e.to_string())?;
        Ok((offered.table, offered.fields.len()))
    });
    let mut found = ServiceFound::EMPTY;
    let status = match answer {
        Ok((table, count)) => {
            found.table = core::ptr::with_exposed_provenance(table);
            found.fields = u32::try_from(count).unwrap_or(u32::MAX);
            found.owner = Str::new(crate::test::MOD_ID);
            Status::OK
        }
        Err(reason) => {
            found.reason = Str::new(Box::leak(reason.into_boxed_str()));
            Status::REFUSED
        }
    };
    match crate::test::table::put_headed(out, found) {
        Status::OK => status,
        other => other,
    }
}

/// Whether init has ended, so registrations come too late; the loader asks
/// this before it reads the call's arguments.
fn closed() -> bool {
    with(false, |s| {
        s.registration == crate::test::table::Registration::Closed
    })
}

pub(crate) extern "system" fn define_event(
    _me: ModHandle,
    spec: *const EventSpec,
    out: *mut u32,
) -> Status {
    if closed() {
        return Status::TOO_LATE;
    }
    let Some(spec) = crate::test::table::get_headed(spec) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
        return Status::BAD_ARGS;
    };
    let id = with(None, |s| {
        if s.defined_events.iter().any(|d| d.0 == name) {
            return None;
        }
        s.defined_events.push((name, fields, spec.layout.size));
        u32::try_from(s.defined_events.len() - 1).ok()
    });
    match id {
        Some(id) => put(out, id),
        None => Status::REFUSED,
    }
}

pub(crate) extern "system" fn subscribe_event(
    me: ModHandle,
    spec: *const EventSpec,
    callback: ModEventFn,
    user: *mut c_void,
) -> Status {
    if closed() {
        return Status::TOO_LATE;
    }
    let Some(spec) = crate::test::table::get_headed(spec) else {
        return Status::BAD_ARGS;
    };
    let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.manifest.as_ref().is_some_and(|manifest| {
            manifest.unexported(
                enshwick_core::schema_section::Kind::Event,
                &name,
                &s.host.exports,
            )
        }) {
            return Status::REFUSED;
        }
        s.mod_event_subscriptions.push((
            name,
            fields,
            spec.layout.size,
            Registered::new(callback, user, me),
        ));
        Status::OK
    })
}

pub(crate) extern "system" fn send_event(
    _me: ModHandle,
    event: u32,
    data: *const c_void,
) -> Status {
    with(Status::MISSING, |s| {
        let Some((name, size)) = s
            .defined_events
            .get(event as usize)
            .map(|d| (d.0.clone(), d.2))
        else {
            return Status::BAD_ARGS;
        };
        let bytes = if data.is_null() {
            Vec::new()
        } else {
            // SAFETY: the safe layer sends the defined event's data, its
            // defined size, alive for the call.
            unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec()
        };
        s.sent_events.push((name, bytes));
        Status::OK
    })
}

pub(crate) extern "system" fn subscribe(
    me: ModHandle,
    event: Event,
    callback: EventFn,
    user: *mut c_void,
) -> Status {
    // The loader refuses an event it never raises, as on an older loader.
    if !(Event::SETTINGS_CHANGED.0..=Event::PROGRESS_WRITTEN.0).contains(&event.0) {
        return Status::MISSING;
    }
    with(Status::MISSING, |s| {
        if s.registration == crate::test::table::Registration::Closed {
            return Status::TOO_LATE;
        }
        s.subscriptions
            .push((event, Registered::new(callback, user, me)));
        Status::OK
    })
}
