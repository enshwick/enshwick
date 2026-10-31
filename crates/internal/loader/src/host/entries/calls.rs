//! Host functions for described services, calls.

use enshwick::abi as api_frame;
use enshwick::abi::{Level, ModHandle, Status};

use crate::host::entries::services::{required_owner, spec_parts, unexported};
use crate::host::memory::{get_headed, put};
use crate::host::{caller_index, calling, entry};

/// The declaration of a call spec read through its header.
///
/// # Safety
///
/// - The spec's name and its `input` and `output` layouts, with their
///   fields and names, are readable and unchanged for the call.
unsafe fn declared(spec: &api_frame::CallSpec) -> Option<crate::calls::Declared> {
    // SAFETY: the caller guarantees the spec's name and its `input` layout
    // readable and unchanged for the call; `spec` holds the same pointers.
    let (name, input) = unsafe { spec_parts(spec.name, &spec.input) }?;
    // SAFETY: the caller guarantees the spec's `output` layout, its fields and
    // their names, readable and unchanged for the call.
    let output = unsafe { enshwick::services::read_layout(&spec.output) }?;
    Some(crate::calls::Declared {
        name: name.to_owned(),
        major: spec.major,
        input,
        output,
        input_size: spec.input.size,
        output_size: spec.output.size,
    })
}

pub(crate) extern "system" fn serve_call(
    me: ModHandle,
    spec: *const api_frame::CallSpec,
    callback: api_frame::CallFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("serve_call", || {
        let (_, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if crate::calls::sealed() {
            return Status::TOO_LATE;
        }
        // SAFETY: a mod passes null or its version of the spec, and all of
        // the spec is required.
        let Some(spec) = (unsafe { get_headed(spec, size_of::<api_frame::CallSpec>()) }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass the spec's name and layouts, with
        // their fields and names, readable and unchanged for the call.
        let Some(declared) = (unsafe { declared(&spec) }) else {
            return Status::BAD_ARGS;
        };
        let served = crate::calls::registry().serve(
            index,
            &record.id,
            declared,
            callback,
            user.expose_provenance(),
        );
        match served {
            Ok(()) => Status::OK,
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn find_call(
    me: ModHandle,
    spec: *const api_frame::CallSpec,
    out: *mut u32,
) -> Status {
    entry("find_call", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if crate::calls::sealed() {
            return Status::TOO_LATE;
        }
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: a mod passes null or its version of the spec, and all of
        // the spec is required.
        let Some(spec) = (unsafe { get_headed(spec, size_of::<api_frame::CallSpec>()) }) else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the ABI has the mod pass the spec's name and layouts, with
        // their fields and names, readable and unchanged for the call.
        let Some(declared) = (unsafe { declared(&spec) }) else {
            return Status::BAD_ARGS;
        };
        let unmet = unexported(loader, record, "call", &declared.name).or_else(|| {
            let owner = required_owner(record, &declared.name)?;
            let reason = crate::calls::registry().unmatched(&declared, loader)?;
            Some(format!(
                "call {} v{}: this mod requires {owner}, and the call cannot match: {reason}",
                declared.name, declared.major
            ))
        });
        if let Some(reason) = unmet {
            crate::log::write(Level::WARN, &record.id, &reason);
            return Status::REFUSED;
        }
        let found = crate::calls::registry().find(index, declared);
        match found {
            // SAFETY: `HostApi::find_call` has the mod pass `out` pointing at a
            // writable, aligned `u32` that no other thread touches during the
            // call; checked non-null above.
            Ok(id) => unsafe { put(out, id) },
            Err(reason) => {
                crate::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn call(
    me: ModHandle,
    id: u32,
    input: *const core::ffi::c_void,
    output: *mut core::ffi::c_void,
) -> Status {
    entry("call", || match caller_index(me) {
        Ok((_, index)) => crate::calls::run(index.get(), id, input, output),
        Err(status) => status,
    })
}
