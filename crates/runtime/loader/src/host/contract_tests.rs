//! The loader's table and the fake host's answer every entry alike under
//! the same few preconditions, so a mod's fake host test fails where the
//! game would: every entry is called with zeroed arguments on both and the
//! statuses compared.

use std::cell::Cell;
use std::sync::atomic::Ordering;

use enshwick_core::abi::{HostApi, ModHandle, Status};

use super::API;
use crate::engine::loader::{initialising, install_for_test};

/// An entry called by `me` with every other argument zero: null pointers,
/// no callbacks, zero ids and counts.
trait Zeroed {
    fn call(self, me: ModHandle) -> Status;
}

/// [`Zeroed`] for the entries of one arity.
macro_rules! zeroed {
    ($($arg:ident),*) => {
        impl<$($arg: bytemuck::Zeroable),*> Zeroed for extern "system" fn(ModHandle $(, $arg)*) -> Status {
            fn call(self, me: ModHandle) -> Status {
                self(me $(, <$arg as bytemuck::Zeroable>::zeroed())*)
            }
        }
    };
}

zeroed!();
zeroed!(A);
zeroed!(A, B);
zeroed!(A, B, C);
zeroed!(A, B, C, D);
zeroed!(A, B, C, D, E);
zeroed!(A, B, C, D, E, F);
zeroed!(A, B, C, D, E, F, G);
zeroed!(A, B, C, D, E, F, G, H);
zeroed!(A, B, C, D, E, F, G, H, I);

/// One entry of a table, called by a handle with zeroed arguments.
type Entry = fn(&HostApi, ModHandle) -> Status;

/// [`ENTRIES`], every entry by name, and [`every_field`], which stops the
/// build when `HostApi` gains one the list lacks.
macro_rules! entries {
    ($($field:ident)*) => {
        const ENTRIES: &[(&str, Entry)] = &[
            $((stringify!($field), |api, me| Zeroed::call(api.$field, me)),)*
        ];

        /// Names every field of `HostApi`, with no rest pattern.
        fn every_field(api: &HostApi) {
            let HostApi { header: _, host_version: _, $($field: _),* } = api;
        }
    };
}

entries! {
    build symbol system field_offset type_size hook caller_frame fault log settings publish
    service constant data_dir log_level subscribe type_hash component entity_template mods
    code_range patch_code workshop_recipes hosting server_world join_missing hook_at status
    subscribe_game_event net_channel net_send net_ready config_schema config_check texts
    define_event subscribe_mod_event send_event rename_content mod_dir language define_point
    attach_point run_point event_types feature loop_wait server_wait task_workers serve_call
    find_call call subscribe_tick set_tick_every set_config subscribe_client_game_event
    serve_resource add_text define_item_data item_data_get set_item_data content_serve
    content_read icons_ready declare_content define_entity_data resource_read game_text
    rebuild_resource recipe_rule recipe_disabled recipe_kind rate set_rate change_data
    set_change use_content resource_keys content_hashes types game_version crafting_prop
    read_only resource_served resource_buffer content_name player players chat chat_all
    set_label rename_filter item_info request_altars altars building_zone_filter base_zones
    entities destroy save_world world_key component_write notify time_of_day set_time_of_day
    request_time_speed set_time_speed game_settings set_game_settings buff_types buffs
    apply_buff remove_buff weather_at weather_fronts set_weather terrain_materials terrain_edit
    altar_zone_filter altar_zones_changed altar_limit_filter base_limit voxel_read build_rights
    transaction_commit transaction_undo transaction_status world_quiet transaction_entities
    world_persists map_markers account_action map_frame request_base_limits set_chat_mute
    chat_muted set_base_limit save_info hold_saves set_saved progress_write server_stop
    fog_uncover language_of join_rule request_inventory inventory_submit survivor_lookup
    survivor_bed_check summon_filter summon_item survivor_status emit teleport spawn
    damage_filter summoned define_stat stat_info stat_value stat_modify stat_buff stats_changed
    stat_sources stat_watch define_affix affix_roll item_lines affix_info item_filter
    drop_items skill_override comfort_filter experience_filter knowledge change_knowledge
    quests knowledge_input_filter game_query run_console interaction_filter craft_filter
    build_filter actions control run_action chat_filter commands run_command command_list
    actions_changed action_window ui_frame ui_widget ui_dialog ui_hud ui_markers request_fog
    fog_read map_marker_types ui_extension local_held local_input_status client_snapshot
    loader_off client_command client_command_status terrain_material_add interaction_prompt
    ui_item_tooltip change_permissions transaction_settled prop_placement_info container_rule
    show_pickup respawn_point set_respawn_point buff_type_add damage_causes_off waypoint
    sound_play sound_steer sound_state sound_play_for sound_steer_for rekey_resource
    game_binding game_rebind quest_reward_filter knowledge_set client_windows
}

/// What the caller is when it calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Precondition {
    /// A handle neither host gave out.
    UnknownMod,
    /// A mod whose init failed, so the host turned it off.
    ModOff,
    /// A mod during its init.
    InInit,
    /// The loader's own copy of the table, the one caller that is no mod.
    Loader,
    /// The same mod once every mod's init returned.
    AfterInit,
}

/// In this order: once a call after init seals the loader's registries,
/// they stay sealed for the process.
const PRECONDITIONS: [Precondition; 5] = [
    Precondition::UnknownMod,
    Precondition::ModOff,
    Precondition::InInit,
    Precondition::Loader,
    Precondition::AfterInit,
];

/// A handle neither host gives out.
const UNKNOWN: ModHandle = ModHandle(9);
/// The handle of the one mod on each host.
const ME: ModHandle = ModHandle(1);

/// A difference the hosts keep on purpose: the entry, the precondition,
/// the loader's answer, the fake's, and why.
type Kept = (&'static str, Precondition, Status, Status, &'static str);

/// The differences the hosts keep on purpose besides [`no_game`].
const KEPT: &[Kept] = &[(
    "item_data_get",
    Precondition::AfterInit,
    Status::MISSING,
    Status::BAD_ARGS,
    "the loader's kinds are the process's: an earlier call of this test defined one, \
     where each fake starts empty",
)];

/// Whether the loader's answer is that it runs no game: the bare loader of
/// this test has none, so it stops where the fake's game goes on, and
/// nothing off Windows can say what a game would answer there. Many
/// entries read their arguments only past the game check, so a refusal of
/// the caller cannot be told apart there either.
fn no_game(loader: Status) -> bool {
    loader == Status::UNAVAILABLE
}

/// The bare loader's answer to `entry` under `precondition`, on a fresh
/// loader with one mod.
///
/// After init the registries mods fill during init are sealed, as start
/// seals them; the seal is the process's for good, so every call after init
/// comes after every other.
fn loader_answers(entry: Entry, precondition: Precondition) -> Status {
    static SEALED: std::sync::Once = std::sync::Once::new();
    let loader = install_for_test(crate::start::tests::loader_running(&["you.probe"]));
    match precondition {
        Precondition::UnknownMod => entry(&API, UNKNOWN),
        Precondition::ModOff => {
            loader.mods[0].off.store(true, Ordering::Release);
            entry(&API, ME)
        }
        Precondition::InInit => initialising(0, || entry(&API, ME)),
        Precondition::Loader => entry(&API, ModHandle::NONE),
        Precondition::AfterInit => {
            SEALED.call_once(|| crate::start::seal(loader));
            let _first = loader.delivery.set(Box::default());
            entry(&API, ME)
        }
    }
}

thread_local! {
    /// The entry [`probe`] calls from the mod's init, and its answer.
    static PROBED: Cell<Option<Entry>> = const { Cell::new(None) };
    static ANSWER: Cell<Status> = const { Cell::new(Status::OK) };
}

/// An init that calls the entry [`PROBED`] holds and keeps its answer.
unsafe extern "system" fn probe(api: *const HostApi, me: ModHandle) -> Status {
    if let Some(entry) = PROBED.take() {
        // SAFETY: the fake passes its table, a static.
        ANSWER.set(entry(unsafe { &*api }, me));
    }
    Status::OK
}

/// An init or ready that agrees.
unsafe extern "system" fn agree(_api: *const HostApi, _me: ModHandle) -> Status {
    Status::OK
}

/// An init that fails, so the fake turns the mod off.
unsafe extern "system" fn refuse(_api: *const HostApi, _me: ModHandle) -> Status {
    Status::REFUSED
}

/// The fake host's answer to `entry` under `precondition`, on a fresh fake
/// with its one mod.
fn fake_answers(entry: Entry, precondition: Precondition) -> Status {
    let fake = enshwick::test::FakeHost::new().install();
    assert_eq!(fake.handle(), ME);
    // SAFETY: the fake's table is a static.
    let api = unsafe { &*fake.api() };
    match precondition {
        Precondition::UnknownMod => entry(api, UNKNOWN),
        Precondition::ModOff => {
            // SAFETY: `refuse` takes the table and handle the fake passes.
            let _off = unsafe { fake.init(refuse) };
            entry(api, ME)
        }
        Precondition::InInit => {
            PROBED.set(Some(entry));
            ANSWER.set(Status::OK);
            // SAFETY: `probe` takes the table and handle the fake passes.
            let _ran = unsafe { fake.init(probe) };
            assert!(PROBED.take().is_none(), "the fake ran no init");
            ANSWER.get()
        }
        Precondition::Loader => entry(api, ModHandle::NONE),
        Precondition::AfterInit => {
            // SAFETY: `agree` takes the table and handle the fake passes.
            let _init = unsafe { fake.init(agree) };
            // SAFETY: as for the init, which answered OK before this ready.
            let _ready = unsafe { fake.ready(agree) };
            entry(api, ME)
        }
    }
}

/// Set in the child process that compares the hosts.
const CHILD: &str = "ENSHWICK_CONTRACT_CHILD";

/// The child test that compares the hosts.
const COMPARED: &str = "host::contract_tests::every_entry_is_compared_in_a_process_of_its_own";

#[test]
fn every_entry_answers_the_same_on_the_loader_and_on_the_fake_host() {
    // The calls fill the loader's process-wide registries and seal them for
    // good, which the other tests of this process read, so they run apart.
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", COMPARED, "--ignored", "--nocapture"])
        .args(["--test-threads", "1"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(child.status.success(), "{out}");
    assert!(out.contains("1 passed"), "the comparison ran: {out}");
}

#[test]
#[ignore = "runs only as that test's child, in a process of its own"]
fn every_entry_is_compared_in_a_process_of_its_own() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    every_field(&API);
    let mut differences = Vec::new();
    for precondition in PRECONDITIONS {
        for &(name, entry) in ENTRIES {
            let loader = loader_answers(entry, precondition);
            let fake = fake_answers(entry, precondition);
            let kept = no_game(loader)
                || KEPT
                    .iter()
                    .any(|kept| *kept == (name, precondition, loader, fake, kept.4));
            if loader != fake && !kept {
                differences.push(format!(
                    "{name} {precondition:?}: loader {loader:?}, fake {fake:?}"
                ));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "{} differences:\n{}",
        differences.len(),
        differences.join("\n")
    );
}
