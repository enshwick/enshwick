//! A mod in C, C++ or Zig can pass a null callback where Rust cannot: every
//! registration family answers it with a bad argument and keeps nothing, as
//! the loader does.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use core::ptr::{null, null_mut};

    use enshwick::abi::{
        ActionCall, CallSpec, ChatLine, ChatRewrite, ControlSpec, Event, EventSpec, Headed,
        PointLink, Status, Str, TickHandle,
    };
    use enshwick::test::FakeHost;

    unsafe extern "system" fn chat(_: *const ChatLine, _: *mut ChatRewrite, _: *mut c_void) -> u8 {
        0
    }

    unsafe extern "system" fn action(_: *const ActionCall, _: *mut c_void, _: *mut Str) {}

    unsafe extern "system" fn event(_: Event, _: *const c_void, _: *mut c_void) {}

    /// A headed struct of zeros, its header set by the caller.
    fn blank<T: Headed>() -> T {
        // SAFETY: `Headed` requires `Zeroable`: all-zero bytes are a valid `T`.
        unsafe { core::mem::zeroed() }
    }

    #[test]
    fn a_null_filter_action_or_event_callback_is_a_bad_argument_and_a_real_one_registers() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        let me = loader.handle();
        assert_eq!((api.chat_filter)(me, None, null_mut()), Status::BAD_ARGS);
        assert_eq!((api.chat_filter)(me, Some(chat), null_mut()), Status::OK);
        assert_eq!((api.actions)(me, None, null_mut()), Status::BAD_ARGS);
        assert_eq!((api.actions)(me, Some(action), null_mut()), Status::OK);
        assert_eq!(
            (api.subscribe)(me, Event::WORLD_SAVED, None, null_mut()),
            Status::BAD_ARGS
        );
        assert_eq!(
            (api.subscribe)(me, Event::WORLD_SAVED, Some(event), null_mut()),
            Status::OK
        );
        let mut tick = TickHandle(0);
        assert_eq!(
            (api.subscribe_tick)(me, 0, None, null_mut(), &raw mut tick),
            Status::BAD_ARGS
        );
    }

    #[test]
    fn a_null_control_point_call_or_mod_event_callback_is_a_bad_argument() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        let me = loader.handle();
        let control = enshwick::abi::headed!(ControlSpec { ..blank() });
        let link = enshwick::abi::headed!(PointLink { ..blank() });
        let call = enshwick::abi::headed!(CallSpec { ..blank() });
        let spec = enshwick::abi::headed!(EventSpec { ..blank() });
        let answers = [
            (api.control)(me, &raw const control, None, null_mut()),
            (api.attach_point)(me, &raw const link, None, null_mut()),
            (api.serve_call)(me, &raw const call, None, null_mut()),
            (api.subscribe_mod_event)(me, &raw const spec, None, null_mut()),
            (api.status)(me, None, null_mut()),
            (api.serve_resource)(me, null(), null(), None, null_mut()),
        ];
        assert_eq!(answers, [Status::BAD_ARGS; 6]);
    }
}
