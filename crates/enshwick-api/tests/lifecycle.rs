//! A mod's whole life against a fake loader: init, settings, hooks, a panic
//! inside a detour, a settings change.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

    use enshwick_api::abi::{
        BuildInfo, Event, EventFn, Header, HookSpec, HostApi, Level, ModHandle, Side, Status, Str,
    };

    static HOOKS: Mutex<Vec<(u64, usize, i32)>> = Mutex::new(Vec::new());
    static FAULTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static ORIGINAL_CALLS: AtomicU32 = AtomicU32::new(0);
    static SUBSCRIPTIONS: Mutex<Vec<(Event, EventFn, usize)>> = Mutex::new(Vec::new());
    /// Which settings file the fake loader serves: as shipped, edited, broken.
    static FILE: AtomicU8 = AtomicU8::new(0);
    static LIVE_LIMIT: AtomicU32 = AtomicU32::new(0);

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

    extern "system" fn caller_frame(out: *mut enshwick_api::abi::CallerFrame) {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = enshwick_api::abi::CallerFrame::default() };
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

    static SETTINGS_TEXTS: [&str; 3] = ["limit = 7\n", "limit = 9\n", "limit = \"x\"\n"];

    extern "system" fn settings(_me: ModHandle, _defaults: Str, out: *mut Str) -> Status {
        let text = SETTINGS_TEXTS[usize::from(FILE.load(Ordering::Relaxed))];
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = Str::new(text) };
        Status::OK
    }

    extern "system" fn subscribe(
        _me: ModHandle,
        event: Event,
        callback: EventFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        SUBSCRIPTIONS
            .lock()
            .unwrap()
            .push((event, callback, user.expose_provenance()));
        Status::OK
    }

    /// Raises `event` the way the loader does.
    fn raise(event: Event) {
        let subscriptions: Vec<_> = SUBSCRIPTIONS.lock().unwrap().clone();
        for (_, callback, user) in subscriptions.into_iter().filter(|s| s.0 == event) {
            // SAFETY: as subscribed by the safe layer.
            unsafe {
                callback(
                    event,
                    core::ptr::null(),
                    core::ptr::with_exposed_provenance_mut(user),
                );
            }
        }
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

    extern "system" fn constant(_me: ModHandle, _name: Str, out: *mut u64) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = 8 };
        Status::OK
    }

    extern "system" fn data_dir(_me: ModHandle, out: *mut Str) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = Str::new("mod-data/test-mod") };
        Status::OK
    }

    extern "system" fn log_level(_me: ModHandle) -> Level {
        Level::WARN
    }

    extern "system" fn type_hash(_t: Str, out: *mut u32) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *out = 0x027a_c564 };
        Status::OK
    }

    extern "system" fn component(
        _me: ModHandle,
        _entity: u32,
        _hash: u32,
        _out: *mut *const core::ffi::c_void,
    ) -> Status {
        Status::MISSING
    }

    extern "system" fn entity_template(_me: ModHandle, _e: u32, _out: *mut [u8; 16]) -> Status {
        Status::MISSING
    }

    extern "system" fn player(
        _me: ModHandle,
        _e: u32,
        _out: *mut enshwick_api::abi::PlayerInfo,
    ) -> Status {
        Status::MISSING
    }

    extern "system" fn players(
        _me: ModHandle,
        _out: *mut enshwick_api::abi::PlayerInfo,
        _capacity: u32,
        count: *mut u32,
    ) -> Status {
        // SAFETY: the safe layer passes a valid out pointer.
        unsafe { *count = 0 };
        Status::OK
    }

    extern "system" fn serve_resource(
        _me: ModHandle,
        _key: *const enshwick_api::abi::ResourceKey,
        _base: *const enshwick_api::abi::ResourceKey,
        _patch: enshwick_api::abi::ResourcePatchFn,
        _user: *mut core::ffi::c_void,
    ) -> Status {
        Status::OK
    }

    extern "system" fn add_text(_me: ModHandle, _id: u32, _language: u32, _text: Str) -> Status {
        Status::OK
    }

    extern "system" fn mods(
        _me: ModHandle,
        out: *mut enshwick_api::abi::ModInfo,
        entry_size: u32,
        capacity: u32,
        count: *mut u32,
    ) -> Status {
        assert_eq!(entry_size as usize, size_of::<enshwick_api::abi::ModInfo>());
        if !out.is_null() && capacity > 0 {
            // SAFETY: the safe layer passes room for `capacity` entries.
            unsafe {
                out.write(enshwick_api::abi::ModInfo {
                    header: Header::of::<enshwick_api::abi::ModInfo>(),
                    id: Str::new("enshwick"),
                    version: Str::new("0.2.0"),
                    side: Side::BOTH,
                    flags: enshwick_api::abi::ManifestFlags::NONE,
                    off: 0,
                    file: Str::new("mods/enshwick/enshwick.dll"),
                    updates: enshwick_api::abi::UpdateSource::github(
                        "themaiby/enshwick",
                        "enshwick-loader",
                    ),
                });
            }
        }
        // SAFETY: the safe layer passes a valid count.
        unsafe { count.write(1) };
        Status::OK
    }

    fn evaluated() -> u32 {
        panic!("a filtered line evaluates nothing")
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
        constant,
        data_dir,
        log_level,
        subscribe,
        type_hash,
        component,
        entity_template,
        player,
        players,
        serve_resource,
        add_text,
        mods,
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

    /// Test settings.
    #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
    struct Live {
        /// A limit.
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
            enshwick_api::debug!("{}", evaluated());
            for _ in 0..3 {
                enshwick_api::warn_once!("once");
                enshwick_api::warn_every!(std::time::Duration::from_secs(3600), "every");
            }
            load.on_config_change(|live: Live| LIVE_LIMIT.store(live.limit, Ordering::Relaxed))?;
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
        let listed = enshwick_api::mods();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].file, "mods/enshwick/enshwick.dll");
        assert_eq!(
            listed[0]
                .updates
                .as_ref()
                .and_then(enshwick_api::mods::Updates::tag_prefix),
            Some("enshwick-loader-v".to_owned())
        );
        assert_eq!(HOOKS.lock().unwrap()[0].0, 0x1_4009_0a60);
        assert_eq!(HOOKS.lock().unwrap()[0].2, -5);
        // The host keeps `warn` and above: `info` is filtered, repeats are not.
        assert_eq!(LOGS.lock().unwrap().as_slice(), ["once", "every"]);

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

        // A settings change reaches the handler; a broken file is skipped.
        let events: Vec<Event> = SUBSCRIPTIONS.lock().unwrap().iter().map(|s| s.0).collect();
        assert_eq!(
            events,
            [Event::LOADER_SETTINGS_CHANGED, Event::SETTINGS_CHANGED]
        );
        FILE.store(1, Ordering::Relaxed);
        raise(Event::SETTINGS_CHANGED);
        assert_eq!(LIVE_LIMIT.load(Ordering::Relaxed), 9);
        FILE.store(2, Ordering::Relaxed);
        raise(Event::SETTINGS_CHANGED);
        assert_eq!(LIVE_LIMIT.load(Ordering::Relaxed), 9);
        assert!(
            LOGS.lock()
                .unwrap()
                .iter()
                .any(|line| line.starts_with("settings change skipped"))
        );
        raise(Event::LOADER_SETTINGS_CHANGED);
    }
}
