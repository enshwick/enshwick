//! Tick handlers on a period, on every step, and with a period they change
//! themselves, delivered by the fake host's steps as the loader delivers
//! them.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use enshwick::abi::{Event, ModHandle, Status, TickData, TickHandle};
    use enshwick::events::{TickPeriod, Ticked};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};

    /// Each call: which handler, the steps and the time since its last call.
    static CALLS: Mutex<Vec<(&'static str, u64, Duration)>> = Mutex::new(Vec::new());
    /// The period of the handler that changes its own.
    static OWN: Mutex<Option<TickPeriod>> = Mutex::new(None);

    const STEP: Duration = Duration::from_millis(40);

    fn record(name: &'static str) -> impl Fn(&TickData) + Send + Sync + 'static {
        move |tick| {
            CALLS
                .lock()
                .unwrap()
                .push((name, tick.elapsed_steps, tick.elapsed()));
        }
    }

    struct Ticking;

    impl Mod for Ticking {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on::<Ticked>(record("default"))?;
            load.on_every_step(record("every"))?;
            load.on_tick(Duration::ZERO, record("zero"))?;
            let own = load.on_tick(Duration::from_secs(1), |tick| {
                record("own")(tick);
                if let Some(own) = *OWN.lock().unwrap() {
                    own.set_every(Duration::from_millis(200)).unwrap();
                }
            })?;
            *OWN.lock().unwrap() = Some(own);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Ticking,
        r#"
        id = "you.ticking"
        side = "server"
        "#
    );

    extern "system" fn nothing(_: Event, _: *const core::ffi::c_void, _: *mut core::ffi::c_void) {}

    fn started() -> Installed {
        let loader = FakeHost::new().install();
        CALLS.lock().unwrap().clear();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Ticking)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The calls of handler `name` over `steps` steps of [`STEP`].
    fn calls_of(loader: &Installed, name: &str, steps: usize) -> Vec<(u64, Duration)> {
        for _ in 0..steps {
            let _ran = loader.tick(STEP);
        }
        CALLS
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.0 == name)
            .map(|c| (c.1, c.2))
            .collect()
    }

    #[test]
    fn the_default_period_skips_steps_and_sums_them_into_the_next_call() {
        let loader = started();
        assert_eq!(
            calls_of(&loader, "default", 7),
            [(1, STEP), (3, STEP * 3), (3, STEP * 3)],
            "at 40 ms, then once 100 ms passed: 160 and 280 ms"
        );
        assert_eq!(Ticked::DEFAULT_EVERY, Duration::from_millis(100));
    }

    #[test]
    fn an_every_step_handler_runs_on_each_step() {
        let loader = started();
        assert_eq!(calls_of(&loader, "every", 5), [(1, STEP); 5]);
    }

    #[test]
    fn a_period_of_zero_runs_on_each_step() {
        let loader = started();
        assert_eq!(calls_of(&loader, "zero", 5), [(1, STEP); 5]);
    }

    #[test]
    fn a_period_set_inside_the_handler_counts_from_its_call() {
        let loader = started();
        assert_eq!(
            calls_of(&loader, "own", 11),
            [(1, STEP), (5, STEP * 5), (5, STEP * 5)],
            "200 ms from the first call, not the second it asked for at init"
        );
    }

    #[test]
    fn a_tick_period_is_shared_across_threads() {
        fn shared<T: Copy + Send + Sync + 'static>() {}
        shared::<TickPeriod>();
    }

    #[test]
    fn a_raised_tick_reaches_every_tick_handler() {
        let loader = started();
        assert_eq!(loader.raise::<Ticked>(&TickData::new(1)), 4);
    }

    #[test]
    fn another_mod_s_handle_is_refused_and_an_unknown_one_missing() {
        let loader = started();
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let other = ModHandle(loader.handle().0 + 1);
        assert_eq!(
            (api.set_tick_every)(other, TickHandle(0), 0),
            Status::REFUSED
        );
        assert_eq!(
            (api.set_tick_every)(loader.handle(), TickHandle(99), 0),
            Status::MISSING
        );
        assert_eq!(
            (api.set_tick_every)(loader.handle(), TickHandle(0), 0),
            Status::OK
        );
    }

    #[test]
    fn a_tick_after_init_is_refused() {
        let loader = started();
        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let mut handle = TickHandle(0);
        let status = (api.subscribe_tick)(
            loader.handle(),
            0,
            nothing,
            core::ptr::null_mut(),
            &raw mut handle,
        );
        assert_eq!(status, Status::TOO_LATE);
    }
}
