//! The fake host refuses the hooks, code patches, extension points and rates
//! the loader refuses, with the same status, so a mod's test fails where the
//! game would.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use core::sync::atomic::AtomicUsize;
    use std::sync::{Mutex, PoisonError};

    use enshwick::abi::{
        Header, HookFlags, HookSpec, HostApi, Layout, ModHandle, PointFlags, PointHandle,
        PointLink, PointMode, PointSpec, PointVerdict, Status, Str,
    };
    #[cfg(feature = "unstable")]
    use enshwick::rates::{ExperienceGroup, Rate};
    use enshwick::test::{FakeHost, Installed, address};
    use enshwick::{Load, Mod, Off};

    /// Code the fake hands out as the game's `.text`.
    static CODE: [u8; 16] = [0x90; 16];

    static FIRST: AtomicUsize = AtomicUsize::new(0);
    static SECOND: AtomicUsize = AtomicUsize::new(0);
    static THIRD: AtomicUsize = AtomicUsize::new(0);
    static FOURTH: AtomicUsize = AtomicUsize::new(0);
    static FIFTH: AtomicUsize = AtomicUsize::new(0);

    /// What each request during init answered, by name.
    static ANSWERS: Mutex<Vec<(&'static str, Status)>> = Mutex::new(Vec::new());

    extern "system" fn detour() {}

    fn detour_address() -> usize {
        usize::try_from(address(detour as extern "system" fn())).unwrap_or(usize::MAX)
    }

    extern "system" fn link(_: *const c_void, _: *mut c_void, _: *mut c_void) -> PointVerdict {
        PointVerdict::PASS
    }

    fn hook_spec(target: u64, detour: usize, next: &'static AtomicUsize) -> HookSpec {
        HookSpec {
            target,
            detour,
            next: &raw const *next,
            priority: 0,
            flags: HookFlags::NONE,
        }
    }

    const EMPTY_LAYOUT: Layout = Layout {
        fields: core::ptr::null(),
        count: 0,
        size: 0,
    };

    fn point_spec(name: &'static str) -> PointSpec {
        PointSpec {
            header: Header::of::<PointSpec>(),
            name: Str::new(name),
            mode: PointMode::OBSERVE,
            flags: PointFlags::NONE,
            input: EMPTY_LAYOUT,
            output: EMPTY_LAYOUT,
        }
    }

    fn code_at(offset: usize) -> u64 {
        CODE.as_ptr().addr() as u64 + offset as u64
    }

    /// Asks the table for each refused request, as raw calls do.
    fn ask(api: &HostApi, me: ModHandle) -> Vec<(&'static str, Status)> {
        let detour = detour_address();
        let hook = |spec: HookSpec| (api.hook)(me, &raw const spec);
        let patch = |address: u64, len: usize| {
            let bytes = [0x90u8; 8];
            let with = [0xCCu8; 8];
            (api.patch_code)(me, address, bytes.as_ptr(), with.as_ptr(), len)
        };
        let define = |name: &'static str| {
            let spec = point_spec(name);
            let mut out = PointHandle::EMPTY;
            (api.define_point)(me, &raw const spec, &raw mut out)
        };
        let null_next = HookSpec {
            next: core::ptr::null(),
            ..hook_spec(0x1000, detour, &FIRST)
        };
        let exclusive = HookSpec {
            flags: HookFlags::EXCLUSIVE,
            ..hook_spec(0x5000, detour, &FIFTH)
        };
        let answers = vec![
            ("hook without a slot", hook(null_next)),
            ("hook at address 0", hook(hook_spec(0, detour, &FIRST))),
            ("hook without a detour", hook(hook_spec(0x1000, 0, &FIRST))),
            ("first hook", hook(hook_spec(0x1000, detour, &FIRST))),
            (
                "one slot on a second target",
                hook(hook_spec(0x2000, detour, &FIRST)),
            ),
            (
                "a second hook on one target",
                hook(hook_spec(0x1000, detour, &SECOND)),
            ),
            (
                "hook on another target",
                hook(hook_spec(0x3000, detour, &THIRD)),
            ),
            ("exclusive hook", hook(exclusive)),
            ("patch outside the code", patch(0x10, 4)),
            ("patch past the code's end", patch(code_at(14), 4)),
            ("patch of no bytes", patch(code_at(0), 0)),
            ("first patch", patch(code_at(0), 4)),
            ("patch over another", patch(code_at(2), 4)),
            ("patch beside another", patch(code_at(4), 4)),
            ("point of another mod", define("other.mod:limit")),
            ("point without a name", define("you.refusals:")),
            ("own point", define("you.refusals:limit")),
            ("point under a former id", define("you.before:limit")),
        ];
        [answers, ask_rates(api, me)].concat()
    }

    #[cfg(feature = "unstable")]
    fn ask_rates(api: &HostApi, me: ModHandle) -> Vec<(&'static str, Status)> {
        vec![(
            "rate clients see, from a mod without join",
            (api.rate)(me, Rate::SkillPoints.id()),
        )]
    }

    #[cfg(not(feature = "unstable"))]
    fn ask_rates(_api: &HostApi, _me: ModHandle) -> Vec<(&'static str, Status)> {
        Vec::new()
    }

    struct Refusals;

    impl Mod for Refusals {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(
                id: "you.refusals",
                side: SERVER,
                was: ["you.before"],
            )
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let host = load.host();
            let answers = ask(host.api(), host.handle());
            *ANSWERS.lock().unwrap_or_else(PoisonError::into_inner) = answers;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Refusals);

    fn started() -> Installed {
        let loader = FakeHost::new().code(&CODE).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Refusals)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn answer(name: &str) -> Status {
        ANSWERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(asked, _)| *asked == name)
            .map_or_else(|| panic!("{name} was not asked"), |(_, status)| *status)
    }

    #[test]
    fn a_hook_without_a_slot_a_target_or_a_detour_is_a_bad_request() {
        let _loader = started();
        assert_eq!(answer("hook without a slot"), Status::BAD_ARGS);
        assert_eq!(answer("hook at address 0"), Status::BAD_ARGS);
        assert_eq!(answer("hook without a detour"), Status::BAD_ARGS);
        assert_eq!(answer("first hook"), Status::OK);
        assert_eq!(answer("hook on another target"), Status::OK);
    }

    #[test]
    fn a_slot_wired_twice_and_a_second_hook_on_one_target_are_refused() {
        let loader = started();
        assert_eq!(answer("one slot on a second target"), Status::REFUSED);
        assert_eq!(answer("a second hook on one target"), Status::REFUSED);
        let targets: Vec<u64> = loader.hooks().iter().map(|h| h.target).collect();
        assert_eq!(targets, [0x1000, 0x3000, 0x5000]);
    }

    #[test]
    fn hooks_after_arming_come_too_late() {
        let loader = started();
        let spec = hook_spec(0x4000, detour_address(), &FOURTH);
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        assert_eq!(
            (api.hook)(loader.handle(), &raw const spec),
            Status::TOO_LATE
        );
    }

    #[test]
    fn a_patch_outside_the_code_of_no_bytes_or_over_another_is_refused() {
        let loader = started();
        assert_eq!(answer("patch outside the code"), Status::BAD_ARGS);
        assert_eq!(answer("patch past the code's end"), Status::BAD_ARGS);
        assert_eq!(answer("patch of no bytes"), Status::BAD_ARGS);
        assert_eq!(answer("first patch"), Status::OK);
        assert_eq!(answer("patch over another"), Status::REFUSED);
        assert_eq!(answer("patch beside another"), Status::OK);
        assert_eq!(loader.code_patches().len(), 2);
    }

    #[test]
    fn a_patch_after_arming_comes_too_late() {
        let loader = started();
        let bytes = [0x90u8; 4];
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let status = (api.patch_code)(
            loader.handle(),
            code_at(8),
            bytes.as_ptr(),
            bytes.as_ptr(),
            bytes.len(),
        );
        assert_eq!(status, Status::TOO_LATE);
    }

    #[test]
    fn a_point_is_named_after_the_mod_or_a_former_id() {
        let loader = started();
        assert_eq!(answer("point of another mod"), Status::REFUSED);
        assert_eq!(answer("point without a name"), Status::REFUSED);
        assert_eq!(answer("own point"), Status::OK);
        assert_eq!(answer("point under a former id"), Status::OK);
        assert_eq!(
            loader.defined_points(),
            ["you.refusals:limit", "you.before:limit"]
        );
    }

    #[test]
    fn points_defined_or_attached_after_init_come_too_late() {
        let loader = started();
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let spec = point_spec("you.refusals:later");
        let mut out = PointHandle::EMPTY;
        let defined = (api.define_point)(loader.handle(), &raw const spec, &raw mut out);
        assert_eq!(defined, Status::TOO_LATE);
        let point = point_spec("other.mod:limit");
        let attach = PointLink {
            header: Header::of::<PointLink>(),
            point: &raw const point,
            before: core::ptr::null(),
            after: core::ptr::null(),
            before_count: 0,
            after_count: 0,
            priority: 0,
        };
        let attached = (api.attach_point)(
            loader.handle(),
            &raw const attach,
            link,
            core::ptr::null_mut(),
        );
        assert_eq!(attached, Status::TOO_LATE);
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn a_rate_clients_see_needs_a_mod_that_joins() {
        let _loader = started();
        assert_eq!(
            answer("rate clients see, from a mod without join"),
            Status::REFUSED
        );
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn a_rate_declared_after_init_comes_too_late() {
        let loader = started();
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let rate = Rate::Experience(ExperienceGroup::Combat).id();
        assert_eq!((api.rate)(loader.handle(), rate), Status::TOO_LATE);
    }
}
