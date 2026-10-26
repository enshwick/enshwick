//! The engine's pacing places: the loader finds them and runs the mod's
//! handlers there.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::pacing::{self, TaskWorkers};
    use enshwick::{Load, Mod, Off};

    static WAITS: Mutex<Vec<(u32, i64)>> = Mutex::new(Vec::new());
    static FOUND: Mutex<Option<(bool, bool, bool, bool)>> = Mutex::new(None);

    struct Quiet;

    impl Mod for Quiet {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.quiet", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let waits =
                load.on_loop_wait(|wait| WAITS.lock().unwrap().push((wait.ms, wait.left_ns)))?;
            let server = load.on_server_wait(|wait_ns| {
                if *wait_ns == pacing::SERVER_IDLE_WAIT_NS {
                    *wait_ns = pacing::SERVER_ONLINE_WAIT_NS;
                }
            })?;
            let workers = load.cap_task_workers(TaskWorkers::at_most(4))?;
            *FOUND.lock().unwrap() = Some((waits.handles, waits.messages, server, workers));
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Quiet);

    #[test]
    fn the_loader_runs_the_handlers_at_every_place_the_build_has() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(*FOUND.lock().unwrap(), Some((true, false, true, true)));
        loader.loop_wait(0, 400_000);
        assert_eq!(*WAITS.lock().unwrap(), [(0, 400_000)]);
        assert_eq!(
            loader.server_wait(pacing::SERVER_IDLE_WAIT_NS),
            pacing::SERVER_ONLINE_WAIT_NS
        );
        assert_eq!(
            loader.server_wait(pacing::SERVER_SLOW_WAIT_NS),
            pacing::SERVER_SLOW_WAIT_NS
        );
        assert_eq!(loader.task_workers(), Some((4, 4)));
    }
}
