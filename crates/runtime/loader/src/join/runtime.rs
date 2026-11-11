//! The join hook on the client.

use std::sync::OnceLock;

use enshwick_core::abi::UiDialog;

use super::super::chains::Next;

/// The online provider's join: every join from the game's menus, to a
/// dedicated server or into a hosted game's lobby, passes a target
/// ([`super::Target`]) through it, and it ends in the provider's state
/// change, whose `bool` it returns in `al` (client builds 410 and 426).
pub(crate) type JoinGame = unsafe extern "system" fn(provider: *mut u8, target: *mut u8) -> u8;

// SAFETY: the slot is linked only into the chain of the provider's join,
// whose functions all have the `JoinGame` signature.
/// The rest of the chain after the loader's link.
pub(crate) static NEXT: Next<JoinGame> = unsafe { Next::new() };

static NAMES: OnceLock<super::Names> = OnceLock::new();
static LAYOUT: OnceLock<super::Layout> = OnceLock::new();

/// Sets the build's join target layout, before the hook links.
pub(crate) fn set_layout(layout: super::Layout) {
    let _first = LAYOUT.set(layout);
}

/// Sets the names of all the client's mods.
pub(crate) fn set_names(names: super::Names) {
    let _first = NAMES.set(names);
}

/// The names of all the client's mods.
pub(crate) fn names() -> super::Names {
    NAMES.get().cloned().unwrap_or_default()
}

/// The loader's link on the online provider's join.
///
/// # Safety
///
/// - The game calls it in place of its join, on the thread that joins.
/// - `target` is null or the join target, readable and writable for the
///   call up to its password at the build's [`super::Layout::head_len`].
pub(crate) unsafe extern "system" fn detour(provider: *mut u8, target: *mut u8) -> u8 {
    let Some(layout) = LAYOUT.get() else {
        // SAFETY: the chain's rest takes the provider and target the game
        // passed this link, unchanged.
        return unsafe { NEXT.get()(provider, target) };
    };
    let allowed = target.is_null() || {
        let mut head: super::Target = [0; super::TARGET_BYTES];
        // SAFETY: `target` is non-null, checked just before, and is the join
        // target the game passes the hooked join, plain data the caller holds
        // for the call up to its password at `head_len` (client builds 410
        // and 426, the build table's `join.target_*` rows), which `Layout`
        // keeps within `head`.
        unsafe {
            enshwick_core::raw::read_bytes_at(
                target.cast_const(),
                0,
                &mut head[..layout.head_len()],
            );
        };
        std::panic::catch_unwind(|| check(layout, &head)).unwrap_or(true)
    };
    if allowed {
        super::interaction::joining(false);
        // SAFETY: the chain's rest takes the provider and target the game
        // passed this link, unchanged.
        return unsafe { NEXT.get()(provider, target) };
    }
    crate::log!(WARN, "join refused: the server runs mods this client lacks");
    super::interaction::joining(true);
    // The provider copies the target into its own state before it
    // returns, and the caller keeps its target, so the caller's copy
    // gets its host back (client builds 410 and 426).
    // SAFETY: `target` is non-null, since a null one sets `allowed`, which
    // returns early, and is the join target the game passes the hooked join,
    // whose `u64` host the build's layout places before its password.
    let host: u64 = unsafe { enshwick_core::raw::read_at(target.cast_const(), layout.host_at()) };
    // SAFETY: the same host, writable for the length of this call, and the
    // game reads it only once the chain's rest runs.
    unsafe { enshwick_core::raw::write_at(target, layout.host_at(), 0_u64) };
    // SAFETY: the chain's rest takes the provider the game passed and the
    // target with its host cleared.
    let state_changed = unsafe { NEXT.get()(provider, target) };
    // SAFETY: the caller's target outlives this call, which its caller
    // makes with the target in hand, and the game holds no reference to
    // it once the provider returned (client builds 410 and 426); its
    // host id is a u64 at the layout's `host_at`.
    unsafe { enshwick_core::raw::write_at(target, layout.host_at(), host) };
    state_changed
}

/// Joins a game whose mods the check cannot read: the client's mods stay
/// on, as the world it joins may run them.
fn unchecked() -> bool {
    crate::net::joining(false);
    crate::host::forget_server_values();
    crate::host::back_on();
    true
}

/// The address to ask a join's server at, `None` for a join the check
/// cannot ask about, with the reason logged.
fn query_address(layout: &super::Layout, target: &super::Target) -> Option<std::net::SocketAddr> {
    match layout.destination(target) {
        super::Destination::Server { query } => Some(query),
        super::Destination::Lobby => {
            // A hosted game's lobby: the lobby module reads its host's
            // mods once the client enters it.
            crate::log!(INFO, "joining a hosted game's lobby");
            None
        }
        super::Destination::NoQueryPort { game } => {
            crate::log!(
                WARN,
                "joining {game}: the game names no query port, so the join check cannot ask the server"
            );
            None
        }
    }
}

/// Compares mod sets, turns off what the server lacks and tells the player;
/// false when the client must not join. The server sends its settings once
/// the client joined.
fn check(layout: &super::Layout, target: &super::Target) -> bool {
    let Some(names) = NAMES.get() else {
        return true;
    };
    let Some(address) = query_address(layout, target) else {
        return unchecked();
    };
    let reply = super::query_rules(address).ok();
    let Some(reply) = reply else {
        // A server that does not answer the query: nothing to compare.
        crate::log!(INFO, "joining {address}: no rules answer");
        return unchecked();
    };
    // The last world's choice goes: this one decides again.
    crate::host::back_on();
    // Read now: a mod a failed hook turned off no longer counts as the
    // client's.
    let mods = &crate::host::join_mods();
    let server = super::rule(&reply, crate::advertise::RULE)
        .map(|fingerprint| (fingerprint, super::mod_rules_value(&reply)));
    crate::net::joining(server.is_some());
    let ours = crate::advertise::fingerprint(env!("CARGO_PKG_VERSION"), mods);
    let ours_mods = crate::advertise::mod_list(mods);
    crate::log!(INFO, "joining {address}: server {server:?}, ours {ours}");
    let complete = super::mods_complete(&reply);
    if !complete {
        crate::log!(
            WARN,
            "joining {address}: the server lists only part of its mods; mods it does not list stay on"
        );
    }
    let plan = super::plan(
        &ours,
        &ours_mods,
        server.as_ref().map(|(f, m)| (f.as_str(), m.as_str())),
        complete,
    );
    if plan.joins() {
        crate::host::forget_server_values();
    }
    for id in plan.turned_off() {
        crate::host::turn_off_for_world(id);
    }
    if plan.joins() {
        if let Some(note) = plan.note(names) {
            // The join goes on, and the note shows over the game.
            crate::ui::show("Enshwick", &note, "", 1 << UiDialog::OK, |_| {});
        }
        return true;
    }
    let server_mods = server.as_ref().map_or("", |(_, m)| m.as_str());
    refused(&plan.missing, server_mods, &ours_mods, complete, names);
    false
}

/// Tells the player which of the server's mods keep the client out, on the
/// mismatch screen in the menu, or in one dialog where this build cannot
/// show the screen, and tells the client's mods.
#[cold]
fn refused(
    missing: &[crate::admission::Need],
    server_mods: &str,
    ours_mods: &str,
    complete: bool,
    names: &super::Names,
) {
    let needed: Vec<String> = missing
        .iter()
        .map(|need| format!("{} {}", need.id, need.version))
        .collect();
    crate::log!(
        WARN,
        "the server runs mods this game lacks: {}; the join is refused ({})",
        needed.join(", "),
        crate::errors::JOIN_MISSING
    );
    let off = crate::host::join_mods_off();
    let off_ids: Vec<String> = off.iter().map(|(id, _)| id.clone()).collect();
    // Mods that are off on the client are the player's to turn on.
    let wanted: Vec<crate::join::missing::Wanted> = missing
        .iter()
        .filter(|need| !off_ids.contains(&need.id))
        .map(|need| crate::join::missing::Wanted {
            id: need.id.clone(),
            version: need.version.clone(),
        })
        .collect();
    if crate::ui::mismatch_screen_on() {
        let off = crate::advertise::mod_list(&off);
        crate::host::show_difference(&super::difference(server_mods, ours_mods, &off, complete));
        crate::host::report_join_missing(&wanted);
        return;
    }
    crate::log!(
        INFO,
        "the mismatch screen is off on this build; the list shows in a dialog"
    );
    let dialog =
        super::refusal::dialog(missing, &super::refusal::Off { this_game: off_ids }, names);
    // The join is refused at once, so the game keeps drawing; the mods
    // hear of it once the player closes the dialog.
    crate::ui::show(
        &dialog.title,
        &dialog.message,
        &dialog.detail,
        1 << UiDialog::OK,
        move |_| crate::host::report_join_missing(&wanted),
    );
}
