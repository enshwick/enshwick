//! The fake reads the calling mod's handle before any entry runs, as the
//! loader does: a handle it never gave out, a mod that is off and the
//! loader's own copy each get the loader's answer.

#[cfg(test)]
mod tests {
    use enshwick::abi::{AltarInfo, HostApi, Level, ModHandle, ModInfo, Status, Str};
    use enshwick::test::{FakeHost, Installed};

    /// A handle no test names.
    const UNKNOWN: ModHandle = ModHandle(3);

    /// An init that fails, so the fake turns the mod off.
    unsafe extern "system" fn failing(_api: *const HostApi, _me: ModHandle) -> Status {
        Status::REFUSED
    }

    fn api(loader: &Installed) -> &'static HostApi {
        // SAFETY: the fake's table is a static.
        unsafe { &*loader.api() }
    }

    /// How many mods the list answers `me`, or its refusal.
    fn listed(api: &HostApi, me: ModHandle) -> Status {
        let mut count = 0;
        (api.mods)(
            me,
            core::ptr::null_mut(),
            u32::try_from(size_of::<ModInfo>()).unwrap(),
            0,
            &raw mut count,
        )
    }

    #[test]
    fn a_handle_the_loader_never_gave_out_is_a_bad_argument_to_every_kind_of_entry() {
        let loader = FakeHost::new().install();
        let api = api(&loader);
        assert_eq!((api.request_altars)(UNKNOWN), Status::BAD_ARGS, "a mod's");
        assert_eq!(listed(api, UNKNOWN), Status::BAD_ARGS, "a mod's, on or off");
        assert_eq!(
            (api.log)(UNKNOWN, Level::INFO, Str::new("hi")),
            Status::BAD_ARGS,
            "a line's writer"
        );
        let mut address = 0;
        assert_eq!(
            (api.system)(UNKNOWN, Str::new("anything"), &raw mut address),
            Status::BAD_ARGS,
            "a mod's or the loader's"
        );
        assert!(loader.logs().is_empty(), "{:?}", loader.logs());
    }

    #[test]
    fn a_mod_whose_init_failed_is_faulted_but_still_lists_the_mods_and_logs() {
        let loader = FakeHost::new().install();
        // SAFETY: `failing` takes the table and handle the fake passes.
        assert_eq!(unsafe { loader.init(failing) }, Status::REFUSED);
        assert!(loader.is_off());
        let (api, me) = (api(&loader), loader.handle());
        assert_eq!((api.request_altars)(me), Status::FAULTED);
        let mut address = 0;
        assert_eq!(
            (api.system)(me, Str::new("anything"), &raw mut address),
            Status::FAULTED
        );
        assert_eq!(listed(api, me), Status::OK);
        assert_eq!(
            (api.log)(me, Level::INFO, Str::new("still here")),
            Status::OK
        );
        assert_eq!(loader.logs(), [(Level::INFO, "still here".to_owned())]);
    }

    #[test]
    fn the_loader_s_own_copy_reads_and_logs_but_is_no_mod() {
        let loader = FakeHost::new().system("found", 0x10).install();
        let api = api(&loader);
        let mut address = 0;
        assert_eq!(
            (api.system)(ModHandle::NONE, Str::new("found"), &raw mut address),
            Status::OK
        );
        assert_eq!(address, 0x10);
        assert_eq!(
            (api.log)(ModHandle::NONE, Level::INFO, Str::new("the loader")),
            Status::OK
        );
        assert_eq!((api.request_altars)(ModHandle::NONE), Status::BAD_ARGS);
        assert_eq!(listed(api, ModHandle::NONE), Status::BAD_ARGS);
    }

    #[test]
    fn a_second_mod_the_test_names_calls_as_a_mod_of_its_own() {
        let other = ModHandle(2);
        let loader = FakeHost::new().other_mod(other).install();
        let api = api(&loader);
        assert_eq!((api.request_altars)(other), Status::OK);
        assert_eq!(listed(api, other), Status::OK);
    }

    #[test]
    fn altars_one_mod_opened_are_not_open_to_another() {
        let other = ModHandle(2);
        let loader = FakeHost::new().other_mod(other).install();
        let api = api(&loader);
        assert_eq!((api.request_altars)(loader.handle()), Status::OK);
        let mut count = 0;
        let listed = (api.altars)(
            other,
            core::ptr::null_mut(),
            u32::try_from(size_of::<AltarInfo>()).unwrap(),
            0,
            &raw mut count,
        );
        assert_eq!(listed, Status::REFUSED, "the other mod never asked");
        assert_eq!((api.altar_zones_changed)(other), Status::REFUSED);
        assert_eq!(loader.altars_opened(), (true, false));
    }
}
