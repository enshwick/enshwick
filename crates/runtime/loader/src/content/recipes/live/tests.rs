use super::*;

#[test]
fn a_list_is_made_once_per_generation_and_kept_until_the_next() {
    let recipes = Recipes::new();
    let (first, next) = (1, 2);
    let key = (7, 0x1000, 3);
    let made = recipes.cached(key, first, || Some(Arc::from([1_u64, 2])));
    let again = recipes.cached(key, first, || panic!("made twice in one generation"));
    assert!(Arc::ptr_eq(made.as_ref().unwrap(), again.as_ref().unwrap()));
    let none = recipes.cached((7, 0x2000, 3), first, || None);
    assert_eq!(none, None);
    assert_eq!(
        recipes.cached((7, 0x2000, 3), first, || panic!("made twice")),
        None
    );
    let renewed = recipes.cached(key, next, || Some(Arc::from([1_u64])));
    assert_eq!(renewed.as_deref(), Some(&[1_u64][..]));
    assert_eq!(
        made.as_deref(),
        Some(&[1_u64, 2][..]),
        "the old list lives on"
    );
}

#[test]
fn a_mod_s_rule_from_init_is_kept_until_the_layout_resolves_and_shows_off_without_it() {
    let recipes = Recipes::new();
    let owner = ModIndex::new(3);
    assert_eq!(
        recipes.rule(owner, Kind::Output, "a-guid", true),
        Status::OK
    );
    assert!(recipes.live(|l| l.mods.contains(&(3, Kind::Output, "a-guid".to_owned()))));
    let loader = crate::start::tests::bare_loader();
    assert!(!recipes.keep_layout(&loader, Err("no recipe layout")));
    assert!(
        DISABLED_FUSE.is_off(),
        "off by its fuse, not only in the log"
    );
    assert!(
        crate::engine::diag::parts_off().iter().any(|off| {
            off.part == DISABLED_FUSE.feature() && off.reason.contains("no recipe layout")
        }),
        "the rules kept from init show as off"
    );
    assert_eq!(
        recipes.rule(owner, Kind::Output, "a-guid", true),
        Status::MISSING,
        "a build without the layout refuses"
    );
}

#[test]
fn a_mod_s_rule_with_a_blank_name_or_an_unknown_kind_is_refused() {
    let owner = ModIndex::new(3);
    assert_eq!(
        rule(owner, RecipeRuleKind::OUTPUT, "  ", true),
        Status::BAD_ARGS
    );
    assert_eq!(
        rule(owner, RecipeRuleKind(u32::MAX), "a-guid", true),
        Status::BAD_ARGS
    );
}

#[test]
fn the_end_of_a_hosted_game_forgets_its_server_s_recipes_and_applies_the_client_s_own() {
    let recipes = Recipes::new();
    recipes.dirty.store(false, Ordering::Relaxed);
    recipes.live(|l| {
        l.server = Some(vec![5]);
        assert!(l.incoming.add(true, false, [6].into_iter()).is_none());
    });
    recipes.forget();
    recipes.live(|l| {
        assert_eq!(l.server, None);
        assert_eq!(
            l.incoming.add(false, true, [7].into_iter()),
            Some(vec![6, 7]),
            "a list a joined server is sending arrives whole"
        );
    });
    assert!(
        recipes.dirty.load(Ordering::Acquire),
        "applied at the next poll"
    );
}

#[test]
fn a_loaded_world_logs_its_rules_problems_anew_and_the_main_menu_does_not() {
    let recipes = Recipes::new();
    let problem = || {
        recipes.live(|l| l.problems = vec!["mod x: unknown item".to_owned()]);
    };
    problem();
    recipes.world_changed(true, false);
    recipes.live(|l| {
        assert_eq!(l.problems.len(), 1, "a stopped world logs nothing again");
        l.server = Some(vec![5]);
    });
    recipes.world_changed(true, true);
    recipes.live(|l| {
        assert!(l.problems.is_empty(), "the next world logs its own");
        assert_eq!(l.server, None, "a client hosting runs its own rules");
    });
    problem();
    recipes.live(|l| l.server = Some(vec![5]));
    recipes.world_changed(false, true);
    recipes.live(|l| assert_eq!(l.server, Some(vec![5]), "a server keeps none"));
}

#[test]
fn a_registry_that_does_not_read_turns_the_rules_in_force_off_with_the_feature() {
    let recipes = Recipes::new();
    recipes.disabled.store(Arc::new(Disabled {
        ids: [5].into_iter().collect(),
        generation: 1,
    }));
    assert!(recipes.is_disabled(5));
    let loader = crate::start::tests::bare_loader();
    recipes.unreadable(&loader, "the served recipe registry");
    assert!(DISABLED_FUSE.is_off());
    assert!(
        !recipes.is_disabled(5),
        "an off feature disables nothing, not the last world's rules"
    );
}

#[test]
fn a_rule_from_the_console_needs_a_running_world() {
    assert_eq!(world_path(), None);
    let answer = change(
        &Index::default(),
        &Names::default(),
        Kind::Workshop,
        super::super::HAND,
        true,
    );
    assert_eq!(answer, texts::fill("recipes.no_world", &[]));
}
