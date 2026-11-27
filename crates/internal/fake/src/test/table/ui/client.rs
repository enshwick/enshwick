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
            if spec.kind == Spec::CANCEL {
                // As the loader: a mod cancels only its own command.
                if let Some(command) = s
                    .ui
                    .client_commands
                    .iter_mut()
                    .find(|c| c.id == spec.cancels && c.owner == me && c.open())
                {
                    command.status.state = Raw::CANCELLED;
                    command.status.reason = Raw::BY_MOD;
                }
                // SAFETY: the safe layer passes a writable `u64`, non-null above.
                unsafe { id.write(spec.cancels) };
                return Status::OK;
            }
            if !(Spec::OPEN_SCREEN..=Spec::QUIT).contains(&spec.kind) || !spec.in_bounds() {
                return Status::BAD_ARGS;
            }
            // As the loader: a rebind names a control a mod declared, which
            // in the fake is one of the mod's own, and a key a control can
            // hold or none. Controls come with `unstable`, as the rebind API
            // does, so without it no control exists to rebind.
            #[cfg(feature = "unstable")]
            if spec.kind == Spec::REBIND && !rebinds_a_declared_control(s, me, &spec) {
                return Status::BAD_ARGS;
            }
            #[cfg(not(feature = "unstable"))]
            if spec.kind == Spec::REBIND {
                return Status::BAD_ARGS;
            }
            // As the loader: an answer goes to the dialog it names while that
            // one shows, and only by a button it has.
            if spec.kind == Spec::ANSWER_DIALOG {
                let Some((_, buttons, _)) = on_screen(s)
                    .filter(|&(number, ..)| number != 0 && number == spec.dialog)
                else {
                    return Status::UNAVAILABLE;
                };
                if buttons & 1 << spec.answer == 0 {
                    return Status::BAD_ARGS;
                }
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
            // As the loader: the caller's own open commands decide whether one
            // more of a kind waits; another mod's never do.
            let mine = || s.ui.client_commands.iter().filter(|c| c.owner == me && c.open());
            if marks(spec.kind) && mine().any(|c| marks(c.kind)) {
                return Status::REFUSED;
            }
            // As the loader: one crafting or item request at a time.
            let acts = |kind: u32| (Spec::CRAFT..=Spec::TRASH).contains(&kind);
            if acts(spec.kind) && mine().any(|c| acts(c.kind)) {
                return Status::REFUSED;
            }
            // As the loader: an equip names a member of the build's
            // `EquipmentSlot`.
            if spec.kind == Spec::EQUIP
                // SAFETY: the mod's spec, its target borrowed for the call.
                && !unsafe { spec.target.as_str() }.is_some_and(|name| {
                    reflects(&s.host, enshwick_core::__fake::ids::EQUIPMENT_SLOT, name)
                })
            {
                return Status::BAD_ARGS;
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
            let names_mod = matches!(
                spec.kind,
                Spec::OPEN_MOD | Spec::OPEN_MOD_SETTINGS | Spec::SET_SETTING
            );
            if names_mod
                && !s.host.base.mods.iter().any(|loaded| {
                    // SAFETY: the test's own mod info, its id text kept for
                    // the fake's life.
                    Some(target.as_str()) == unsafe { loaded.id.as_str() }
                })
            {
                return Status::BAD_ARGS;
            }
            if matches!(spec.kind, Spec::HOST | Spec::QUIT) && expects {
                return Status::BAD_ARGS;
            }
            if !named_as_the_loader_takes(&spec) {
                return Status::BAD_ARGS;
            }
            // One of each kind waits, one press of each action, and a mod's
            // presses up to the loader's limit.
            let presses = mine().filter(|c| c.kind == Spec::PRESS).count();
            if mine().any(|c| {
                c.kind == spec.kind && (spec.kind != Spec::PRESS || c.target == target)
            }) || (spec.kind == Spec::PRESS
                && presses >= enshwick_core::__fake::rules::PRESSES_PER_MOD)
            {
                return Status::REFUSED;
            }
            // One join or leave at a time across all mods, as the loader's
            // play runs one; a join from the main menu, outside a world, which
            // the snapshot's session names, and a leave inside one.
            let plays = |kind: u32| matches!(kind, Spec::JOIN | Spec::LEAVE | Spec::HOST);
            if plays(spec.kind) || spec.kind == Spec::QUIT {
                let in_world = s.ui.client_snapshot.session != 0;
                let running = s.ui.client_commands.iter().any(|c| plays(c.kind) && c.open());
                let from_menu = matches!(spec.kind, Spec::JOIN | Spec::HOST | Spec::QUIT);
                if running || in_world == from_menu {
                    return Status::REFUSED;
                }
            }
            let next = s.ui.client_commands.len() as u64 + 1;
            s.ui.client_commands.push(super::QueuedCommand {
                id: next,
                owner: me,
                kind: spec.kind,
                target,
                status: enshwick_core::headed!(Raw {
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
                }),
            });
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
/// The dialog on screen as the loader tells it: `None` before the mod's
/// init returned, since the loader shows dialogs in game only once its hooks
/// are armed; else the oldest waiting one, numbered by the answers before
/// it as the loader numbers dialogs as they show, 0 while none waits, its
/// buttons, and whether the mod under test asked it.
fn on_screen(
    s: &crate::test::table::State,
) -> Option<(u32, u32, Option<enshwick_core::abi::ModHandle>)> {
    if s.base.registration != crate::test::table::Registration::Closed {
        return None;
    }
    Some(s.ui.dialogs.first().map_or((0, 0, None), |(shown, asked)| {
        (
            s.ui.dialogs_answered % 65_535 + 1,
            shown.buttons,
            asked.owner(),
        )
    }))
}

/// Whether rebind `spec` from mod `me` names one of its own declared
/// controls, by the caller's id, with no key or bits a key holds, as the
/// loader checks a rebind against every declared control.
#[cfg(feature = "unstable")]
fn rebinds_a_declared_control(
    s: &crate::test::table::State,
    me: enshwick_core::abi::ModHandle,
    spec: &enshwick_core::abi::ClientCommand,
) -> bool {
    // SAFETY: the mod's spec, its target borrowed for the call.
    let target = unsafe { spec.target.as_str() };
    // SAFETY: the same spec's item, borrowed for the call.
    let item = unsafe { spec.item.as_str() };
    let own = crate::test::table::base::mods::id_of(s, me).is_some_and(|id| target == Some(id));
    let key = spec.key == 0 || enshwick_core::controls::Key::from_bits(spec.key).is_some();
    own && key && item.is_some_and(|name| s.actions.controls.declares(name))
}

fn reflects_action(host: &crate::test::FakeHost, name: &str) -> bool {
    reflects(host, enshwick_core::__fake::ids::PLAYER_INPUT_TYPE, name)
}

/// Whether the fake build reflects the enum `ty` with a member `name`.
fn reflects(host: &crate::test::FakeHost, ty: &str, name: &str) -> bool {
    host.base
        .type_hashes
        .get(ty)
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
/// a look or move that expects only the world's screen, and a chat line or
/// a setting's key and value of UTF-8 text, which the loader copies as text.
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
    if spec.kind == enshwick_core::abi::ClientCommand::CHAT && text.is_none() {
        return false;
    }
    if spec.kind == enshwick_core::abi::ClientCommand::SET_SETTING
        && (item.is_none() || text.is_none())
    {
        return false;
    }
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
    /// Where a queued client command stands, as the test set it; as the
    /// loader, unavailable for another mod's.
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
                .find(|c| c.id == id && c.owner == me)
            {
                Some(command) => crate::test::table::put_headed(out, command.status),
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
            let mut snapshot = if s.ui.snapshot_reads.1 {
                s.ui.client_snapshot
            } else {
                enshwick_core::abi::ClientSnapshot::EMPTY
            };
            if let Some((number, buttons, owner)) = on_screen(s) {
                snapshot.available |= enshwick_core::abi::ClientSnapshot::DIALOG;
                snapshot.dialog = number;
                snapshot.dialog_buttons = buttons;
                snapshot.dialog_mod = owner
                    .and_then(|owner| crate::test::table::base::mods::id_of(s, owner))
                    .map_or(enshwick_core::abi::Str::EMPTY, enshwick_core::abi::Str::new);
            }
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

    /// The local player's slots the test set, `NOT_LOADED` while it set
    /// none; unavailable on a server, as the loader answers.
    pub(crate) fn client_slots(
        me as Caller,
        [out: *mut enshwick_core::abi::ClientSlot, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let slots = with(Err(Status::MISSING), |s| {
            if s.host.base.side == Side::SERVER {
                return Err(Status::UNAVAILABLE);
            }
            s.ui.client_slots.clone().ok_or(Status::NOT_LOADED)
        });
        match slots {
            Ok(slots) => crate::test::table::base::io::list(&slots, out, entry_size, capacity, count),
            Err(status) => status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_core::abi::{ClientCommand as Spec, Str};

    #[test]
    fn a_chat_line_that_is_not_utf8_is_refused_as_the_loader_refuses_it() {
        #[repr(C)]
        struct View {
            ptr: *const u8,
            len: usize,
        }
        let bytes = [0xff_u8, 0xfe, 0xfd];
        // SAFETY: `Str` is `repr(C)` with a pointer and then a length, as its
        // golden layout fixes; the view borrows `bytes`, alive for the test.
        let broken = unsafe {
            core::mem::transmute::<View, Str>(View {
                ptr: bytes.as_ptr(),
                len: bytes.len(),
            })
        };
        let chat = |text| {
            enshwick_abi::headed!(Spec {
                kind: Spec::CHAT,
                text,
                ..Spec::EMPTY
            })
        };
        assert!(named_as_the_loader_takes(&chat(Str::new("hello"))));
        assert!(!named_as_the_loader_takes(&chat(broken)));
    }

    #[test]
    fn a_dialog_names_its_owner_by_an_id_for_every_handle_the_fake_gave_out() {
        use enshwick_core::abi::ModHandle;

        let fake = crate::test::FakeHost::new()
            .other_mod(ModHandle(5))
            .install();
        let ids = with(Vec::new(), |s| {
            [fake.handle(), ModHandle(5), ModHandle(9)]
                .map(|me| crate::test::table::base::mods::id_of(s, me))
                .to_vec()
        });
        assert_eq!(ids, [Some(crate::test::MOD_ID), Some("test.mod-5"), None]);
    }
}
