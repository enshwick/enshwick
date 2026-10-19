//! A mod's whole life against a fake loader: init, settings, hooks, a panic
//! inside a detour.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick_api::abi::{
        BuildInfo, Header, HookSpec, HostApi, Level, ModHandle, Side, Status, Str,
    };

    static HOOKS: Mutex<Vec<(u64, usize, i32)>> = Mutex::new(Vec::new());
    static FAULTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static ORIGINAL_CALLS: AtomicU32 = AtomicU32::new(0);

    extern "system" fn build(out: *mut BuildInfo) {
        // SAFETY: the safe layer passes a valid struct.
        let out = unsafe { &mut *out };
        out.side = Side::SERVER;
        out.base = 0x1_4000_0000;
    }

    extern "system" fn symbol(_me: ModHandle, name: Str, out: *mut u64) -> Status {
        // SAFETY: the name is borrowed for the call.
        match unsafe { name.as_str() } {
            Some("npc.lookup") => {
                // SAFETY: the safe layer passes a valid out pointer.
                unsafe { *out = 0x1_4020_2e80 };
                Status::OK
            }
            _ => Status::MISSING,
        }
    }

    extern "system" fn system(_name: Str, out: *mut u64) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = 0x1_4009_0a60 };
        Status::OK
    }

    extern "system" fn field_offset(_t: Str, _f: Str, out: *mut u64) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = 4 };
        Status::OK
    }

    extern "system" fn type_size(_t: Str, out: *mut u32) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = 16 };
        Status::OK
    }

    extern "system" fn hook(_me: ModHandle, spec: *const HookSpec) -> Status {
        // SAFETY: the safe layer passes a valid spec.
        let spec = unsafe { &*spec };
        // Arm at once: the rest of the chain is the fake original.
        // SAFETY: `next` points at the hook's static slot.
        unsafe { &*spec.next }.store((original as *const ()).addr(), Ordering::Release);
        HOOKS
            .lock()
            .unwrap()
            .push((spec.target, spec.detour, spec.priority));
        Status::OK
    }

    extern "system" fn caller_frame() -> usize {
        0
    }

    extern "system" fn fault(_me: ModHandle, reason: Str) {
        // SAFETY: borrowed for the call.
        FAULTS
            .lock()
            .unwrap()
            .push(unsafe { reason.as_str() }.unwrap().to_owned());
    }

    extern "system" fn log(_me: ModHandle, _level: Level, message: Str) {
        // SAFETY: borrowed for the call.
        LOGS.lock()
            .unwrap()
            .push(unsafe { message.as_str() }.unwrap().to_owned());
    }

    static SETTINGS_TEXT: &str = "limit = 7\n";

    extern "system" fn settings(_me: ModHandle, _defaults: Str, out: *mut Str) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = Str::new(SETTINGS_TEXT) };
        Status::OK
    }

    extern "system" fn publish(
        _me: ModHandle,
        _n: Str,
        _v: u32,
        _t: *const core::ffi::c_void,
    ) -> Status {
        Status::OK
    }

    extern "system" fn service(
        _me: ModHandle,
        _n: Str,
        _v: u32,
        _out: *mut *const core::ffi::c_void,
    ) -> Status {
        Status::MISSING
    }

    static API: HostApi = HostApi {
        header: Header::of::<HostApi>(),
        host_version: Str::new("0.0.0-test"),
        build,
        symbol,
        system,
        field_offset,
        type_size,
        hook,
        caller_frame,
        fault,
        log,
        settings,
        publish,
        service,
    };

    extern "system" fn original(value: u32) -> u32 {
        ORIGINAL_CALLS.fetch_add(1, Ordering::Relaxed);
        value + 1
    }

    enshwick_api::detour! {
        static DOUBLE: extern "system" fn(value: u32) -> u32 = double, priority = -5;
    }

    fn double(next: enshwick_api::Next<extern "system" fn(u32) -> u32>, value: u32) -> u32 {
        assert_ne!(value, 13, "unlucky");
        next.get()(value) * 2
    }

    #[derive(serde::Deserialize)]
    struct Settings {
        limit: u32,
    }

    struct Test {
        limit: u32,
    }

    impl enshwick_api::Mod for Test {
        fn manifest() -> &'static enshwick_api::abi::Manifest {
            enshwick_api::manifest!(id: "test-mod", side: BOTH, join = true, requires: ["other" => "^1"])
        }

        fn init(load: &mut enshwick_api::Load<'_>) -> Result<Self, enshwick_api::Off> {
            let settings: Settings = load.settings("limit = 1\n")?;
            assert_eq!(load.symbol("npc.lookup")?, 0x1_4020_2e80);
            assert!(load.symbol("nope").is_err());
            load.hook(&DOUBLE, load.system("anything")?)?;
            enshwick_api::info!("limit {}", settings.limit);
            Ok(Self {
                limit: settings.limit,
            })
        }
    }

    enshwick_api::declare_mod!(Test);

    #[test]
    fn a_mod_lives_its_whole_life() {
        let manifest = enshwick_mod_query();
        // SAFETY: the manifest is static.
        let manifest = unsafe { &*manifest };
        // SAFETY: static strings.
        assert_eq!(unsafe { manifest.id.as_str() }, Some("test-mod"));
        assert_eq!(manifest.dependency_count, 1);
        assert_eq!(enshwick_mod_abi(), 1);

        // SAFETY: a valid table for the process.
        let status = unsafe { enshwick_mod_init(&raw const API, ModHandle(1)) };
        assert_eq!(status, Status::OK);
        assert_eq!(Test::instance().map(|t| t.limit), Some(7));
        assert_eq!(
            enshwick_api::host().map(|h| h.build().side),
            Some(Side::SERVER)
        );
        assert_eq!(HOOKS.lock().unwrap()[0].0, 0x1_4009_0a60);
        assert_eq!(HOOKS.lock().unwrap()[0].2, -5);
        assert!(LOGS.lock().unwrap().contains(&"limit 7".to_owned()));

        // The chain: the detour doubles what the original returns.
        let entry: extern "system" fn(u32) -> u32 =
            // SAFETY: the detour's address has this signature.
            unsafe { core::mem::transmute(HOOKS.lock().unwrap()[0].1) };
        assert_eq!(entry(2), 6);

        // A panic in the detour reports a fault and falls through to the original.
        let calls = ORIGINAL_CALLS.load(Ordering::Relaxed);
        assert_eq!(entry(13), 14);
        assert_eq!(ORIGINAL_CALLS.load(Ordering::Relaxed), calls + 1);
        assert_eq!(FAULTS.lock().unwrap().as_slice(), ["panic in hook DOUBLE"]);

        // SAFETY: as for init.
        unsafe { enshwick_mod_ready(&raw const API, ModHandle(1)) };
    }
}
