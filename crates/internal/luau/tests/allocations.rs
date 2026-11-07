//! A call from a hook into a script allocates nothing once warm: the
//! arguments, the lent object, a method's arguments and a choice's name
//! cross without the heap.

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::time::Duration;

    use enshwick::abi::{DamageCheck, Header, Status};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    /// Counts the allocations of each thread, so a test counts its own.
    struct Counting;

    thread_local! {
        static MADE: Cell<u64> = const { Cell::new(0) };
    }

    fn count() {
        // A thread being torn down has no counter left; its allocations go
        // uncounted.
        let _counted = MADE.try_with(|made| made.set(made.get() + 1));
    }

    // SAFETY: every call forwards to `System` with the caller's own arguments.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count();
            // SAFETY: the caller's guarantees for `alloc` are `System`'s.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: `ptr` came from `System` through this allocator.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            count();
            // SAFETY: `ptr` came from `System` through this allocator, and the
            // caller's guarantees for `realloc` are `System`'s.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    const MAIN: &str = r#"
        enshwick.damage.on_damage(function(damage)
            if damage:cause() == "Impact" and damage:amount("MagicalFire") > 10 then
                damage:scale_type("MagicalFire", 0.5)
            end
        end)
    "#;

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            // A debug build runs slower than the default budget allows.
            let limits = Limits::default()
                .with_call(Duration::from_secs(1))
                .with_per_second(Duration::from_secs(3_600));
            let _runtime = enshwick_luau::start(
                load,
                "you.luau",
                Scripts::new().with("main.luau", MAIN),
                None,
                limits,
            )?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.luau"
        side = "server"
        "#
    );

    fn fire() -> DamageCheck {
        let mut amounts = [0.0; enshwick::abi::DAMAGE_TYPES];
        amounts[enshwick::damage::DamageTypeNew::MagicalFire.index()] = 40.0;
        DamageCheck {
            header: Header::of::<DamageCheck>(),
            cause: DamageCheck::IMPACT,
            target: enshwick::Entity(2),
            source: enshwick::Entity(3),
            attacker: enshwick::Entity(3),
            amounts,
            total: 40.0,
        }
    }

    #[test]
    fn a_warm_damage_filter_allocates_nothing_per_hit() {
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let fire_at = enshwick::damage::DamageTypeNew::MagicalFire.index();
        for _ in 0..200 {
            assert!((loader.damage(fire()).amounts[fire_at] - 20.0).abs() < 1e-6);
        }
        let before = MADE.with(Cell::get);
        for _ in 0..1_000 {
            let _hit = loader.damage(fire());
        }
        let made = MADE.with(Cell::get) - before;
        assert_eq!(made, 0, "allocations in 1000 warm hits");
    }
}
