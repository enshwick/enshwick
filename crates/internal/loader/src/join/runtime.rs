//! The join hook on the client.

use super::super::chains::Next;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use enshwick::abi::UiDialog;

/// The online provider's join: every join from the game's menus, to a
/// dedicated server or into a hosted game's lobby, passes a target
/// ([`super::Target`]) through it, and it ends in the provider's state
/// change, whose `bool` it returns in `al` (client builds 410 and 426).
pub(crate) type JoinGame = unsafe extern "system" fn(provider: *mut u8, target: *mut u8) -> u8;

// SAFETY: the slot joins the chain of the join, typed `JoinGame`.
/// The rest of the chain after the loader's link.
pub(crate) static NEXT: Next<JoinGame> = unsafe { Next::new() };

static NAMES: OnceLock<super::Names> = OnceLock::new();

/// Sets the names of all the client's mods.
pub(crate) fn set_names(names: super::Names) {
    let _first = NAMES.set(names);
}

/// The names of all the client's mods.
pub(crate) fn names() -> super::Names {
    NAMES.get().cloned().unwrap_or_default()
}

/// The loader's link on the online provider's join.
pub(crate) unsafe extern "system" fn detour(provider: *mut u8, target: *mut u8) -> u8 {
    let allowed = target.is_null() || {
        // SAFETY: the game's join target holds at least `TARGET_BYTES`
        // of plain data (client builds 410 and 426).
        let head = unsafe { target.cast::<super::Target>().read_unaligned() };
        std::panic::catch_unwind(|| check(&head)).unwrap_or(true)
    };
    if allowed {
        super::interaction::joining(false);
        // SAFETY: forwarded unchanged.
        return unsafe { NEXT.get()(provider, target) };
    }
    crate::log!(WARN, "join refused: the server runs mods this client lacks");
    super::interaction::joining(true);
    // The provider copies the target into its own state before it
    // returns, and the caller keeps its target, so the caller's copy
    // gets its host back (client builds 410 and 426).
    // SAFETY: the target is the caller's join target, writable for the
    // length of this call and not null; the reference ends here, before
    // the game reads the target.
    let host = super::refuse(unsafe { &mut *target.cast::<super::Target>() });
    // SAFETY: forwarded with the host taken out.
    let state_changed = unsafe { NEXT.get()(provider, target) };
    // SAFETY: the caller's target outlives this call, which its caller
    // makes with the target in hand, and the game holds no reference to
    // it once the provider returned (client builds 410 and 426); its
    // host id is an unaligned u64 at `HOST`.
    unsafe {
        target
            .wrapping_add(super::HOST)
            .cast::<u64>()
            .write_unaligned(host);
    }
    state_changed
}

/// Joins a game whose mods the check cannot read, on the client's own
/// join values.
fn unchecked() -> bool {
    crate::net::joining(false);
    crate::host::apply_join_values(&BTreeMap::new());
    true
}

/// The address to ask a join's server at, `None` for a join the check
/// cannot ask about, with the reason logged.
fn query_address(target: &super::Target) -> Option<std::net::SocketAddr> {
    match super::destination(target) {
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

/// Compares mod sets, turns off what the server lacks, takes the server's
/// join values and tells the player; false when the client must not join.
fn check(target: &super::Target) -> bool {
    let Some(own_names) = NAMES.get() else {
        return true;
    };
    let Some(address) = query_address(target) else {
        return unchecked();
    };
    // Read now: a mod an earlier server or a failed hook turned off no
    // longer counts as the client's.
    let mods = &crate::host::join_mods();
    let reply = super::query_rules(address).ok();
    let Some(reply) = reply else {
        // A server that does not answer the query: nothing to compare.
        crate::log!(INFO, "joining {address}: no rules answer");
        return unchecked();
    };
    let server = super::rule(&reply, crate::advertise::RULE)
        .map(|fingerprint| (fingerprint, super::mod_rules_value(&reply)));
    crate::net::joining(server.is_some());
    // The client runs with the server's join values, so its own
    // fingerprint counts them.
    let values = if server.is_some() {
        super::server_values(&reply, mods.iter().map(|(id, _)| id.as_str()))
    } else {
        BTreeMap::new()
    };
    let ours = crate::advertise::fingerprint(env!("CARGO_PKG_VERSION"), mods, &values);
    let ours_mods = crate::advertise::mod_list(mods);
    crate::log!(INFO, "joining {address}: server {server:?}, ours {ours}");
    let mut names = own_names.clone();
    if let Some((_, server_mods)) = &server {
        for (id, _) in super::wanted(server_mods, "") {
            let name = super::name(&reply, &id);
            if !name.is_empty() {
                names.entry(id).or_insert(name);
            }
        }
    }
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
        crate::host::apply_join_values(&values);
    }
    for id in plan.turned_off() {
        crate::host::turn_off_mod(
            id,
            "the server does not run it; restart the game to use it again",
        );
        crate::reload::hold(id);
    }
    if plan.joins() {
        if let Some(note) = plan.note(&names) {
            // The join goes on, and the note shows over the game.
            crate::ui::show("Enshwick", &note, "", 1 << UiDialog::OK, |_| {});
        }
        return true;
    }
    refused(&plan.missing, &reply, &names);
    false
}

/// Tells the player, in one dialog, which of the server's mods keep the
/// client out, and tells the client's mods.
#[cold]
fn refused(missing: &[crate::admission::Need], reply: &[u8], names: &super::Names) {
    let off = super::refusal::Off {
        by_player: crate::modlist::disabled().to_vec(),
        this_game: crate::host::join_mods_off(),
    };
    let pages: BTreeMap<String, String> = missing
        .iter()
        .filter_map(|need| Some((need.id.clone(), super::page(reply, &need.id)?)))
        .collect();
    let dialog = super::refusal::dialog(missing, &off, names, &pages);
    // Mods that are off on the client are the player's to turn on.
    let wanted: Vec<crate::host::Wanted> = missing
        .iter()
        .filter(|need| !off.by_player.contains(&need.id) && !off.this_game.contains(&need.id))
        .map(|need| crate::host::Wanted {
            name: super::name(reply, &need.id),
            page: pages.get(&need.id).cloned().unwrap_or_default(),
            id: need.id.clone(),
            version: need.version.clone(),
        })
        .collect();
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
