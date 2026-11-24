//! The fake's client commands, client snapshot and carried windows' items:
//! the loader's refusals, with the outcomes the test drives.

use crate::test::table::base::args;
use crate::test::table::entries;
use enshwick_core::abi::{Side, Status};

use crate::test::table::with;

entries! {

    /// Queues a client command as the loader does: unavailable on a server,
    /// a bad argument for an unknown kind, refused while one of its kind waits;
    /// the test drives its status.
    pub(crate) fn client_command(
        me as Mod,
        spec: *const enshwick_core::abi::ClientCommand as args::Headed,
        id: *mut u64 as args::Out,
    ) -> Status {
        use enshwick_core::abi::{ClientCommand as Spec, ClientCommandStatus as Raw};
        let Some(spec) = crate::test::table::get_headed(spec) else {
            return Status::BAD_ARGS;
        };
        if id.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            let open = |status: &Raw| status.state == Raw::PENDING || status.state == Raw::APPLIED;
            if spec.kind == Spec::CANCEL {
                if let Some((.., status)) =
                    s.ui.client_commands
                        .iter_mut()
                        .find(|(queued, .., status)| *queued == spec.cancels && open(status))
                {
                    status.state = Raw::CANCELLED;
                    status.reason = Raw::BY_MOD;
                }
                // SAFETY: the safe layer passes a writable `u64`, non-null above.
                unsafe { id.write(spec.cancels) };
                return Status::OK;
            }
            if !(Spec::OPEN_SCREEN..=Spec::WINDOW_TEXT).contains(&spec.kind) || !spec.in_bounds() {
                return Status::BAD_ARGS;
            }
            // As the loader: a waypoint or ping inside the map's frame, when
            // the world has one.
            let marks = |kind: u32| {
                matches!(
                    kind,
                    Spec::SET_WAYPOINT | Spec::CLEAR_WAYPOINT | Spec::PING
                )
            };
            if matches!(spec.kind, Spec::SET_WAYPOINT | Spec::PING)
                && s.host.world.map_frame.is_some_and(|frame| {
                    !enshwick_base::__internal::waypoints::inside(
                        frame.origin,
                        frame.size,
                        spec.position,
                    )
                })
            {
                return Status::BAD_ARGS;
            }
            if marks(spec.kind)
                && s.ui
                    .client_commands
                    .iter()
                    .any(|(_, kind, _, status)| marks(*kind) && open(status))
            {
                return Status::REFUSED;
            }
            // SAFETY: the mod's spec, its target borrowed for the call; the
            // password is never read, kept or returned.
            let target = unsafe { spec.target.as_str() }
                .unwrap_or_default()
                .to_owned();
            // As the loader: a press's action and its bit before its screen.
            if spec.kind == Spec::PRESS {
                // `GameAction` comes with `unstable`, as the press API does.
                #[cfg(feature = "unstable")]
                if !enshwick_core::controls::GameAction::ALL
                    .iter()
                    .any(|action| action.name() == target)
                {
                    return Status::BAD_ARGS;
                }
                if !reflects_action(&s.host, &target) {
                    return Status::UNAVAILABLE;
                }
            }
            // As the loader: a host starts at the main menu, which no HUD
            // screen names.
            // SAFETY: the mod's spec, its expected screen borrowed for the call.
            let expects = unsafe { spec.expected_screen.as_str() }.is_some_and(|s| !s.is_empty());
            // As the loader: a mod's page by the id of a mod the test
            // loaded.
            let names_mod = matches!(spec.kind, Spec::OPEN_MOD | Spec::OPEN_MOD_SETTINGS);
            if names_mod
                && !s.host.base.mods.iter().any(|loaded| {
                    // SAFETY: the test's own mod info, its id text kept for
                    // the fake's life.
                    Some(target.as_str()) == unsafe { loaded.id.as_str() }
                })
            {
                return Status::BAD_ARGS;
            }
            if spec.kind == Spec::HOST && expects {
                return Status::BAD_ARGS;
            }
            if !named_as_the_loader_takes(&spec) {
                return Status::BAD_ARGS;
            }
            // One of each kind waits, one press of each action, and a mod's
            // presses up to the loader's limit.
            let presses =
                s.ui.client_commands
                    .iter()
                    .filter(|(_, kind, _, status)| *kind == Spec::PRESS && open(status))
                    .count();
            if s.ui.client_commands.iter().any(|(_, kind, queued, status)| {
                *kind == spec.kind && open(status) && (spec.kind != Spec::PRESS || *queued == target)
            }) || (spec.kind == Spec::PRESS
                && presses >= enshwick_core::__fake::rules::PRESSES_PER_MOD)
            {
                return Status::REFUSED;
            }
            // One join or leave at a time; a join from the main menu, outside a
            // world, which the snapshot's session names, and a leave inside one.
            let plays = |kind: u32| matches!(kind, Spec::JOIN | Spec::LEAVE | Spec::HOST);
            if plays(spec.kind) {
                let in_world = s.ui.client_snapshot.session != 0;
                let running =
                    s.ui.client_commands
                        .iter()
                        .any(|(_, kind, _, status)| plays(*kind) && open(status));
                let from_menu = matches!(spec.kind, Spec::JOIN | Spec::HOST);
                if running || in_world == from_menu {
                    return Status::REFUSED;
                }
            }
            let next = s.ui.client_commands.len() as u64 + 1;
            s.ui.client_commands.push((
                next,
                spec.kind,
                target,
                Raw {
                    kind: spec.kind,
                    // As the loader queues them: a join waits for its server's
                    // answer, a leave starts in the world.
                    phase: match spec.kind {
                        Spec::JOIN => Raw::PHASE_QUEUED,
                        Spec::HOST => Raw::PHASE_MENU,
                        Spec::LEAVE => Raw::PHASE_IN_WORLD,
                        _ => 0,
                    },
                    ..Raw::EMPTY
                },
            ));
            // SAFETY: the safe layer passes a writable `u64`, non-null above.
            unsafe { id.write(next) };
            Status::OK
        })
    }
}

/// Whether the host's reflected `PlayerInputType`, which
/// [`FakeHost::player_input`](crate::test::FakeHost::player_input) declares,
/// names the game action `name`, as the loader finds a press's bit; a host
/// without it names none.
fn reflects_action(host: &crate::test::FakeHost, name: &str) -> bool {
    host.base
        .type_hashes
        .get(enshwick_core::__fake::ids::PLAYER_INPUT_TYPE)
        .and_then(|hash| {
            host.content
                .type_list
                .iter()
                .find(|ty| ty.qualified_hash() == *hash)
        })
        .is_some_and(|ty| ty.constants().iter().any(|c| c.name() == name))
}

/// Whether `spec`'s target and expected screen are ones the loader takes:
/// an open of a screen the game opens from the world, a tab the menu has,
/// and a look or move that expects only the world's screen.
fn named_as_the_loader_takes(spec: &enshwick_core::abi::ClientCommand) -> bool {
    // SAFETY: the mod's spec, its target borrowed for the call.
    let target = unsafe { spec.target.as_str() };
    // SAFETY: the mod's spec, its expected screen borrowed for the call.
    let expected = unsafe { spec.expected_screen.as_str() };
    // SAFETY: the mod's spec, its item borrowed for the call.
    let item = unsafe { spec.item.as_str() };
    // SAFETY: the mod's spec, its text borrowed for the call.
    let text = unsafe { spec.text.as_str() };
    let window_kind = matches!(
        spec.kind,
        enshwick_core::abi::ClientCommand::WINDOW_PRESS
            | enshwick_core::abi::ClientCommand::WINDOW_PICK
            | enshwick_core::abi::ClientCommand::WINDOW_TEXT
    );
    if window_kind {
        return target
            .zip(item)
            .zip(text)
            .is_some_and(|((window, item), text)| {
                enshwick_core::__fake::rules::window_command_names(spec.kind, window, item, text)
            });
    }
    target.zip(expected).is_some_and(|(target, expected)| {
        enshwick_core::__fake::rules::client_command_names(spec.kind, target, expected)
    })
}

entries! {
    /// Where a queued client command stands, as the test set it.
    pub(crate) fn client_command_status(
        me as Mod,
        id: u64,
        out: *mut enshwick_core::abi::ClientCommandStatus as args::HeadedOut,
    ) -> Status {
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            match s
                .ui
                .client_commands
                .iter()
                .find(|(queued, ..)| *queued == id)
            {
                Some((.., status)) => crate::test::table::put_headed(out, *status),
                None => Status::UNAVAILABLE,
            }
        })
    }

    /// The client snapshot the test set; the loader's answers: unavailable on
    /// a server, and from the first ask, which starts the reads, until an
    /// interface step reads them, the empty snapshot, never read.
    pub(crate) fn client_snapshot(
        me as Caller,
        out: *mut enshwick_core::abi::ClientSnapshot as args::HeadedOut,
    ) -> Status {
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if s.host.base.side == Side::SERVER {
                return Status::UNAVAILABLE;
            }
            s.ui.snapshot_reads.0 = true;
            let snapshot = if s.ui.snapshot_reads.1 {
                s.ui.client_snapshot
            } else {
                enshwick_core::abi::ClientSnapshot::EMPTY
            };
            crate::test::table::put_headed(out, snapshot)
        })
    }

    /// The carried windows' items the test set; unavailable on a server, as
    /// the loader answers.
    pub(crate) fn client_windows(
        me as Caller,
        [out: *mut enshwick_core::abi::ClientWindowItem, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let items = with(Err(Status::MISSING), |s| {
            if s.host.base.side == Side::SERVER {
                return Err(Status::UNAVAILABLE);
            }
            Ok(s.ui.client_windows.clone())
        });
        match items {
            Ok(items) => crate::test::table::base::io::list(&items, out, entry_size, capacity, count),
            Err(status) => status,
        }
    }
}
