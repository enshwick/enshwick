//! Capping the task workers: no cap is a no-op that says so, a cap links one
//! mid-function hook per capped group.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use enshwick::abi::{Side, Status};
    use enshwick::pacing::TaskWorkers;
    use enshwick::{Load, Mod, Off};

    /// The worker sizing as the dedicated server's code has it.
    static CODE: [u8; 44] = [
        0x41, 0x0F, 0xB6, 0x47, 0x09, 0x8B, 0xCF, 0x40, 0x3A, 0xC7, 0x0F, 0x42, 0xC8, 0x84, 0xDB,
        0x89, 0x8D, 0x00, 0x00, 0x00, 0x00, 0x8B, 0xCF, 0x0F, 0xB6, 0xC3, 0x0F, 0x45, 0xC8, 0x41,
        0x0F, 0xB6, 0x47, 0x0A, 0x44, 0x0F, 0xB6, 0xE9, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
    ];

    static CAPS: Mutex<TaskWorkers> = Mutex::new(TaskWorkers::at_most(0));
    static CAPPED: AtomicBool = AtomicBool::new(false);

    struct Workers;

    impl Mod for Workers {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "workers", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let caps = *CAPS.lock().map_err(|_| Off::new("poisoned"))?;
            CAPPED.store(load.cap_task_workers(caps)?, Ordering::Relaxed);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Workers);

    fn start(caps: TaskWorkers) -> (bool, usize) {
        *CAPS.lock().unwrap() = caps;
        let loader = enshwick::test::FakeHost::new()
            .side(Side::SERVER)
            .code(&CODE)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (CAPPED.load(Ordering::Relaxed), loader.mid_hooks().len())
    }

    #[test]
    fn no_cap_links_nothing_and_says_false() {
        assert_eq!(start(TaskWorkers::at_most(0)), (false, 0));
    }

    #[test]
    fn a_cap_links_a_hook_per_capped_group() {
        assert_eq!(start(TaskWorkers::at_most(4)), (true, 2));
    }
}
