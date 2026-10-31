//! A lock a panicking thread held, taken and waited on through the poison
//! policy: the warning names the mod's own line, not the API's.

#[cfg(test)]
mod tests {
    use std::sync::{Condvar, Mutex, PoisonError};
    use std::time::Duration;

    use enshwick::abi::{Level, Status};
    use enshwick::{Load, Mod, Off, Relock as _, RelockWait as _};

    struct Lanterns;

    impl Mod for Lanterns {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            let lit = Mutex::new(0);
            let _panicked = std::panic::catch_unwind(|| {
                let _held = lit.lock().unwrap_or_else(PoisonError::into_inner);
                panic!("while holding");
            });
            let wake = Condvar::new();
            let guard = lit.lock().unwrap_or_else(PoisonError::into_inner);
            let (guard, _) = wake.relock_wait_timeout(guard, Duration::ZERO);
            drop(guard);
            *lit.relock() = 1;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lanterns,
        r#"
        id = "you.lanterns"
        side = "server"
        "#
    );

    #[test]
    fn a_poisoned_lock_is_logged_at_the_line_that_took_it() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let warned: Vec<String> = loader
            .logs()
            .into_iter()
            .filter(|(level, text)| *level == Level::WARN && text.contains("panicked holding"))
            .map(|(_, text)| text)
            .collect();
        assert_eq!(warned.len(), 2, "{warned:?}");
        for text in &warned {
            assert!(text.contains("poisoned_locks.rs:"), "{text}");
        }
    }
}
