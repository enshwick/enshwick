//! A call from a hook into a script allocates nothing once warm: the
//! arguments, the lent object, a method's arguments and a choice's name
//! cross without the heap.

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::fmt::Write as _;
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

    /// The calls into the API one hit of a kind of argument makes.
    const CALLS: u64 = 10;

    /// Each kind of argument: its name, the call [`script`] makes with it,
    /// and the allocations one warm call makes. The floor: mlua allocates a
    /// counter to read a string's bytes; a list or record parameter owns its
    /// values, a buffer parameter its bytes, a function parameter its
    /// callback.
    const KINDS: [(&str, &str, u64); 6] = [
        ("scalar", "scale_whole(100, 1.5)", 0),
        ("string", r#"is_id("you.luau:a_content_id")"#, 1),
        ("list", "combine(factors)", 2),
        ("record", "start_fill(meter)", 3),
        ("bytes", "looks_like_code(bytes)", 1),
        ("function", "store:with(read)", 1),
    ];

    /// A filter that halves magical fire on impact, and, for a hit whose
    /// total is the place of one of [`KINDS`], from 1, makes [`CALLS`] of
    /// its call.
    fn script() -> String {
        let kinds = KINDS.iter().fold(String::new(), |mut kinds, (_, call, _)| {
            let _ = writeln!(kinds, "function() for _ = 1, {CALLS} do {call} end end,");
            kinds
        });
        MAIN.replace("--[[kinds]]", &kinds)
    }

    /// [`script`] without its kinds.
    const MAIN: &str = r#"
        local scale_whole = enshwick.rates.scale_whole
        local is_id = enshwick.content.is_id
        local combine = enshwick.rates.combine
        local start_fill = enshwick.content.start_fill
        local looks_like_code = enshwick.net.looks_like_code
        local store = enshwick.world_store("allocations")
        local factors = { 1.5, 2, 0.5, 1.25 }
        local meter = { template = "you.luau:pouch", max_fuel_charges = 4, start = 2 }
        local bytes = buffer.create(64)
        local read = function(value) return value end
        local kinds = {
            --[[kinds]]
        }
        enshwick.damage.on_damage(function(damage)
            local kind = kinds[damage:total()]
            if kind then
                kind()
            elseif damage:cause() == "Impact" and damage:amount("MagicalFire") > 10 then
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
                Scripts::new().with("main.luau", script()),
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

    /// A hit that makes the calls of the kind `total` names.
    fn calls_of(total: f32) -> DamageCheck {
        DamageCheck {
            cause: DamageCheck::FALL_DAMAGE,
            total,
            ..fire()
        }
    }

    #[test]
    fn a_warm_call_allocates_only_what_its_kind_of_argument_needs() {
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        for (total, (kind, _, per_call)) in (1u8..).map(f32::from).zip(KINDS) {
            for _ in 0..20 {
                let _warm = loader.damage(calls_of(total));
            }
            let before = MADE.with(Cell::get);
            for _ in 0..100 {
                let _hit = loader.damage(calls_of(total));
            }
            let made = MADE.with(Cell::get) - before;
            assert_eq!(made, per_call * 100 * CALLS, "{kind}: {:?}", loader.logs());
        }
        let fire_at = enshwick::damage::DamageTypeNew::MagicalFire.index();
        assert!(
            (loader.damage(fire()).amounts[fire_at] - 20.0).abs() < 1e-6,
            "the mod still runs: {:?}",
            loader.logs()
        );
    }
}
