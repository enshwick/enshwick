//! The contract the loader's `HostApi` and the fake host's keep: every entry
//! named by its field, its caller kind, and its answer to a call with zeroed
//! arguments under each [`Precondition`], written as one table.
//!
//! The loader's tests write the table from the loader and the fake's tests
//! hold the fake to it, each on its own feature set.

use core::fmt::Write as _;

use crate::abi::{HostApi, ModHandle, Status};

mod valid;

pub use valid::VALID;

/// An entry called by `me` with every other argument zero: null pointers,
/// no callbacks, zero ids and counts.
pub trait Zeroed {
    /// Calls the entry.
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
pub type Entry = fn(&HostApi, ModHandle) -> Status;

/// Calls the macro `$then!` with the name of every `HostApi` entry, in field
/// order, so each host's tests list its own facts of every entry.
#[macro_export]
#[doc(hidden)]
macro_rules! __host_entries {
    ($then:ident) => {
        $then! {
            build symbol system field_offset type_size hook caller_frame fault log settings publish
            service constant data_dir log_level subscribe type_hash component entity_template mods
            code_range patch_code workshop_recipes hosting server_world join_missing hook_at status
            subscribe_game_event net_channel net_send net_ready config_schema config_check texts
            define_event subscribe_mod_event send_event rename_content mod_dir language define_point
            attach_point run_point event_types feature loader_off loop_wait server_wait task_workers
            serve_call find_call call subscribe_tick set_tick_every set_config
            subscribe_client_game_event serve_resource add_text define_item_data item_data_get
            set_item_data content_serve content_read icons_ready declare_content define_entity_data
            resource_read game_text rebuild_resource rekey_resource recipe_rule recipe_disabled
            recipe_kind rate set_rate change_data set_change use_content resource_keys
            content_hashes types game_version crafting_prop read_only resource_served
            resource_buffer content_name player players chat chat_all set_label rename_filter
            item_info request_altars altars building_zone_filter base_zones entities destroy
            save_world world_key component_write notify time_of_day set_time_of_day
            request_time_speed set_time_speed game_settings set_game_settings buff_types
            buff_type_add buffs apply_buff remove_buff weather_at weather_fronts set_weather
            terrain_materials terrain_material_add terrain_edit altar_zone_filter
            altar_zones_changed altar_limit_filter base_limit voxel_read build_rights
            change_permissions transaction_commit transaction_undo transaction_status world_quiet
            transaction_entities transaction_settled prop_placement_info world_persists map_markers
            account_action map_frame waypoint request_base_limits set_chat_mute chat_muted
            set_base_limit save_info hold_saves set_saved progress_write server_stop fog_uncover
            language_of join_rule request_inventory inventory_submit container_rule show_pickup
            survivor_lookup survivor_bed_check summon_filter summon_item survivor_status emit
            teleport spawn respawn_point set_respawn_point damage_filter damage_causes_off summoned
            define_stat stat_info stat_value stat_modify stat_buff stats_changed stat_sources
            stat_watch define_affix affix_roll item_lines affix_info item_filter drop_items
            skill_override comfort_filter experience_filter knowledge change_knowledge knowledge_set
            quests quest_reward_filter knowledge_input_filter game_query run_console
            interaction_filter interaction_prompt craft_filter build_filter actions control
            run_action chat_filter commands run_command command_list actions_changed action_window
            local_held local_input_status client_snapshot client_command client_command_status
            client_windows client_slots game_binding game_rebind ui_frame ui_widget ui_dialog ui_hud
            ui_markers request_fog fog_read map_marker_types ui_extension ui_item_tooltip sound_play
            sound_steer sound_state sound_play_for sound_steer_for
        }
    };
}

/// [`ENTRIES`], and [`every_field`], which stops the build when `HostApi`
/// gains a field [`__host_entries!`] lacks.
macro_rules! entries {
    ($($field:ident)*) => {
        /// Every entry by name, in field order, called with zeroed arguments.
        pub const ENTRIES: &[(&str, Entry)] = &[
            $((stringify!($field), |api, me| Zeroed::call(api.$field, me)),)*
        ];

        /// Names every field of `HostApi`, with no rest pattern.
        const fn every_field(api: &HostApi) {
            let HostApi { header: _, host_version: _, $($field: _),* } = api;
        }

        /// Each entry's offset in `HostApi`, in the order of [`ENTRIES`].
        #[cfg(test)]
        const OFFSETS: &[usize] = &[$(core::mem::offset_of!(HostApi, $field)),*];
    };
}

__host_entries!(entries);

const _: fn(&HostApi) = every_field;

/// What the caller is when it calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "hidden: only the loader and the fake host name it"
)]
pub enum Precondition {
    /// A handle neither host gave out.
    UnknownMod,
    /// A mod whose init failed, so the host turned it off.
    ModOff,
    /// A mod during its init.
    InInit,
    /// A data pack of the server side calling in its init: on the loader
    /// during the pack's own init, as the loader runs it; on the fake host,
    /// which runs no init of a pack's own, while its mod's init runs, the
    /// one window it gives a pack's init calls.
    Pack,
    /// The loader's own copy of the table, the one caller that is no mod.
    Loader,
    /// The same mod once every mod's init returned.
    AfterInit,
}

/// In this order: once a call after init seals the loader's registries,
/// they stay sealed for the process.
pub const PRECONDITIONS: [Precondition; 6] = [
    Precondition::UnknownMod,
    Precondition::ModOff,
    Precondition::InInit,
    Precondition::Pack,
    Precondition::Loader,
    Precondition::AfterInit,
];

/// A handle neither host gives out.
pub const UNKNOWN: ModHandle = ModHandle(9);
/// The handle of the mod on each host.
pub const ME: ModHandle = ModHandle(1);
/// The handle of the data pack on each host.
pub const PACK: ModHandle = ModHandle(2);
/// The id of the mod each host runs as [`ME`], whose namespace the calls of
/// [`VALID`] name.
pub const MOD_ID: &str = "test.fake";

/// Every status by its constant's name, so the table reads as the ABI does.
pub const STATUS_NAMES: [(Status, &str); 18] = [
    (Status::OK, "OK"),
    (Status::MISSING, "MISSING"),
    (Status::AMBIGUOUS, "AMBIGUOUS"),
    (Status::REFUSED, "REFUSED"),
    (Status::BAD_ARGS, "BAD_ARGS"),
    (Status::TOO_LATE, "TOO_LATE"),
    (Status::FAULTED, "FAULTED"),
    (Status::WRONG_THREAD, "WRONG_THREAD"),
    (Status::NOT_LOADED, "NOT_LOADED"),
    (Status::OUTSIDE_WORLD, "OUTSIDE_WORLD"),
    (Status::STATIC_HALF, "STATIC_HALF"),
    (Status::NOT_A_PROP, "NOT_A_PROP"),
    (Status::NOT_KEPT, "NOT_KEPT"),
    (Status::UNAVAILABLE, "UNAVAILABLE"),
    (Status::BUILDING, "BUILDING"),
    (Status::LOADER_FAULT, "LOADER_FAULT"),
    (Status::BUSY, "BUSY"),
    (Status::TOO_SMALL, "TOO_SMALL"),
];

/// The constant's name of `status`; `None` for a status the ABI does not
/// name.
#[must_use]
pub fn status_name(status: Status) -> Option<&'static str> {
    STATUS_NAMES
        .iter()
        .find(|&&(known, _)| known == status)
        .map(|&(_, name)| name)
}

/// Who may call an entry, as each host's entry layer reads the calling
/// handle before the entry's body runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "hidden: only the loader and the fake host name it"
)]
pub enum CallerKind {
    /// A loaded mod that is on.
    Mod,
    /// A loaded mod, on or off.
    AnyMod,
    /// A mod that is on, or the loader's own copy.
    Caller,
    /// A loaded mod, on or off, or the loader's own copy.
    Writer,
}

impl CallerKind {
    /// Every kind.
    pub const ALL: [Self; 4] = [Self::Mod, Self::AnyMod, Self::Caller, Self::Writer];

    /// The kind's word in the table, the loader's name for its parameter
    /// kind.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mod => "Mod",
            Self::AnyMod => "AnyMod",
            Self::Caller => "Caller",
            Self::Writer => "Writer",
        }
    }
}

/// One entry's line of the table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "hidden: only the loader and the fake host name it"
)]
pub struct Row {
    /// The entry's field name.
    pub entry: String,
    /// The kind of caller the entry takes.
    pub caller: CallerKind,
    /// The answer under each of [`PRECONDITIONS`], in order.
    pub answers: [Status; PRECONDITIONS.len()],
}

/// The table of `rows`, one line each after a line naming the columns.
#[must_use]
pub fn render(rows: &[Row]) -> String {
    let mut text = String::from("entry caller");
    for precondition in PRECONDITIONS {
        let _ = write!(text, " {precondition:?}");
    }
    for row in rows {
        let _ = write!(text, "\n{} {}", row.entry, row.caller.name());
        for status in row.answers {
            let _ = match status_name(status) {
                Some(name) => write!(text, " {name}"),
                None => write!(text, " {}", status.0),
            };
        }
    }
    text
}

/// Why a table does not read.
#[derive(Debug, thiserror::Error, crate::script::ErrorKind)]
#[error("line {line} of the contract table: {why}")]
#[expect(
    clippy::exhaustive_structs,
    reason = "hidden: only the loader and the fake host name it"
)]
pub struct ReadTableError {
    /// The line, counted from 1 after any snapshot header.
    pub line: usize,
    /// What is wrong with it.
    pub why: &'static str,
}

/// The rows of `text`, as [`render`] writes them, after the header an insta
/// snapshot file opens with when it has one.
///
/// # Errors
///
/// The first line that is not as [`render`] writes it.
pub fn parse(text: &str) -> Result<Vec<Row>, ReadTableError> {
    let body = text
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---").map(|(_, body)| body))
        .map_or(text, |body| body.trim_start_matches(['\r', '\n']));
    let mut lines = body
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.trim_end()));
    let header = render(&[]);
    if lines.next().map(|(_, line)| line) != Some(header.as_str()) {
        return Err(ReadTableError {
            line: 1,
            why: "the first line does not name the columns of today's preconditions",
        });
    }
    lines
        .filter(|(_, line)| !line.is_empty())
        .map(|(line, text)| {
            row(text).ok_or(ReadTableError {
                line,
                why: "not an entry, a caller and an answer per precondition",
            })
        })
        .collect()
}

/// One line of the table.
fn row(line: &str) -> Option<Row> {
    let mut words = line.split(' ');
    let entry = words.next()?.to_owned();
    let word = words.next()?;
    let caller = CallerKind::ALL
        .into_iter()
        .find(|kind| kind.name() == word)?;
    let mut answers = [Status::OK; PRECONDITIONS.len()];
    for answer in &mut answers {
        let word = words.next()?;
        *answer = STATUS_NAMES
            .iter()
            .find(|&&(_, name)| name == word)
            .map_or_else(
                || word.parse().ok().map(Status),
                |&(status, _)| Some(status),
            )?;
    }
    words.next().is_none().then_some(Row {
        entry,
        caller,
        answers,
    })
}

/// Set in the child process [`run_apart`] starts.
const APART: &str = "ENSHWICK_CONTRACT_CHILD";

/// Runs `test`, an ignored test of this test binary, alone in a child
/// process: each host's half fills process-wide state that the other tests
/// of its binary read.
///
/// # Errors
///
/// The child process did not start.
///
/// # Panics
///
/// When `test` fails in the child or does not run there.
pub fn run_apart(test: &str) -> std::io::Result<()> {
    let child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", test, "--ignored", "--nocapture"])
        .args(["--test-threads", "1"])
        .env(APART, "1")
        .output()?;
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(child.status.success(), "{out}");
    assert!(out.contains("1 passed"), "{test} ran in the child: {out}");
    Ok(())
}

/// Whether this process is the child [`run_apart`] started, the one where
/// its test does its work.
#[must_use]
pub fn apart() -> bool {
    std::env::var_os(APART).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row> {
        vec![
            Row {
                entry: "build".to_owned(),
                caller: CallerKind::Caller,
                answers: [
                    Status::BAD_ARGS,
                    Status::FAULTED,
                    Status::OK,
                    Status::OK,
                    Status::OK,
                    Status(99),
                ],
            },
            Row {
                entry: "log".to_owned(),
                caller: CallerKind::Writer,
                answers: [Status::UNAVAILABLE; 6],
            },
        ]
    }

    #[test]
    fn a_rendered_table_parses_back_to_its_rows() {
        let text = render(&rows());
        assert_eq!(parse(&text).unwrap(), rows());
        assert!(text.ends_with(
            "log Writer UNAVAILABLE UNAVAILABLE UNAVAILABLE UNAVAILABLE UNAVAILABLE UNAVAILABLE"
        ));
    }

    #[test]
    fn a_table_in_an_insta_snapshot_file_parses_past_its_header() {
        let file = format!(
            "---\nsource: crates/runtime/loader/src/host/contract_tests.rs\nexpression: table\n---\n{}\n",
            render(&rows())
        );
        assert_eq!(parse(&file).unwrap(), rows());
        assert_eq!(parse(&file.replace('\n', "\r\n")).unwrap(), rows());
    }

    #[test]
    fn a_table_of_other_columns_or_a_short_row_names_its_line() {
        let old = render(&rows()).replacen(" Pack", "", 1);
        assert_eq!(parse(&old).unwrap_err().line, 1);
        let short = format!("{}\nmods AnyMod OK", render(&rows()));
        assert_eq!(parse(&short).unwrap_err().line, 4);
    }

    #[test]
    fn a_row_whose_caller_is_no_kind_names_its_line() {
        let table = render(&rows()).replacen("Writer", "Admin", 1);
        assert_eq!(parse(&table).unwrap_err().line, 3);
        for kind in CallerKind::ALL {
            let row = Row {
                caller: kind,
                ..rows().remove(1)
            };
            assert_eq!(parse(&render(core::slice::from_ref(&row))).unwrap(), [row]);
        }
    }

    #[test]
    fn the_entries_are_listed_in_the_order_of_the_fields_of_host_api() {
        let early = OFFSETS
            .windows(2)
            .position(|pair| pair[0] >= pair[1])
            .map(|at| ENTRIES[at + 1].0);
        assert_eq!(early, None, "listed before a field it follows");
    }

    #[test]
    fn the_valid_calls_name_entries_once_each_in_field_order() {
        let at = |name: &str| ENTRIES.iter().position(|&(entry, _)| entry == name);
        let places: Vec<Option<usize>> = VALID.iter().map(|&(name, _)| at(name)).collect();
        assert!(places.iter().all(Option::is_some), "{places:?}");
        assert!(places.is_sorted_by(|a, b| a < b), "{places:?}");
    }
}
