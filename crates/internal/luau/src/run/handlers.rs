//! The calls out to a mod's handlers: loader and game events, mod events
//! and the points its scripts extend.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use enshwick::Relock as _;
use enshwick::points::DescribedVerdict;
use enshwick::script::EventKind;
use mlua::{Function, Lua, Value as LuaValue};

use super::{Inner, Scope};
use crate::bridge::to_lua;
use crate::calls::ServedCall;
use crate::points::{PointDesc, Read, decode, encode};
use crate::subscribe::Subscription;

impl Inner {
    pub(super) fn dispatch(&self, kind: &'static EventKind, raw: *const core::ffi::c_void) {
        if self.off.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: `start` subscribed this closure with `Load::on_raw_event`
        // for `kind`'s event alone, and the loader delivers that event's data
        // for the call with no other thread writing it, as `read` asks.
        let Some(data) = (unsafe { kind.read(raw) }) else {
            return;
        };
        self.deliver(&Subscription::Loader(kind.event()), |lua, scope| {
            to_lua(lua, scope, data.clone(), kind.data())
        });
    }

    /// Runs the handlers of the point `sub` on its `input`, the answer so
    /// far `answer`, each seeing the answer the one before left; an error
    /// turns the mod off and leaves the answer as it was.
    pub(super) fn run_point(
        &self,
        sub: &Subscription,
        desc: &PointDesc,
        input: &[u8],
        answer: &[u8],
    ) -> DescribedVerdict {
        if self.off.load(Ordering::Acquire) {
            return DescribedVerdict::Pass;
        }
        let Some(vm) = self.vm.load_full() else {
            return DescribedVerdict::Pass;
        };
        let handlers: Vec<Function> = vm
            .handlers
            .relock()
            .iter()
            .filter(|(key, _)| key == sub)
            .map(|(_, handler)| handler.clone())
            .collect();
        let input_read = Read::Struct(desc.input.clone());
        let mut value = answer.to_vec();
        let mut set = false;
        for handler in handlers {
            let outcome = self.call(&vm, &|| sub.name(), |vm| {
                let lua_input = decode(&vm.lua, input, 0, &input_read)?;
                let lua_answer = decode(&vm.lua, &value, 0, &desc.output)?;
                let result: LuaValue = handler.call((lua_input, lua_answer))?;
                if let (LuaValue::Table(t), Some(refuse)) = (&result, vm.refuse.get())
                    && t == refuse
                {
                    return Ok(None);
                }
                if result.is_nil() {
                    return Ok(Some(false));
                }
                let mut next = value.clone();
                encode(&result, &desc.output, &mut next, 0)?;
                value = next;
                Ok(Some(true))
            });
            match outcome {
                None => return DescribedVerdict::Pass,
                Some(None) => return DescribedVerdict::Refuse,
                Some(Some(changed)) => set |= changed,
            }
        }
        if set {
            DescribedVerdict::Set(value)
        } else {
            DescribedVerdict::Pass
        }
    }

    /// Runs the handler of the served call `sub` on its `input` and writes
    /// its answer to `output`; `false` fails the call: the mod is off, the
    /// handler returned nil or raised an error, which also turns it off.
    pub(super) fn serve(
        &self,
        sub: &Subscription,
        desc: &ServedCall,
        input: &[u8],
        output: &mut [u8],
    ) -> bool {
        if self.off.load(Ordering::Acquire) {
            return false;
        }
        let Some(vm) = self.vm.load_full() else {
            return false;
        };
        let Some(handler) = vm
            .handlers
            .relock()
            .iter()
            .find(|(key, _)| key == sub)
            .map(|(_, handler)| handler.clone())
        else {
            return false;
        };
        let answer = self.call(&vm, &|| sub.name(), |vm| {
            let lua_input = decode(&vm.lua, input, 0, &desc.input)?;
            let result: LuaValue = handler.call(lua_input)?;
            if result.is_nil() {
                return Ok(None);
            }
            let mut bytes = vec![0u8; output.len()];
            encode(&result, &desc.output, &mut bytes, 0)?;
            Ok(Some(bytes))
        });
        match answer.flatten() {
            Some(bytes) => {
                output.copy_from_slice(&bytes);
                true
            }
            None => false,
        }
    }

    /// Calls every handler of `sub` in the current VM with the argument
    /// `arg` makes; stops at the first that fails, which turns the mod off.
    pub(super) fn deliver(
        &self,
        sub: &Subscription,
        arg: impl Fn(&Lua, &Scope) -> mlua::Result<LuaValue>,
    ) {
        if self.off.load(Ordering::Acquire) {
            return;
        }
        let Some(vm) = self.vm.load_full() else {
            return;
        };
        let handlers: Vec<Function> = vm
            .handlers
            .relock()
            .iter()
            .filter(|(key, _)| key == sub)
            .map(|(_, handler)| handler.clone())
            .collect();
        let scope = Scope {
            inner: Weak::clone(&self.me),
            vm: Arc::downgrade(&vm),
        };
        for handler in handlers {
            let done = self.call(&vm, &|| sub.name(), |vm| {
                handler.call::<()>(arg(&vm.lua, &scope)?)
            });
            if done.is_none() {
                return;
            }
        }
    }
}
