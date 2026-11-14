//! Host functions for services, mod events and extension points.

use std::sync::atomic::Ordering;

use enshwick_base::Relock;
use enshwick_core::abi as api_frame;
use enshwick_core::abi::{Event, EventFn, Header, Status, Str, TickHandle};

use crate::engine::kept_text::keep;
use crate::engine::loader::{Loader, ModRecord, Subscription};
use crate::host::layer::guards::{before_delivery, open};
use crate::host::layer::{Asker, Headed, HeadedOut, Mod, Out, Rule, entries};
use crate::host::memory::{headed_in, text};

impl crate::base::points::Mods for Loader {
    fn find(&self, id: &str) -> Option<usize> {
        self.find_mod(id).map(|(index, _)| index)
    }
}

impl crate::base::services::Names for Loader {
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

/// The mod side of `name`, `<id>:<what>`, when the mod of `record`
/// requires it: a call or point of a required mod, which initialises first,
/// so what it offers is known by now.
pub(crate) fn required_owner<'a>(record: &ModRecord, name: &'a str) -> Option<&'a str> {
    let (owner, _) = name.split_once(':')?;
    record.requires(owner).then_some(owner)
}

/// Why the mod of `record` may not use the `kind` `name`, `<id>:<what>`: a
/// loaded mod `id` whose manifest does not export `what` as that kind. `None`
/// for a name of its own, an exported one, or one of a mod that is not
/// loaded, which stays unmatched, so built-in `enshwick:` ids pass.
pub(crate) fn unexported(
    loader: &Loader,
    record: &ModRecord,
    kind: crate::base::manifest::Kind,
    name: &str,
) -> Option<String> {
    let (owner, _) = name.split_once(':')?;
    let (_, provider) = loader.find_mod(owner)?;
    crate::base::manifest::unexported(kind, name, &record.id, &provider.exports())
}

/// [`unexported`] as a refusal of the call.
///
/// # Errors
///
/// The [`Rule`] the unexported name breaks.
pub(crate) fn exported(
    loader: &Loader,
    record: &ModRecord,
    kind: crate::base::manifest::Kind,
    name: &str,
) -> Result<(), Rule> {
    unexported(loader, record, kind, name).map_or(Ok(()), |reason| Err(Rule(reason)))
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
) -> Option<(&str, crate::base::services::Fields)> {
    let name = text(name)?;
    // SAFETY: the caller guarantees `layout`'s fields and their names readable
    // and unchanged for the call.
    let fields = unsafe { enshwick_core::services::read_layout(layout) }?;
    Some((name, fields))
}

/// [`spec_parts`] of a spec the mod passed, refused as a bad argument when it
/// does not read.
fn parts(
    name: Str,
    layout: &api_frame::Layout,
) -> Result<(&str, crate::base::services::Fields), Status> {
    // SAFETY: the ABI has the mod pass a spec's name and layout, with its
    // fields and their names, readable and unchanged for the call.
    unsafe { spec_parts(name, layout) }.ok_or(Status::BAD_ARGS)
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
) -> Option<crate::base::points::Declared> {
    // SAFETY: the caller guarantees `spec` null or readable for the size its
    // header gives, with no concurrent writer; all of the spec is required.
    let spec = unsafe {
        headed_in(
            spec,
            <api_frame::PointSpec as enshwick_core::abi::Headed>::BORN,
        )
    }?;
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
pub(crate) unsafe fn declared(
    spec: &api_frame::PointSpec,
) -> Option<crate::base::points::Declared> {
    // SAFETY: the caller guarantees the spec's name and its `input` layout
    // readable and unchanged for the call.
    let (name, input) = unsafe { spec_parts(spec.name, &spec.input) }?;
    // SAFETY: the caller guarantees the spec's `output` layout, its fields and
    // their names, readable and unchanged for the call.
    let output = unsafe { enshwick_core::services::read_layout(&spec.output) }?;
    Some(crate::base::points::Declared {
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
    // SAFETY: the caller guarantees `raw` null or `count` aligned `Str`s
    // readable for the call, which is `list_in`'s need.
    let ids = unsafe { crate::host::memory::list_in(raw, count as usize) }?;
    ids.iter().map(|id| text(*id).map(str::to_owned)).collect()
}

entries! {
    fn publish(me as Mod, spec as Headed<api_frame::ServiceSpec>) -> Status {
        let (name, fields) = parts(spec.name, &spec.layout)?;
        if spec.table.is_null() {
            return Err(Status::BAD_ARGS.into());
        }
        crate::base::services::registry().publish(
            me.index,
            name,
            spec.major,
            fields,
            spec.table.expose_provenance(),
            me.loader,
        )?;
        Ok(())
    }

    fn service(
        me as Asker,
        wanted as Headed<api_frame::ServiceSpec>,
        out as HeadedOut<api_frame::ServiceFound>,
    ) -> Status {
        let (loader, record) = (me.loader, me.record);
        let (name, fields) = parts(wanted.name, &wanted.layout)?;
        let refused = record.and_then(|record| {
            unexported(loader, record, crate::base::manifest::Kind::Service, name)
        });
        let found = match refused {
            Some(reason) => Err(crate::base::services::FindServiceError::Unexported { reason }),
            None => crate::base::services::registry().find(name, wanted.major, &fields, loader),
        };
        let mut answer = api_frame::ServiceFound::EMPTY;
        let status = match found {
            Ok(found) => {
                let (id, version) = crate::base::services::Names::name(loader, found.owner.get());
                answer.table = core::ptr::with_exposed_provenance(found.table);
                answer.fields = found.fields;
                answer.owner = Str::new(keep(id));
                answer.owner_version = Str::new(keep(version));
                Status::OK
            }
            Err(refused) => {
                answer.reason = Str::new(keep(&refused.to_string()));
                crate::host::layer::Refusal::status(&refused)
            }
        };
        out.put(answer)?;
        Ok(status)
    }

    fn define_event(me as Mod, spec as Headed<api_frame::EventSpec>, out as Out<u32>) -> Status {
        open(crate::base::services::sealed())?;
        let (name, fields) = parts(spec.name, &spec.layout)?;
        let id = crate::base::services::registry().define(
            me.index,
            &me.record.id,
            name,
            fields,
            spec.layout.size,
        )?;
        out.put(id)
    }

    fn subscribe_event(
        me as Mod,
        spec as Headed<api_frame::EventSpec>,
        callback: api_frame::ModEventFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        open(crate::base::services::sealed())?;
        let (name, fields) = parts(spec.name, &spec.layout)?;
        exported(me.loader, me.record, crate::base::manifest::Kind::Event, name)?;
        crate::base::services::registry().subscribe(
            me.index,
            name,
            fields,
            spec.layout.size,
            callback,
            user.expose_provenance(),
        );
        Ok(())
    }

    fn define_point(
        me as Mod,
        spec as Headed<api_frame::PointSpec>,
        out as HeadedOut<api_frame::PointHandle>,
    ) -> Status {
        open(crate::base::points::sealed())?;
        // SAFETY: the ABI has the mod pass the spec's name and layouts, with
        // their fields and names, readable and unchanged for the call.
        let declared = unsafe { declared(&spec) }.ok_or(Status::BAD_ARGS)?;
        // Checked before the point is defined: a point whose handle is lost
        // could never be asked, nor defined again.
        if !out.holds_born() {
            return Err(Status::BAD_ARGS.into());
        }
        let (id, live) = crate::base::points::registry().define(me.index, &me.record.id, declared)?;
        out.put(api_frame::PointHandle {
            header: Header::of::<api_frame::PointHandle>(),
            id,
            live,
        })
    }

    fn attach_point(
        me as Mod,
        link as Headed<api_frame::PointLink>,
        callback: api_frame::PointFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        open(crate::base::points::sealed())?;
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
        let (Some(declared), Some(before), Some(after)) = (declared, before, after) else {
            return Err(Status::BAD_ARGS.into());
        };
        exported(me.loader, me.record, crate::base::manifest::Kind::Point, &declared.name)?;
        if let Some(owner) = required_owner(me.record, &declared.name)
            && let Some(reason) = crate::base::points::registry().unlinkable(&declared, me.loader)
        {
            return Err(Rule(format!(
                "point {}: this mod requires {owner}, and the link cannot match: {reason}",
                declared.name
            ))
            .into());
        }
        crate::base::points::registry().attach(crate::base::points::Pending {
            owner: me.index,
            declared,
            priority: link.priority,
            before,
            after,
            callback,
            user: user.expose_provenance(),
        })?;
        Ok(())
    }

    fn run_point(
        me as Mod,
        point: u32,
        input: *const core::ffi::c_void,
        value: *mut core::ffi::c_void,
        verdict as Out<api_frame::PointVerdict>,
    ) -> Status {
        verdict.put(crate::base::points::run(me.index.get(), point, input, value)?)
    }

    fn send_event(me as Mod, event: u32, data: *const core::ffi::c_void) -> Status {
        Ok(crate::base::services::send(me.index.get(), event, data))
    }

    fn subscribe(
        me as Mod,
        event: Event,
        callback: EventFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        if !(Event::SETTINGS_CHANGED.0..=Event::PROGRESS_WRITTEN.0).contains(&event.0) {
            return Err(Status::MISSING.into());
        }
        before_delivery(me.loader)?;
        me.loader.subscriptions.relock().push(Subscription {
            owner: me.index,
            event,
            callback,
            user: user.expose_provenance(),
        });
        Ok(())
    }

    fn subscribe_tick(
        me as Mod,
        every_nanos: u64,
        callback: EventFn,
        user: *mut core::ffi::c_void,
        out as Out<TickHandle>,
    ) -> Status {
        before_delivery(me.loader)?;
        let handle = crate::engine::ticks::TICKS
            .add(me.index, every_nanos, callback, user)
            .ok_or(Status::REFUSED)?;
        out.put(handle)
    }

    fn set_tick_every(me as Mod, handle: TickHandle, every_nanos: u64) -> Status {
        Ok(crate::engine::ticks::TICKS.set_every(handle, me.index, every_nanos))
    }
}
