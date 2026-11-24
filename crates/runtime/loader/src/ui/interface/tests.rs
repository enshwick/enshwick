use super::*;

#[test]
fn the_arena_turns_once_a_frame_whichever_hook_is_wired() {
    assert!(!hud_turns_frames(true));
    assert!(hud_turns_frames(false));
}

#[test]
fn a_hud_that_cannot_link_is_off_for_its_draw_too() {
    static HUD: Fuse = Fuse::new("test HUD");
    let loader = crate::start::tests::bare_loader();
    assert!(!link_hud(&loader, &HUD));
    assert!(
        HUD.is_off(),
        "the draw runs under the feature whose link failed"
    );
    let listed = crate::engine::diag::parts_off();
    assert!(listed.iter().any(|p| p.part == HUD.feature()), "{listed:?}");
}

#[test]
fn item_slots_list_off_only_when_a_mod_asks_and_turn_off_with_the_interface() {
    let loader = crate::start::tests::bare_loader();
    item_slots::link(&loader, false);
    assert!(
        !item_slots::SLOTS_FUSE.is_off() && !item_slots::HOVER_FUSE.is_off(),
        "a build without the slots and no mod asking lists nothing off"
    );
    parts_off_with_interface();
    for fuse in [
        &item_slots::SLOTS_FUSE,
        &item_slots::HOVER_FUSE,
        &tooltips::FUSE,
    ] {
        assert!(
            fuse.is_off(),
            "{} is off with the interface",
            fuse.feature()
        );
    }
}

#[test]
fn each_optional_part_turns_off_the_feature_its_work_runs_under() {
    let same = |a: &Fuse, b: &Fuse| core::ptr::from_ref(a) == core::ptr::from_ref(b);
    for (part, fuse) in Part::ALL.iter().zip(PART_FUSES) {
        assert_eq!(fuse.feature(), part.name, "in the order of Part::ALL");
        assert!(
            !same(fuse, Fuse::named(part.name)),
            "{} has a feature of its own, not one found by name",
            part.name
        );
    }
    assert!(same(PART_FUSES[2], &HUD_FUSE));
}

#[test]
fn the_mods_screen_shows_only_while_the_interface_its_entry_and_itself_are_on() {
    let fuses = || {
        [
            Fuse::new("test interface"),
            Fuse::new("test entry"),
            Fuse::new("test screen"),
        ]
    };
    let [interface, entry, screen] = fuses();
    assert!(mods::screen_drawn(&interface, &entry, &screen));
    for off in 0..3 {
        let all = fuses();
        all[off].blow("a test");
        let [interface, entry, screen] = &all;
        assert!(!mods::screen_drawn(interface, entry, screen), "{off} off");
    }
}

#[test]
fn a_command_opens_the_mods_screen_only_while_it_and_one_menu_entry_are_on() {
    let fuses = || ["interface", "main", "pause", "screen"].map(Fuse::new);
    let opens = |[interface, main, pause, screen]: &[Fuse; 4]| {
        mods::opens_with(interface, [main, pause], screen)
    };
    let all = fuses();
    assert!(opens(&all));
    all[1].blow("a test");
    assert!(opens(&all), "over the pause menu");
    all[2].blow("a test");
    assert!(!opens(&all), "neither menu's entry");
    for off in [0, 3] {
        let all = fuses();
        all[off].blow("a test");
        assert!(!opens(&all), "{off} off");
    }
}

#[test]
fn a_command_s_page_opens_only_over_a_menu_of_this_frame_and_only_in_its_frame() {
    use crate::actions::client_commands::ModsAsk;
    // The menu's hook ran in this frame, before the draw counts it.
    assert!(mods::drew_before_draw(40, 40));
    assert!(
        !mods::drew_before_draw(39, 40),
        "the pause menu closed this frame: its draw closes the screen"
    );
    assert!(!mods::drew_before_draw(0, 0), "no menu ever drew");
    let ask = ModsAsk {
        mod_index: Some(2),
        gear: true,
    };
    assert_eq!(mods::asked_in(Some((ask, 40)), 41), Some(ask));
    assert_eq!(
        mods::asked_in(Some((ask, 40)), 90),
        None,
        "an ask whose frame did not draw the screen never opens it later"
    );
}

#[test]
fn dialogs_draw_in_game_only_while_the_interface_and_its_dialogs_are_on() {
    let (interface, dialogs) = (Fuse::new("test interface"), Fuse::new("test dialogs"));
    assert!(off::both_on(&interface, &dialogs));
    dialogs.blow("a test");
    assert!(!off::both_on(&interface, &dialogs));
    let (interface, dialogs) = (Fuse::new("test interface"), Fuse::new("test dialogs"));
    interface.blow("a test");
    assert!(
        !off::both_on(&interface, &dialogs),
        "the interface draws them"
    );
}

#[test]
fn a_link_runs_only_while_wired_and_its_feature_on() {
    // SAFETY: the slot is this test's own and never called; `PopupFn` is a
    // pointer-sized function pointer type.
    static WIRED: Next<PopupFn> = unsafe { Next::new() };
    let feature = Fuse::new("test popup");
    assert!(!runs(&WIRED, &feature), "not wired");
    WIRED.slot().store(0x1000, Ordering::Release);
    assert!(runs(&WIRED, &feature));
    feature.blow("a test");
    assert!(!runs(&WIRED, &feature), "wired, but its feature is off");
}

#[test]
fn app_states_map_to_screens() {
    assert_eq!(screen(7), UiFrame::IN_GAME);
    assert_eq!(screen(6), UiFrame::LOADING);
    assert_eq!(screen(1), UiFrame::MENU);
    assert_eq!(screen(0), UiFrame::MENU);
}
