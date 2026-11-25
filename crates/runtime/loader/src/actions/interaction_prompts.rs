//! Mods' prompts on the objects placed from their items: the game verb and
//! the replaced action, written in the server's world each step right after
//! the game clears its overrides, and the mod's own text in the verb's place
//! on this client.

use std::sync::OnceLock;

use enshwick_core::abi::{HookFlags, InteractionPrompt, Registers, Side};
use enshwick_core::raw::{read_at, read_ptr_at, write_at};
use enshwick_gamedata::catalogue::Fact;

use crate::engine::builds::ABSENT;
use crate::engine::catalogue::{self as names, ids};
use crate::engine::ecs::Components;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::registry::{JoinFeatureError, Registry, sealed_is_too_late};

/// The build table ids this part of the loader names, apart from the
/// interaction filters', so a build without the prompt's pieces lacks only
/// the prompts.
pub(crate) const TABLE_IDS: &[&str] = &[
    // The query walk's, all of it, so this row names what turns it off.
    pinned::ecs::CONTEXT_ROW.id(),
    pinned::ecs::DESCRIPTOR_ENTITY_RECORDS.id(),
    pinned::ecs::ENTITY_RECORD_SIZE.id(),
    pinned::ecs::ITERATOR_INIT.id(),
    pinned::ecs::ITERATOR_NEXT.id(),
    pinned::ui::PROMPT_CONTEXT.id(),
    pinned::ui::PROMPT_SITE.id(),
    pinned::ui::PROMPT_VERB.id(),
];

/// The system that clears every object's prompt overrides each step.
const RESET_SYSTEM: &str = "interaction_override_reset";
const RESET_READS: [&str; 0] = [];
const RESET_WRITES: [&str; 1] = ["InteractionOfferRuntime"];
/// `interaction_override_reset`'s iterator: the context, the row's
/// `InteractionOfferRuntime`, then its change tracking (two stamps, entity
/// record, type and flags, tracker), which the system marks for every row
/// before it clears both overrides. Read in the decompiler on server
/// 26430253; checked against the system's reflected components on every
/// build.
const RESET_WORDS: usize = 7;
const RUNTIME: usize = 1;
/// The name of the one sub-sequence of the game's empty interaction: a
/// 1 s reach and nothing else, on every game version from 1269579 to
/// 1287369 (its id there is `0x8d57a851`).
const EMPTY_SEQUENCE: &str = "Interaction";

/// The server's writes: the verb and, under [`REPLACE_FUSE`], the action.
static PROMPTS_FUSE: Fuse = Fuse::new("interaction prompts");
/// The replaced actions, off alone when the game's data lacks the empty
/// interaction.
static REPLACE_FUSE: Fuse = Fuse::new("replaced interaction actions");
/// The mods' texts in the prompt on this client.
static TEXT_FUSE: Fuse = Fuse::new("interaction prompt texts");
/// The prompt's site on this client, which keeps its target and verb for
/// the client snapshot and runs the mods' texts.
static SITE_FUSE: Fuse = Fuse::new("the interaction prompt's target");

/// What the server's writes read, from the build.
#[derive(Debug, Clone, Copy)]
struct ServerLayout {
    components: Components,
    used_item: u32,
    /// The hash of `InteractionOfferRuntime`, the reset's row component.
    runtime: u32,
    sequence_at: usize,
    verb_at: usize,
}

/// What the client's text reads, from the build.
#[derive(Debug, Clone, Copy)]
struct ClientLayout {
    components: Components,
    used_item: u32,
    /// Where `player_ui` keeps its system context, from its frame pointer.
    context_at: usize,
    verb_at: usize,
}

/// The links' state, set once each links.
struct Links {
    /// The server's layout, the sealed prompts and the empty interaction's
    /// sequence id, `None` when the game's data lacks it.
    server: OnceLock<(ServerLayout, &'static [Prompt], Option<u32>)>,
    client: OnceLock<(ClientLayout, &'static [Prompt])>,
    /// Where the play data keeps the prompt's verb, once the site linked on
    /// this client.
    site: OnceLock<usize>,
}

static LINKS: Links = Links {
    server: OnceLock::new(),
    client: OnceLock::new(),
    site: OnceLock::new(),
};

/// One mod's prompt on the objects placed from `item`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prompt {
    owner: ModIndex,
    item: u32,
    /// The game's `LocaTagId` every client shows, 0 for the object's own.
    verb: u32,
    /// The mod's `LocaTagId` this client shows in the verb's place, 0 none.
    text: u32,
    /// The press runs the game's empty interaction.
    replace: bool,
}

impl Prompt {
    /// The prompt `spec` asks for, for mod `owner`.
    fn of(owner: ModIndex, spec: &InteractionPrompt) -> Result<Self, PromptError> {
        if !spec.is_valid() {
            return Err(PromptError::Malformed);
        }
        Ok(Self {
            owner,
            item: spec.item.get(),
            verb: spec.verb,
            text: spec.text,
            replace: spec.flags == InteractionPrompt::REPLACE_ACTION,
        })
    }
}

/// Why a mod's prompt was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PromptError {
    /// Item 0, an unknown flag, or a prompt that changes nothing.
    #[error("item 0, an unknown flag or a prompt that changes nothing")]
    Malformed,
    /// After init, on a build without the prompt's pieces, or for an item
    /// another prompt has.
    #[error(transparent)]
    Register(#[from] JoinFeatureError),
}

sealed_is_too_late!(PromptError);

/// The feature a prompt on `item` claims in the conflict report: one per
/// item, as one mod's prompt shuts out no other item's.
pub(crate) fn feature(item: enshwick_core::ItemId) -> String {
    format!("interaction prompt on item {}", item.get())
}

/// The prompts mods registered, sealed in item order.
static PROMPTS: Registry<Prompt> = Registry::new();

/// Registers mod `owner`'s prompt `spec`: one per item across all mods. A
/// build without the client's text site keeps the verb and the action and
/// turns the text off alone at the link.
///
/// # Errors
///
/// [`PromptError`] for a malformed spec, after init, for a verb or an
/// action on a build without the server's writes, or for an item another
/// prompt has.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    spec: &InteractionPrompt,
) -> Result<(), PromptError> {
    loader.before_delivery()?;
    let prompt = Prompt::of(owner, spec)?;
    if (prompt.verb != 0 || prompt.replace) && server_layout(loader).is_err() {
        return Err(JoinFeatureError::Unavailable.into());
    }
    PROMPTS.edit(|prompts| add(prompts, prompt))?
}

/// The server's layout, or the id of what this build lacks.
fn server_layout(loader: &Loader) -> Result<ServerLayout, &'static str> {
    crate::engine::ecs::walk(loader)?;
    crate::engine::ecs::components_match(loader, RESET_SYSTEM, &RESET_READS, &RESET_WRITES)
        .map_err(|_| RESET_SYSTEM)?;
    Ok(ServerLayout {
        components: Components::found(loader)?,
        used_item: names::component_hash(loader, ids::USED_ITEM).ok_or(ids::USED_ITEM)?,
        runtime: names::component_hash(loader, ids::INTERACTION_OFFER)
            .ok_or(ids::INTERACTION_OFFER)?,
        sequence_at: names::offset(loader, ids::INTERACTION_OFFER_OVERRIDE_SEQUENCE)
            .ok_or(ids::INTERACTION_OFFER_OVERRIDE_SEQUENCE)?,
        verb_at: names::offset(loader, ids::INTERACTION_OFFER_OVERRIDE_VERB)
            .ok_or(ids::INTERACTION_OFFER_OVERRIDE_VERB)?,
    })
}

/// The client's layout, or the id of what this build lacks.
fn client_layout(loader: &Loader) -> Result<ClientLayout, &'static str> {
    let table = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let site = pinned::ui::PROMPT_SITE;
    loader.address(site).ok_or(site.id())?;
    Ok(ClientLayout {
        components: Components::found(loader)?,
        used_item: names::component_hash(loader, ids::USED_ITEM).ok_or(ids::USED_ITEM)?,
        context_at: table(pinned::ui::PROMPT_CONTEXT)?,
        verb_at: table(pinned::ui::PROMPT_VERB)?,
    })
}

/// Links the server's writes and this client's texts for the sealed
/// prompts, each where a prompt needs it.
pub(crate) fn link(loader: &'static Loader) {
    let prompts = seal();
    if prompts.iter().any(|p| p.verb != 0 || p.replace) {
        link_server(loader, prompts);
    }
    if loader.side == Side::CLIENT {
        link_client(loader, prompts);
    }
}

/// Links the reset on every side, as any can host a world.
fn link_server(loader: &'static Loader, prompts: &'static [Prompt]) {
    if crate::engine::ecs::walk(loader).is_ok()
        && let Err(why) =
            crate::engine::ecs::components_match(loader, RESET_SYSTEM, &RESET_READS, &RESET_WRITES)
    {
        loader.fuse_off(&PROMPTS_FUSE, RESET_SYSTEM, &why.to_string());
        return;
    }
    let layout = match server_layout(loader) {
        Ok(layout) => layout,
        Err(id) => {
            loader.fuse_off(&PROMPTS_FUSE, id, ABSENT);
            return;
        }
    };
    let empty = if prompts.iter().any(|p| p.replace) {
        let found = empty_sequence(loader);
        if found.is_none() {
            loader.fuse_off(
                &REPLACE_FUSE,
                ids::ACTOR_SEQUENCE,
                "the game's data has not exactly one empty interaction",
            );
        }
        found
    } else {
        None
    };
    let _first = LINKS.server.set((layout, prompts, empty));
    let entry = loader
        .reflection
        .system(RESET_SYSTEM)
        .ok()
        .map(enshwick_gamedata::reflect::System::entry);
    if loader.link_feature(
        &PROMPTS_FUSE,
        Target::Found(RESET_SYSTEM, entry),
        &RESET_NEXT,
        reset,
        HookFlags::NONE,
    ) {
        crate::log!(INFO, "interaction prompts on {} items", prompts.len());
    }
}

/// Links the prompt's site on this client, for the client snapshot's read
/// of its target, which a mod may ask for at any time, and for the mods'
/// texts where a prompt has one.
fn link_client(loader: &'static Loader, prompts: &'static [Prompt]) {
    let texts = prompts.iter().any(|p| p.text != 0);
    if texts {
        match client_layout(loader) {
            Ok(layout) => {
                let _first = LINKS.client.set((layout, prompts));
            }
            Err(id) => {
                loader.fuse_off(&TEXT_FUSE, id, ABSENT);
            }
        }
    }
    let Some(verb_at) = loader.offset(pinned::ui::PROMPT_VERB) else {
        loader.fuse_off(&SITE_FUSE, pinned::ui::PROMPT_VERB.id(), ABSENT);
        return;
    };
    let linked = loader.link_mid(
        &SITE_FUSE,
        Target::Pinned(pinned::ui::PROMPT_SITE),
        prompt_site,
        HookFlags::NONE,
    );
    if linked {
        let _first = LINKS.site.set(verb_at);
    } else if texts {
        loader.fuse_off(&TEXT_FUSE, pinned::ui::PROMPT_SITE.id(), ABSENT);
    }
}

/// The sequence id of the game's empty interaction: the one sequence whose
/// first sub-sequence is named [`EMPTY_SEQUENCE`], read from the game's
/// archive; `None` without the archive or exactly one.
fn empty_sequence(loader: &Loader) -> Option<u32> {
    let archive = crate::engine::archive::game()?;
    let found = empty_sequence_in(archive, &loader.reflection.registry());
    archive.trim();
    found
}

/// [`empty_sequence`] in `archive`, its types read through `registry`.
fn empty_sequence_in(
    archive: &crate::engine::archive::Archive,
    registry: &enshwick_core::resources::Registry<'_>,
) -> Option<u32> {
    let field = |id| match enshwick_gamedata::catalogue::fact(id) {
        Some(Fact::Offset { field, .. }) => Some(field),
        _ => None,
    };
    let (subs, name) = (
        field(ids::ACTOR_SEQUENCE_SUB_SEQUENCES)?,
        field(ids::SEQUENCE_NAME)?,
    );
    let ty = names::type_name(ids::ACTOR_SEQUENCE);
    let hash = enshwick_gamedata::catalogue::type_hash(ids::ACTOR_SEQUENCE)?;
    let found: Vec<u32> = archive
        .keys_of(hash)
        .iter()
        .filter(|key| {
            archive
                .resource(key)
                .and_then(|bytes| registry.decode(ty, &bytes).ok())
                .is_some_and(|sequence| first_name(&sequence, subs, name) == Some(EMPTY_SEQUENCE))
        })
        .map(|key| key.guid.fnv1a())
        .collect();
    match found.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}

/// The name of `sequence`'s first sub-sequence, by the fields `subs` and
/// `name`.
fn first_name<'v>(
    sequence: &'v enshwick_core::resources::Value,
    subs: &str,
    name: &str,
) -> Option<&'v str> {
    use enshwick_core::resources::Value;
    let Value::List(list) = sequence.field(subs)? else {
        return None;
    };
    match list.first()?.field(name)? {
        Value::Text(text) => Some(text),
        _ => None,
    }
}

crate::engine::chains::chain_detour! {
    /// The reflected `interaction_override_reset` system's entry: once the
    /// game cleared every object's overrides, writes the mods' prompts into
    /// theirs, so each system after it in the step, the accept and the offer
    /// among them, reads them whatever their order.
    fn reset(ctx: *mut u64) -> ();
    next RESET_NEXT;
    fuse &PROMPTS_FUSE, "the prompt overrides";
    |onward| {
        onward.call();
        // SAFETY: `ctx` is the game's argument to the reset system, after
        // its own run and before it returns, on its thread.
        unsafe { write_prompts(ctx) };
    }
}

/// Writes the verb and the action of every row the reset of `ctx` holds
/// whose object was placed from an item with a prompt.
///
/// Its cost, timed live on client 26430625 hosting a world with two
/// prompted objects (2026-11-17, release build), around this function
/// only: two timer reads of about 40 ns are in the figures, the detour's
/// own chaining is not. 840 to 900 runs a second of about 2 rows each
/// took 330 to 400 ns a run and 150 to 200 ns a row (the game's own reset
/// took 176 to 209 ns a run), so about 0.3 ms a second, 0.03% of a 60 Hz
/// step. The slowest run took 336 µs once in the first minute and 24 to
/// 118 µs in each later minute. The cost grows with the rows the reset
/// visits: 15 rows a run comes to about 0.2%, and 1% of a step, where a
/// per-row cache would pay, near 60 to 75 rows a run. Only a hosted
/// single-player world was timed, not a dedicated server with many bases.
///
/// Once any mod registers a verb or a replaced action, [`link`] links this
/// walk on every side, and it reads every row whether or not an object
/// with a prompt exists or its mod is on in this world.
///
/// # Safety
///
/// - `ctx` is null or the live query context the game passed to
///   `interaction_override_reset`, used on that system's thread after its
///   own run and before it returns.
unsafe fn write_prompts(ctx: *mut u64) {
    let (Some((layout, prompts, empty)), Some(engine)) =
        (LINKS.server.get(), crate::engine::ecs::walked())
    else {
        return;
    };
    // The reset's context does not name its world (client 26430625, hosted
    // game, seen live 2026-11-16), so its rows are read in the server's, and
    // only on the server's game thread, where its world cannot change under
    // the read or be freed.
    if crate::engine::world::reader().is_err() {
        return;
    }
    let world = usize::try_from(crate::engine::world::server_world()).unwrap_or(0);
    if ctx.is_null() || world == 0 {
        return;
    }
    let empty = empty.filter(|_| !REPLACE_FUSE.is_off());
    // SAFETY: `ctx` is the reset's live, non-null context on its thread
    // (the caller's contract).
    let row = unsafe { engine.row(ctx) };
    // SAFETY: `ctx` is the reset's live, non-null context on its thread
    // (the caller's contract); the system's own run is over, so only this
    // thread writes its row counter, which is put back below.
    unsafe { engine.set_row(ctx, 0) };
    {
        // SAFETY: `ctx` is the reset's live context on its thread (the
        // caller's contract); `RESET_WORDS` is its iterator size, the layout
        // `server_layout` checks against its reflected components; `query`
        // drops at the end of this block, before the system returns.
        let mut query = unsafe { engine.query::<RESET_WORDS>(ctx) };
        while let Some(words) = query.next() {
            let runtime = core::ptr::with_exposed_provenance_mut::<u8>(
                usize::try_from(words[RUNTIME]).unwrap_or(0),
            );
            if runtime.is_null() {
                continue;
            }
            // SAFETY: `ctx` is the reset's live context on its thread, on the
            // row `query` just moved to.
            let entity = unsafe { engine.entity(ctx) };
            // SAFETY: `world` is the server's live, non-zero entity world,
            // read on the thread of its running reset system.
            let Some(used) = (unsafe {
                layout
                    .components
                    .component_in(world, entity, layout.used_item)
            }) else {
                continue;
            };
            // SAFETY: `used` is the entity's live `UsedItem` component, which
            // is the u32 item id alone.
            let item: u32 = unsafe { read_at(used, 0) };
            let Some(prompt) = live(prompts, item) else {
                continue;
            };
            // Only the server's own row of the entity: a row of another world
            // may carry an id the server's world gives another entity.
            // SAFETY: `world` is the server's live, non-zero entity world,
            // read on the thread of its running reset system.
            let own = unsafe {
                layout
                    .components
                    .component_in(world, entity, layout.runtime)
            };
            if own != Some(runtime.cast_const()) {
                continue;
            }
            if prompt.verb != 0 {
                // SAFETY: `runtime` is the row's non-null
                // `InteractionOfferRuntime`, which this system writes, on its
                // thread; its u32 `overrideVerbLocaId` sits at the
                // catalogue's offset.
                unsafe { write_at(runtime, layout.verb_at, prompt.verb) };
            }
            if let (true, Some(empty)) = (prompt.replace, empty) {
                // SAFETY: `runtime` is the row's non-null
                // `InteractionOfferRuntime`, which this system writes, on its
                // thread; its u32 `overrideSequence` sits at the catalogue's
                // offset.
                unsafe { write_at(runtime, layout.sequence_at, empty) };
            }
        }
    }
    // SAFETY: `ctx` is the reset's live, non-null context on its thread
    // (the caller's contract), whose row counter only this thread writes
    // now; `row` is the value the system left in it.
    unsafe { engine.set_row(ctx, row) };
}

crate::engine::mid::mid_handler! {
    /// The table's `ui.prompt_site` in `player_ui`, after the prompt's
    /// title and verb are stored and before a lock may put its reason in the
    /// verb's place: keeps its target and verb for the client snapshot once
    /// a mod asked for it, then shows a mod's text there for its objects.
    ///
    /// It runs once a frame while any prompt shows and never otherwise, on
    /// every client, as a mod may ask for the snapshot at any time; until
    /// one asks and while no prompt has a text, its gate is two atomic loads.
    /// Timed live on client 26430625 (2026-11-17, release build) at 106 to
    /// 109 frames a second, around [`show_text`] only, without the stub's
    /// save and restore of the vector state and its dispatch: about 440 ns
    /// a hit on a vanilla object and 347 to 366 ns on a mod's object where
    /// it writes the text, at most 6 to 20 µs, so about 40 µs a second,
    /// under 0.01% of a 60 Hz step.
    fn prompt_site;
    gate wanted();
    fuse SITE_FUSE, "the prompt's site";
    |registers| {
        // SAFETY: the stub calls the handler at `ui.prompt_site` with that
        // site's registers, on `player_ui`'s thread.
        unsafe { note_target(registers) };
        if LINKS.client.get().is_some() {
            let _shown = TEXT_FUSE.run("the prompt's text", || {
                // SAFETY: the stub calls the handler at `ui.prompt_site`
                // with that site's registers, on `player_ui`'s thread.
                unsafe { show_text(registers) };
            });
        }
    }
}

/// For the benchmark of what every client pays at the prompt's site once a
/// frame while a prompt shows, with no mod asking for the client snapshot
/// and no prompt text: the handler's gate, as the stub calls it, past which
/// it reads nothing. Whether the snapshot stayed unasked.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_prompt_site_idle() -> impl FnMut() -> bool {
    let mut registers = Registers::default();
    move || {
        // SAFETY: the handler takes the stub's saved registers and its user
        // pointer, which it ignores; these are a live set on this thread,
        // and with nothing asked its gate returns before it reads them.
        unsafe { prompt_site(&raw mut registers, core::ptr::null_mut()) };
        !crate::actions::client_snapshot::asked()
    }
}

/// Whether the prompt's site keeps its target for the client snapshot on
/// this client: it linked and is on.
pub(crate) fn site_reads() -> bool {
    LINKS.site.get().is_some() && !SITE_FUSE.is_off()
}

/// Whether the prompt's site has work: a mod asked for the client snapshot,
/// or a prompt has a text.
fn wanted() -> bool {
    crate::actions::client_snapshot::asked() || LINKS.client.get().is_some()
}

/// Keeps the prompt's target and the verb its object offers, before a mod's
/// text takes the verb's place, for the client snapshot once a mod asked
/// for it.
///
/// # Safety
///
/// - `registers` are those at `ui.prompt_site` in `player_ui`, on its
///   thread: `rsi` the interface's play data, `edi` the prompt's target in
///   the client's world (client 26430625, seen live on 2026-11-25).
unsafe fn note_target(registers: &Registers) {
    let Some(&verb_at) = LINKS.site.get() else {
        return;
    };
    if !crate::actions::client_snapshot::asked() {
        return;
    }
    #[expect(clippy::cast_possible_truncation, reason = "the target is `edi`")]
    let target = registers.rdi as u32;
    let play =
        core::ptr::with_exposed_provenance::<u8>(usize::try_from(registers.rsi).unwrap_or(0));
    if play.is_null() {
        return;
    }
    // SAFETY: `play` is the interface's non-null play data at the site (the
    // caller's contract), which `player_ui` writes on this thread; its u32
    // verb sits at the table's `ui.prompt_verb`.
    let verb: u32 = unsafe { read_at(play, verb_at) };
    crate::actions::client_snapshot::note_prompt(target, verb);
}

/// Puts the mod's text tag in the verb's place when the prompt's target
/// was placed from an item whose prompt has one.
///
/// # Safety
///
/// - `registers` are those at `ui.prompt_site` in `player_ui`, on its
///   thread: `rsi` the interface's play data, `edi` the prompt's target,
///   `rbp` the frame pointer (client 26430625, the same on 26320373).
unsafe fn show_text(registers: &Registers) {
    let Some((layout, prompts)) = LINKS.client.get() else {
        return;
    };
    #[expect(clippy::cast_possible_truncation, reason = "the target is `edi`")]
    let target = registers.rdi as u32;
    let play =
        core::ptr::with_exposed_provenance_mut::<u8>(usize::try_from(registers.rsi).unwrap_or(0));
    let frame =
        core::ptr::with_exposed_provenance::<u8>(usize::try_from(registers.rbp).unwrap_or(0));
    if target == 0 || play.is_null() || frame.is_null() {
        return;
    }
    // SAFETY: `frame` is `player_ui`'s non-null frame pointer at the site
    // (the caller's contract), and the system keeps its context in its home
    // slot at the table's `ui.prompt_context` from it.
    let ctx = unsafe { read_ptr_at::<u64>(frame, layout.context_at) };
    if ctx.is_null() {
        return;
    }
    // SAFETY: `ctx` is `player_ui`'s live, non-null context, used on its
    // thread while the system runs.
    let Some(used) = (unsafe { layout.components.component(ctx, target, layout.used_item) }) else {
        return;
    };
    // SAFETY: `used` is the target's live `UsedItem` component, which is the
    // u32 item id alone.
    let item: u32 = unsafe { read_at(used, 0) };
    if let Some(prompt) = live(prompts, item).filter(|p| p.text != 0) {
        // SAFETY: `play` is the interface's non-null play data at the site
        // (the caller's contract), which `player_ui` writes on this thread;
        // its u32 verb sits at the table's `ui.prompt_verb`.
        unsafe { write_at(play, layout.verb_at, prompt.text) };
    }
}

/// Adds `prompt` unless its item has one.
fn add(prompts: &mut Vec<Prompt>, prompt: Prompt) -> Result<(), PromptError> {
    if prompts.iter().any(|p| p.item == prompt.item) {
        return Err(JoinFeatureError::Taken.into());
    }
    prompts.push(prompt);
    Ok(())
}

/// Seals the prompts in item order, for [`find`].
pub(crate) fn seal() -> &'static [Prompt] {
    PROMPTS.seal_with(|mut prompts| {
        prompts.sort_unstable_by_key(|p| p.item);
        prompts
    })
}

/// The prompt on objects placed from `item`, in prompts sealed by [`seal`].
fn find(prompts: &[Prompt], item: u32) -> Option<&Prompt> {
    prompts
        .binary_search_by_key(&item, |p| p.item)
        .ok()
        .and_then(|at| prompts.get(at))
}

/// [`find`]'s prompt while its mod is on: a mod turned off, or off for
/// this world, changes no prompt.
fn live(prompts: &[Prompt], item: u32) -> Option<&Prompt> {
    find(prompts, item).filter(|p| !crate::engine::loader::is_off(p.owner))
}

#[cfg(test)]
mod tests {
    use enshwick_core::ItemId;
    use enshwick_core::abi::Header;

    use super::*;

    fn spec(item: u32, verb: u32, text: u32, flags: u32) -> InteractionPrompt {
        InteractionPrompt {
            header: Header::of::<InteractionPrompt>(),
            item: ItemId::new(item),
            verb,
            text,
            flags,
        }
    }

    #[test]
    fn a_prompt_needs_an_item_known_flags_and_something_to_change() {
        let mine = ModIndex::new(0);
        assert_eq!(
            Prompt::of(mine, &spec(0, 7, 0, 0)),
            Err(PromptError::Malformed)
        );
        assert_eq!(
            Prompt::of(mine, &spec(5, 7, 0, 2)),
            Err(PromptError::Malformed)
        );
        assert_eq!(
            Prompt::of(mine, &spec(5, 0, 0, 0)),
            Err(PromptError::Malformed)
        );
        for (verb, text, flags) in [(7, 0, 0), (0, 9, 0), (0, 0, 1)] {
            let prompt = Prompt::of(mine, &spec(5, verb, text, flags));
            assert_eq!(
                prompt.map(|p| (p.item, p.verb, p.text, p.replace)),
                Ok((5, verb, text, flags == 1))
            );
        }
    }

    #[test]
    fn a_second_prompt_on_an_item_is_refused_whichever_mod_asks() {
        let mut prompts = Vec::new();
        let first = Prompt::of(ModIndex::new(0), &spec(5, 7, 0, 0)).unwrap();
        let again = Prompt::of(ModIndex::new(1), &spec(5, 0, 9, 1)).unwrap();
        assert_eq!(add(&mut prompts, first), Ok(()));
        assert_eq!(
            add(&mut prompts, again),
            Err(PromptError::Register(JoinFeatureError::Taken))
        );
        assert_eq!(prompts, [first]);
    }

    /// On the server `ENSH_SERVER_EXE` names, with its archive beside it
    /// (skipped without them): the game's data has exactly one empty
    /// interaction, the sequence the research found (servers 1269579 to
    /// 26430253).
    #[test]
    fn the_server_archive_has_one_empty_interaction() {
        use enshwick_gamedata::reflect::{Image, Layout, Reflection};

        let Ok(path) = std::env::var("ENSH_SERVER_EXE") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        let Some(archive) = path
            .parent()
            .and_then(|dir| crate::engine::archive::Archive::open(dir, "enshrouded_server"))
        else {
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        assert_eq!(
            empty_sequence_in(&archive, &reflection.registry()),
            Some(0x8d57_a851)
        );
    }

    #[test]
    fn prompts_on_two_items_claim_two_features() {
        assert_ne!(feature(ItemId::new(1)), feature(ItemId::new(2)));
        assert_eq!(feature(ItemId::new(1)), feature(ItemId::new(1)));
    }

    #[test]
    fn a_prompt_of_a_mod_that_is_off_changes_nothing() {
        // No mod of this index runs, so it reads as off.
        let gone = Prompt::of(ModIndex::new(400), &spec(5, 7, 9, 1)).unwrap();
        let prompts = [gone];
        assert_eq!(find(&prompts, 5), Some(&gone));
        assert_eq!(live(&prompts, 5), None);
    }

    #[test]
    fn a_build_without_the_text_site_keeps_a_text_and_refuses_a_verb() {
        let loader = crate::start::tests::pinning_loader(Side::CLIENT, &[], &[]);
        let mine = ModIndex::new(0);
        assert_eq!(
            register(&loader, mine, &spec(0xC0DE_0563, 0, 9, 0)),
            Ok(()),
            "the text turns off alone at the link"
        );
        assert_eq!(
            register(&loader, mine, &spec(0xC0DE_0564, 7, 9, 0)),
            Err(PromptError::Register(JoinFeatureError::Unavailable))
        );
    }

    #[test]
    fn a_build_without_the_entity_engine_lacks_the_server_s_writes() {
        let loader = crate::start::tests::bare_loader();
        assert_eq!(
            server_layout(&loader).map(|_| ()),
            Err(pinned::ecs::ITERATOR_INIT.id())
        );
    }

    #[test]
    fn sealed_prompts_are_found_by_item_and_others_are_not() {
        let prompts: Vec<Prompt> = [9, 3, 5]
            .into_iter()
            .map(|item| Prompt::of(ModIndex::new(0), &spec(item, 1, 0, 0)).unwrap())
            .collect();
        let mut sorted = prompts.clone();
        sorted.sort_unstable_by_key(|p| p.item);
        for item in [3, 5, 9] {
            assert_eq!(find(&sorted, item).map(|p| p.item), Some(item));
        }
        for item in [0, 4, 10] {
            assert_eq!(find(&sorted, item), None);
        }
    }
}
