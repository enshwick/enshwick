//! The scheduler through the fake host: a task queued while a game runs ends
//! with that game, a task queued at init runs in every game, and a tick the
//! loader refuses leaves the mod without a scheduler.

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use enshwick::abi::Status;
    use enshwick::schedule::Scheduler;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);
    static FROM_INIT: AtomicU32 = AtomicU32::new(0);
    static FROM_GAME: AtomicU32 = AtomicU32::new(0);

    struct Timers;

    impl Mod for Timers {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scheduler = load.scheduler()?;
            scheduler.every(Duration::ZERO, || {
                FROM_INIT.fetch_add(1, Ordering::Relaxed);
                ControlFlow::Continue(())
            });
            *SCHEDULER.lock().unwrap() = Some(scheduler);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Timers,
        r#"
        id = "you.timers"
        side = "server"
        "#
    );

    const STEP: Duration = Duration::from_millis(100);

    #[test]
    fn a_scheduler_whose_tick_the_loader_refuses_turns_the_mod_off() {
        let loader = FakeHost::new().refuse_tick(0).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Timers)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_ne!(status, Status::OK);
        assert!(SCHEDULER.lock().unwrap().is_none(), "init never got one");
    }

    #[test]
    fn a_task_queued_in_one_game_never_runs_in_the_next() {
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Timers)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let scheduler = SCHEDULER.lock().unwrap().unwrap();

        let _ran = loader.tick(STEP);
        assert_eq!(FROM_INIT.load(Ordering::Relaxed), 1);
        scheduler.after(Duration::ZERO, || {
            FROM_GAME.fetch_add(1, Ordering::Relaxed);
        });
        scheduler.every(Duration::ZERO, || {
            FROM_GAME.fetch_add(100, Ordering::Relaxed);
            ControlFlow::Continue(())
        });

        loader.end_game();
        let _ran = loader.tick(STEP);
        let _ran = loader.tick(STEP);
        assert_eq!(
            FROM_GAME.load(Ordering::Relaxed),
            0,
            "the first game's tasks ended with it"
        );
        assert_eq!(
            FROM_INIT.load(Ordering::Relaxed),
            3,
            "the task from init runs in every game"
        );

        scheduler.after(Duration::ZERO, || {
            FROM_GAME.fetch_add(1, Ordering::Relaxed);
        });
        let _ran = loader.tick(STEP);
        assert_eq!(
            FROM_GAME.load(Ordering::Relaxed),
            1,
            "a task of the running game runs"
        );
    }

    #[test]
    fn a_task_queued_when_the_next_game_s_world_loads_runs_in_that_game() {
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Timers)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let scheduler = SCHEDULER.lock().unwrap().unwrap();
        let _ran = loader.tick(STEP);
        scheduler.after(Duration::from_secs(1), || {
            FROM_GAME.fetch_add(100, Ordering::Relaxed);
        });

        // The next game's world loads before any step of it, as a client's
        // hosted game does; a world load handler queues the task.
        loader.end_game();
        let _handled = loader.raise::<enshwick::events::WorldLoaded>(&());
        scheduler.after(Duration::ZERO, || {
            FROM_GAME.fetch_add(1, Ordering::Relaxed);
        });
        let _ran = loader.tick(STEP);
        let _ran = loader.tick(Duration::from_secs(1));
        assert_eq!(
            FROM_GAME.load(Ordering::Relaxed),
            1,
            "the task queued at the world's load ran, the last game's did not"
        );
    }
}
