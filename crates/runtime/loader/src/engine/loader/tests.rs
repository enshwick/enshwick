use super::*;

type Lit = unsafe extern "system" fn(u32) -> u32;
// SAFETY: `Lit` stands for the hooked function; each slot is the `next` of
// one link of the test below, which never arms, so no detour calls `get`.
static FIRST_NEXT: Next<Lit> = unsafe { Next::new() };
// SAFETY: as `FIRST_NEXT`'s, for the second link.
static SECOND_NEXT: Next<Lit> = unsafe { Next::new() };
// SAFETY: as `FIRST_NEXT`'s, for the third link.
static THIRD_NEXT: Next<Lit> = unsafe { Next::new() };

// SAFETY: as `FIRST_NEXT`'s, for the link of the rewire test, which wires
// the chain by hand and never calls the detour.
static WICK_NEXT: Next<Lit> = unsafe { Next::new() };

// SAFETY: as `WICK_NEXT`'s, for the armed link of the arming test.
static LAMP_NEXT: Next<Lit> = unsafe { Next::new() };
// SAFETY: as `WICK_NEXT`'s, for the link of the arming test whose target
// fails to arm.
static GLOW_NEXT: Next<Lit> = unsafe { Next::new() };

unsafe extern "system" fn lit(glow: u32) -> u32 {
    glow
}

// SAFETY: as `WICK_NEXT`'s, for the link of the shared name test.
static EMBER_NEXT: Next<Lit> = unsafe { Next::new() };

#[test]
fn a_panic_under_another_fuse_of_a_link_s_name_leaves_the_link_wired() {
    static EMBERS: Fuse = Fuse::new("test embers");
    let loader = crate::start::tests::bare_loader();
    let target = Target::Found("ember.light", Some(0x7000));
    assert!(loader.link_feature(EMBERS.feature(), target, &EMBER_NEXT, lit, HookFlags::NONE));
    let head: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
    {
        let mut chains = loader.chains.relock();
        chains.seal();
        for chain in chains.iter_mut().filter(|c| c.target == 0x7000) {
            chain.head = Some(head);
            chain.original = 0xff;
            chain.wire(|_| false);
        }
    }
    EMBERS.blow("a test");
    loader.rewire();
    assert!(!Fuse::named(EMBERS.feature()).is_off());
    assert_eq!(
        head.load(Ordering::Acquire),
        Next::<Lit>::address_of(lit),
        "a name shared by two parts couples neither"
    );
}

#[test]
fn a_feature_whose_target_fails_to_arm_is_wired_past_on_its_other_targets() {
    let loader = crate::start::tests::bare_loader();
    let feature = "test lamps";
    let link = |id, at, next: &'static Next<Lit>| {
        loader.link_feature(
            feature,
            Target::Found(id, Some(at)),
            next,
            lit,
            HookFlags::NONE,
        )
    };
    assert!(link("lamp.light", 0x5000, &LAMP_NEXT));
    assert!(link("lamp.glow", 0x6000, &GLOW_NEXT));
    let head: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
    {
        let mut chains = loader.chains.relock();
        chains.seal();
        for chain in chains.iter_mut().filter(|c| c.target == 0x5000) {
            chain.head = Some(head);
            chain.original = 0xff;
            chain.wire(|_| false);
        }
    }
    assert_eq!(head.load(Ordering::Acquire), Next::<Lit>::address_of(lit));
    loader.unarmed(0x6000, "lamp.glow", "could not be armed: a test");
    loader.rewire();
    assert!(Fuse::named(feature).is_off());
    assert_eq!(
        head.load(Ordering::Acquire),
        0xff,
        "the armed half runs vanilla too"
    );
    let listed = crate::engine::diag::parts_off();
    assert!(listed.iter().any(|p| p.part == feature), "{listed:?}");
}

#[test]
fn a_feature_that_turns_off_after_arming_is_wired_past_by_the_loader() {
    let loader = crate::start::tests::bare_loader();
    let feature = "test wicks";
    let target = Target::Found("wick.light", Some(0x4000));
    assert!(loader.link_feature(feature, target, &WICK_NEXT, lit, HookFlags::NONE));
    let head: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
    {
        let mut chains = loader.chains.relock();
        chains.seal();
        for chain in chains.iter_mut() {
            chain.head = Some(head);
            chain.original = 0xff;
            chain.wire(|_| false);
        }
    }
    assert_eq!(head.load(Ordering::Acquire), Next::<Lit>::address_of(lit));
    Fuse::named(feature).blow("a test");
    loader.rewire();
    assert_eq!(
        head.load(Ordering::Acquire),
        0xff,
        "the game runs it vanilla"
    );
}

#[test]
fn a_feature_whose_second_mid_hook_fails_is_wholly_off_and_its_first_address_never_hooked() {
    unsafe extern "system" fn handler(
        _registers: *mut enshwick_core::abi::Registers,
        _user: *mut core::ffi::c_void,
    ) {
    }
    static TORCHES: Fuse = Fuse::new("test torches");
    let loader = crate::start::tests::bare_loader();
    let link = |id, at| loader.link_mid(&TORCHES, Target::Found(id, at), handler, HookFlags::NONE);
    assert!(link("torch.flame", Some(0x1000)));
    assert_eq!(loader.mids.relock().addresses(), [0x1000]);
    assert!(!link("torch.smoke", None));
    assert!(TORCHES.is_off());
    assert!(
        !link("torch.ash", Some(0x3000)),
        "a feature that is off links nothing more"
    );
    let mut mids = loader.mids.relock();
    mids.seal();
    assert_eq!(mids.len(), 0, "the first hook went with its feature");
}

#[test]
fn a_feature_whose_mid_hook_fails_to_arm_is_wholly_off_with_its_other_sites() {
    unsafe extern "system" fn handler(
        _registers: *mut enshwick_core::abi::Registers,
        _user: *mut core::ffi::c_void,
    ) {
    }
    static CANDLES: Fuse = Fuse::new("test candles");
    let loader = crate::start::tests::bare_loader();
    let link = |id, at| loader.link_mid(&CANDLES, Target::Found(id, at), handler, HookFlags::NONE);
    assert!(link("candle.wick", Some(0x1000)));
    assert!(link("candle.wax", Some(0x2000)));
    loader.mids.relock().seal();
    loader.unarmed(0x2000, "candle.wax", "could not be armed: a test");
    assert!(
        CANDLES.is_off(),
        "the wick's site is passed by dispatch from now on"
    );
}

#[test]
fn a_feature_whose_second_link_fails_is_wholly_off_and_its_first_target_never_armed() {
    let loader = crate::start::tests::bare_loader();
    let feature = "test lanterns";
    let link = |id, at, next: &'static Next<Lit>| {
        loader.link_feature(feature, Target::Found(id, at), next, lit, HookFlags::NONE)
    };
    assert!(link("lantern.light", Some(0x1000), &FIRST_NEXT));
    assert!(!link("lantern.glow", None, &SECOND_NEXT));
    assert!(
        !link("lantern.wick", Some(0x3000), &THIRD_NEXT),
        "a feature that is off links nothing more"
    );
    assert!(Fuse::named(feature).is_off());
    let mut chains = loader.chains.relock();
    chains.seal();
    assert!(
        chains.is_empty(),
        "the first link went with its feature, so its target is never patched"
    );
    let listed = crate::engine::diag::parts_off();
    let off = listed.iter().find(|p| p.part == feature);
    assert!(
        off.is_some_and(|p| p.reason.starts_with("lantern.glow ")),
        "{listed:?}"
    );
}

#[test]
fn a_raise_walks_its_event_s_subscriptions_in_their_order() {
    unsafe extern "system" fn handler(
        _: Event,
        _: *const core::ffi::c_void,
        _: *mut core::ffi::c_void,
    ) {
    }
    let made = |owner, event| Subscription {
        owner,
        event,
        callback: handler,
        user: 0,
    };
    let mut sealed = vec![
        made(ModIndex::new(2), Event::TICKED),
        made(ModIndex::new(0), Event::WORLD_SAVED),
        made(ModIndex::new(1), Event::TICKED),
        made(ModIndex::new(0), Event::TICKED),
        made(ModIndex::new(1), Event::SETTINGS_CHANGED),
    ];
    sealed.sort_by_key(|s| s.event.0);
    let owners = |event| {
        of_event(&sealed, event)
            .iter()
            .map(|s| s.owner)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        owners(Event::TICKED),
        [ModIndex::new(2), ModIndex::new(1), ModIndex::new(0)]
    );
    assert_eq!(owners(Event::WORLD_SAVED), [ModIndex::new(0)]);
    assert_eq!(owners(Event::SETTINGS_CHANGED), [ModIndex::new(1)]);
    assert!(owners(Event::SHUTDOWN_STARTED).is_empty());
}

#[test]
fn a_poisoned_lock_is_taken_over_with_its_state() {
    let mutex = std::sync::Arc::new(Mutex::new(vec![1]));
    let held = std::sync::Arc::clone(&mutex);
    let panicked = std::thread::spawn(move || {
        let mut guard = held.lock().unwrap();
        guard.push(2);
        panic!("poisons the lock");
    })
    .join();
    assert!(panicked.is_err() && mutex.is_poisoned());
    mutex.relock().push(3);
    assert!(!mutex.is_poisoned());
    assert_eq!(*mutex.relock(), [1, 2, 3]);
}

/// The world events each follower of [`FOLLOWING`] heard, a bit per event.
static HEARD: AtomicU64 = AtomicU64::new(0);
/// Whether [`FOLLOWING`]'s part says it follows the world.
static FOLLOWS: AtomicBool = AtomicBool::new(false);

fn heard(_: &Loader, event: Event) {
    HEARD.fetch_or(1 << event.0, Ordering::Relaxed);
}

static FOLLOWING: &[&crate::engine::parts::Part] = &[
    &crate::engine::parts::Part {
        world_event: Some(crate::engine::parts::fused!(
            "test world panics",
            |_, event| assert!(event != Event::WORLD_LOADED, "a corrupt file")
        )),
        ..crate::engine::parts::part!("panicking")
    },
    &crate::engine::parts::Part {
        world_event: Some(crate::engine::parts::fused!(
            "test world follower",
            |loader, event| heard(loader, event)
        )),
        follows: Some(|_| FOLLOWS.load(Ordering::Relaxed)),
        ..crate::engine::parts::part!("following")
    },
];
static LAYERS: &[(&str, &[&crate::engine::parts::Part])] = &[("test", FOLLOWING)];

fn following_loader() -> Loader {
    let mut loader = crate::start::tests::bare_loader();
    loader.parts = crate::engine::parts::Parts::new(LAYERS);
    loader
}

#[test]
fn the_parts_that_follow_the_world_hear_its_three_events_though_one_before_them_panics() {
    let loader = following_loader();
    for event in [
        Event::WORLD_LOADED,
        Event::WORLD_SAVED,
        Event::SHUTDOWN_STARTED,
        Event::TICKED,
    ] {
        loader.emit(event, None, core::ptr::null());
    }
    loader.emit(Event::PLAYER_JOINED, None, core::ptr::null());
    loader.emit(Event::WORLD_SAVED, Some(0), core::ptr::null());
    let wanted = (1 << Event::WORLD_LOADED.0)
        | (1 << Event::WORLD_SAVED.0)
        | (1 << Event::SHUTDOWN_STARTED.0);
    assert_eq!(HEARD.load(Ordering::Relaxed), wanted);
}

#[test]
fn a_part_that_follows_the_world_has_its_three_events_linked_and_no_other() {
    let loader = following_loader();
    assert!(
        !loader.subscribed(Event::WORLD_LOADED),
        "it follows nothing yet"
    );
    FOLLOWS.store(true, Ordering::Relaxed);
    for event in [
        Event::WORLD_LOADED,
        Event::WORLD_SAVED,
        Event::SHUTDOWN_STARTED,
    ] {
        assert!(loader.subscribed(event), "{event:?}");
    }
    assert!(!loader.subscribed(Event::TICKED));
    assert!(!loader.subscribed(Event::PLAYER_LEFT));
}
