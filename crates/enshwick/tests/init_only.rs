//! Every entry a mod may call only during its init answers too late once
//! init has ended, as the loader does, even with arguments it would take.

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use core::ptr::null_mut;

    use core::sync::atomic::AtomicUsize;
    use enshwick::abi::{
        AffixSpec, CallSpec, ControlSpec, DataChange, EntityDataFlags, Event, EventSpec, Guid,
        Headed, Header, HookFlags, HookSpec, HostApi, ItemId, LoopWaitSites, MidHookSpec,
        ModHandle, NpcId, PointHandle, PointLink, PointSpec, RateId, ResourceKey, ServiceSpec,
        Side, SkillNode, StatRef, StatSpec, Status, Str, UiExtension,
    };

    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};

    unsafe extern "system" fn never_runs() {}

    /// Code the fake hands out as the game's `.text`, for the hook.
    static CODE: [u8; 16] = [0x90; 16];

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    extern "system" fn detour() {}

    fn hook_spec() -> HookSpec {
        let detour = enshwick::test::address(detour as extern "system" fn());
        enshwick::abi::headed!(HookSpec {
            target: CODE.as_ptr().addr() as u64,
            detour: usize::try_from(detour).unwrap_or(usize::MAX),
            next: &raw const NEXT,
            priority: 0,
            flags: HookFlags::NONE,
        })
    }

    /// A callback of the type `F` that an entry takes; the entries answer
    /// too late and keep nothing, so it never runs.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "every entry takes its callback as an `Option`"
    )]
    fn callback<F: Copy>() -> Option<F> {
        const { assert!(size_of::<F>() == size_of::<unsafe extern "system" fn()>()) };
        let any: unsafe extern "system" fn() = never_runs;
        // SAFETY: `F` is one of the ABI's `unsafe extern "system" fn` types,
        // the size of any function pointer by the assertion before it; the
        // pointer is only handed over, never called.
        Some(unsafe { core::mem::transmute_copy(&any) })
    }

    /// A headed struct of zeros with its own header.
    fn spec<T: Headed>() -> T {
        // SAFETY: `Headed` requires `Zeroable`: all-zero bytes are a valid `T`.
        let mut spec: T = unsafe { core::mem::zeroed() };
        // SAFETY: a headed struct starts with its `Header`, by `Headed`'s
        // contract.
        unsafe {
            core::ptr::from_mut(&mut spec)
                .cast::<Header>()
                .write(Header::of::<T>());
        }
        spec
    }

    const KEY: ResourceKey = ResourceKey {
        guid: Guid([7; 16]),
        type_hash: 1,
        part: 0,
    };

    /// Each init-only entry a server serves, called as a mod would after
    /// its init.
    #[expect(clippy::too_many_lines, reason = "one line per init-only entry")]
    fn late_answers(api: &HostApi, me: ModHandle) -> Vec<(&'static str, Status)> {
        let user = null_mut::<c_void>();
        let (mut index, mut guid) = (0u32, Guid([0; 16]));
        let (mut stat, mut point, mut found) = (StatRef::default(), PointHandle::EMPTY, 0u8);
        let mut sites = LoopWaitSites(0);
        let id = Str::new("you.late:thing");
        vec![
            ("hook", (api.hook)(me, &hook_spec())),
            ("publish", (api.publish)(me, &spec::<ServiceSpec>())),
            (
                "subscribe",
                (api.subscribe)(me, Event::WORLD_SAVED, callback(), user),
            ),
            (
                "serve_resource",
                (api.serve_resource)(me, &KEY, &KEY, callback(), user),
            ),
            ("add_text", (api.add_text)(me, 1, 0, Str::new("text"))),
            ("rename_filter", (api.rename_filter)(me, callback(), user)),
            ("hook_at", (api.hook_at)(me, &spec::<MidHookSpec>())),
            ("request_inventory", (api.request_inventory)(me)),
            ("status", (api.status)(me, callback(), user)),
            ("request_altars", (api.request_altars)(me)),
            (
                "building_zone_filter",
                (api.building_zone_filter)(me, callback(), user),
            ),
            (
                "survivor_lookup",
                (api.survivor_lookup)(me, callback(), user),
            ),
            (
                "survivor_bed_check",
                (api.survivor_bed_check)(me, callback(), user),
            ),
            ("summon_filter", (api.summon_filter)(me, callback(), user)),
            ("summon_item", (api.summon_item)(me, ItemId(1), NpcId(1))),
            (
                "survivor_status",
                (api.survivor_status)(me, callback(), user),
            ),
            (
                "interaction_filter",
                (api.interaction_filter)(me, callback(), user),
            ),
            (
                "skill_override",
                (api.skill_override)(me, SkillNode(1), callback(), user),
            ),
            ("comfort_filter", (api.comfort_filter)(me, callback(), user)),
            (
                "experience_filter",
                (api.experience_filter)(me, callback(), user),
            ),
            (
                "subscribe_game_event",
                (api.subscribe_game_event)(me, 1, callback(), user),
            ),
            ("craft_filter", (api.craft_filter)(me, callback(), user)),
            ("build_filter", (api.build_filter)(me, callback(), user)),
            (
                "net_channel",
                (api.net_channel)(me, Str::new("late"), callback(), user, &raw mut index),
            ),
            ("request_time_speed", (api.request_time_speed)(me)),
            ("damage_filter", (api.damage_filter)(me, callback(), user)),
            ("summoned", (api.summoned)(me, callback(), user)),
            (
                "define_item_data",
                (api.define_item_data)(me, id, &raw mut index),
            ),
            (
                "config_check",
                (api.config_check)(me, Str::new("late"), callback(), user),
            ),
            (
                "define_stat",
                (api.define_stat)(me, id, &spec::<StatSpec>(), &raw mut stat),
            ),
            ("stat_buff", (api.stat_buff)(me, 1, core::ptr::null(), 0)),
            ("stats_changed", (api.stats_changed)(me, callback(), user)),
            (
                "define_event",
                (api.define_event)(me, &spec::<EventSpec>(), &raw mut index),
            ),
            (
                "subscribe_mod_event",
                (api.subscribe_mod_event)(me, &spec::<EventSpec>(), callback(), user),
            ),
            (
                "define_affix",
                (api.define_affix)(me, id, &spec::<AffixSpec>(), &raw mut stat),
            ),
            (
                "declare_content",
                (api.declare_content)(me, id, &raw mut guid),
            ),
            (
                "define_entity_data",
                (api.define_entity_data)(me, id, EntityDataFlags(0), &raw mut index),
            ),
            (
                "knowledge_input_filter",
                (api.knowledge_input_filter)(me, callback(), user),
            ),
            (
                "define_point",
                (api.define_point)(me, &spec::<PointSpec>(), &raw mut point),
            ),
            (
                "attach_point",
                (api.attach_point)(me, &spec::<PointLink>(), callback(), user),
            ),
            ("actions", (api.actions)(me, callback(), user)),
            ("rate", (api.rate)(me, RateId(1))),
            (
                "change_data",
                (api.change_data)(me, &spec::<DataChange>(), null_mut()),
            ),
            (
                "loop_wait",
                (api.loop_wait)(me, callback(), user, &raw mut sites),
            ),
            (
                "server_wait",
                (api.server_wait)(me, callback(), user, &raw mut found),
            ),
            ("task_workers", (api.task_workers)(me, 1, 1, &raw mut found)),
            (
                "altar_zone_filter",
                (api.altar_zone_filter)(me, callback(), user),
            ),
            (
                "serve_call",
                (api.serve_call)(me, &spec::<CallSpec>(), callback(), user),
            ),
            (
                "find_call",
                (api.find_call)(me, &spec::<CallSpec>(), &raw mut index),
            ),
            ("request_base_limits", (api.request_base_limits)(me, 1, 1)),
            (
                "control",
                (api.control)(me, &spec::<ControlSpec>(), callback(), user),
            ),
            ("item_filter", (api.item_filter)(me, callback(), user)),
            ("chat_filter", (api.chat_filter)(me, callback(), user)),
            (
                "crafting_prop",
                (api.crafting_prop)(me, &KEY.guid, &raw mut found),
            ),
            ("commands", (api.commands)(me, Str::new("late"))),
            ("join_rule", (api.join_rule)(me, callback(), user)),
            (
                "container_rule",
                (api.container_rule)(me, ItemId::new(0x5e01), callback(), user),
            ),
        ]
    }

    /// Each init-only entry only a client serves, called the same way.
    fn client_late_answers(api: &HostApi, me: ModHandle) -> Vec<(&'static str, Status)> {
        let user = null_mut::<c_void>();
        vec![
            ("ui_frame", (api.ui_frame)(me, callback(), user)),
            ("ui_hud", (api.ui_hud)(me, callback(), user)),
            ("request_fog", (api.request_fog)(me)),
            (
                "ui_extension",
                (api.ui_extension)(me, &spec::<UiExtension>(), callback(), user),
            ),
        ]
    }

    struct Late;

    impl Mod for Late {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Late,
        r#"
        id = "you.late"
        side = "both"
        "#
    );

    /// What each of `answers` said that was not too late, on the fake for
    /// `side` once the mod's init has returned.
    fn early(
        side: Side,
        answers: fn(&HostApi, ModHandle) -> Vec<(&'static str, Status)>,
    ) -> Vec<(&'static str, Status)> {
        let loader: Installed = FakeHost::new().side(side).code(&CODE).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Late)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        answers(api, loader.handle())
            .into_iter()
            .filter(|(_, status)| *status != Status::TOO_LATE)
            .collect()
    }

    #[test]
    fn every_init_only_entry_answers_too_late_once_init_has_ended() {
        assert_eq!(early(Side::SERVER, late_answers), []);
    }

    #[test]
    fn every_client_init_only_entry_answers_too_late_once_init_has_ended() {
        assert_eq!(early(Side::CLIENT, client_late_answers), []);
    }
}
