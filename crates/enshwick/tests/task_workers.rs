//! Capping the task workers: no cap is a no-op that says so, a cap reaches
//! the loader, and only the dedicated server takes it.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use enshwick::abi::{Side, Status};
    use enshwick::pacing::TaskWorkers;
    use enshwick::{Load, Mod, Off};

    static CAPS: Mutex<TaskWorkers> = Mutex::new(TaskWorkers::at_most(0));
    static CAPPED: AtomicBool = AtomicBool::new(false);

    struct Workers;

    impl Mod for Workers {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.workers", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let caps = *CAPS.lock().map_err(|_| Off::new("poisoned"))?;
            CAPPED.store(load.cap_task_workers(caps)?, Ordering::Relaxed);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Workers);

    fn start(side: Side, caps: TaskWorkers) -> (bool, Option<(u32, u32)>) {
        let loader = enshwick::test::FakeHost::new().side(side).install();
        *CAPS.lock().unwrap() = caps;
        // SAFETY: both functions are the exports of this file's `declare_mod!(Workers)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (CAPPED.load(Ordering::Relaxed), loader.task_workers())
    }

    #[test]
    fn no_cap_links_nothing_and_says_false() {
        assert_eq!(start(Side::SERVER, TaskWorkers::at_most(0)), (false, None));
    }

    #[test]
    fn a_cap_reaches_the_loader_and_only_the_server_takes_it() {
        assert_eq!(
            start(Side::SERVER, TaskWorkers::at_most(4)),
            (true, Some((4, 4)))
        );
        assert_eq!(start(Side::CLIENT, TaskWorkers::at_most(4)), (false, None));
    }
}
