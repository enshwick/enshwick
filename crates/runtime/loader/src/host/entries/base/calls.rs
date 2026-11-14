//! Host functions for described services, calls.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::Status;

use crate::host::entries::base::services::{exported, required_owner, spec_parts};
use crate::host::layer::guards::open;
use crate::host::layer::{Headed, Mod, Out, Rule, entries};

/// The declaration of a call spec read through its header: a bad argument
/// when its name or layouts do not read.
fn declared(spec: &api_frame::CallSpec) -> Result<crate::base::calls::Declared, Status> {
    // SAFETY: the ABI has the mod pass the spec's name and its `input`
    // layout, with its fields and names, readable and unchanged for the call.
    let (name, input) = unsafe { spec_parts(spec.name, &spec.input) }.ok_or(Status::BAD_ARGS)?;
    // SAFETY: the ABI has the mod pass the spec's `output` layout, its fields
    // and their names, readable and unchanged for the call.
    let output =
        unsafe { enshwick_core::services::read_layout(&spec.output) }.ok_or(Status::BAD_ARGS)?;
    Ok(crate::base::calls::Declared {
        name: name.to_owned(),
        major: spec.major,
        input,
        output,
        input_size: spec.input.size,
        output_size: spec.output.size,
    })
}

entries! {
    fn serve_call(
        me as Mod,
        spec as Headed<api_frame::CallSpec>,
        callback: api_frame::CallFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        open(crate::base::calls::sealed())?;
        let declared = declared(&spec)?;
        crate::base::calls::registry().serve(
            me.index,
            &me.record.id,
            declared,
            callback,
            user.expose_provenance(),
        )?;
        Ok(())
    }

    fn find_call(me as Mod, spec as Headed<api_frame::CallSpec>, out as Out<u32>) -> Status {
        open(crate::base::calls::sealed())?;
        let declared = declared(&spec)?;
        exported(me.loader, me.record, crate::base::manifest::Kind::Call, &declared.name)?;
        if let Some(owner) = required_owner(me.record, &declared.name)
            && let Some(reason) = crate::base::calls::registry().unmatched(&declared, me.loader)
        {
            return Err(Rule(format!(
                "call {} v{}: this mod requires {owner}, and the call cannot match: {reason}",
                declared.name, declared.major
            ))
            .into());
        }
        out.put(crate::base::calls::registry().find(me.index, declared)?)
    }

    fn call(
        me as Mod,
        id: u32,
        input: *const core::ffi::c_void,
        output: *mut core::ffi::c_void,
    ) -> Status {
        Ok(crate::base::calls::run(me.index.get(), id, input, output))
    }
}
