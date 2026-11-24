//! The fake's services, mod events and content ids.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::{Registered, put, text, with};
use enshwick_core::abi::{
    Event, EventFn, EventSpec, ModEventFn, ServiceFound, ServiceSpec, Status, Str,
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

entries! {
    pub(crate) fn publish(me as Mod, spec: *const ServiceSpec as args::Headed) -> Status {
        let Some(spec) = crate::test::table::get_headed(spec) else {
            return Status::BAD_ARGS;
        };
        let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            let taken = s
                .base
                .published
                .iter()
                .any(|p| p.name == name && p.major == spec.major)
                || s.host
                    .base
                    .services
                    .iter()
                    .any(|p| p.name == name && p.major == spec.major);
            if taken {
                return Status::REFUSED;
            }
            s.base.published.push(crate::test::FakeService {
                name,
                major: spec.major,
                fields,
                table: spec.table.expose_provenance(),
            });
            Status::OK
        })
    }

    pub(crate) fn service(
        me as Caller,
        wanted: *const ServiceSpec as args::Headed,
        out: *mut ServiceFound as args::HeadedOut,
    ) -> Status {
        let Some(wanted) = crate::test::table::get_headed(wanted) else {
            return Status::BAD_ARGS;
        };
        let (name, Some(fields)) = (text(wanted.name), layout_of(&wanted.layout)) else {
            return Status::BAD_ARGS;
        };
        let answer = with(Err("no fake installed".to_owned()), |s| {
            if s.base.manifest.as_ref().is_some_and(|manifest| {
                manifest.unexported(
                    enshwick_core::schema_section::Kind::Service,
                    &name,
                    &s.host.base.exports,
                )
            }) {
                return Err("its mod does not export it".to_owned());
            }
            let offered = s
                .host
                .base
                .services
                .iter()
                .chain(&s.base.published)
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

    pub(crate) fn define_event(
        me as Mod,
        spec: *const EventSpec as args::Headed,
        out: *mut u32 as args::Out,
    ) -> Status {
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let Some(spec) = crate::test::table::get_headed(spec) else {
            return Status::BAD_ARGS;
        };
        let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
            return Status::BAD_ARGS;
        };
        let id = with(None, |s| {
            if s.base.defined_events.iter().any(|d| d.0 == name) {
                return None;
            }
            s.base
                .defined_events
                .push((name, fields, spec.layout.size, me));
            u32::try_from(s.base.defined_events.len() - 1).ok()
        });
        match id {
            Some(id) => put(out, id),
            None => Status::REFUSED,
        }
    }

    pub(crate) fn subscribe_mod_event(
        me as Mod,
        spec: *const EventSpec as args::Headed,
        callback: Option<ModEventFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let Some(spec) = crate::test::table::get_headed(spec) else {
            return Status::BAD_ARGS;
        };
        let (name, Some(fields)) = (text(spec.name), layout_of(&spec.layout)) else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            if s.base.manifest.as_ref().is_some_and(|manifest| {
                manifest.unexported(
                    enshwick_core::schema_section::Kind::Event,
                    &name,
                    &s.host.base.exports,
                )
            }) {
                return Status::REFUSED;
            }
            s.base.mod_event_subscriptions.push((
                name,
                fields,
                spec.layout.size,
                Registered::new(callback, user, me),
            ));
            Status::OK
        })
    }

    pub(crate) fn send_event(
        me as Mod,
        event: u32,
        data: *const c_void,
    ) -> Status {
        with(Status::MISSING, |s| {
            // As the loader: before the events are sealed a send of any id
            // reaches no one and is no error.
            if !s.base.closed() {
                return Status::OK;
            }
            let Some((name, size, owner)) = s
                .base
                .defined_events
                .get(event as usize)
                .map(|d| (d.0.clone(), d.2, d.3))
            else {
                return Status::BAD_ARGS;
            };
            if owner != me {
                return Status::REFUSED;
            }
            let bytes = if data.is_null() {
                Vec::new()
            } else {
                // SAFETY: the safe layer sends the defined event's data, its
                // defined size, alive for the call.
                unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec()
            };
            s.base.sent_events.push((name, bytes));
            Status::OK
        })
    }

    pub(crate) fn subscribe(
        me as Mod,
        event: Event,
        callback: Option<EventFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        // The loader refuses an event it never raises, as on an older loader.
        if !(Event::SETTINGS_CHANGED.0..=Event::WAYPOINT_CHANGED.0).contains(&event.0) {
            return Status::MISSING;
        }
        crate::test::table::with_open(|s| {
            s.base
                .subscriptions
                .push((event, Registered::new(callback, user, me)));
            Status::OK
        })
    }
}
