//! Vetoes on crafting and building, asked of the mods' filters before the
//! game acts.
//!
//! A craft request is a `CraftRecipeEvent` the server's `player_crafting`
//! system raises from the player's input; the crafting system reads it in
//! the next step. The loader screens the requests in the event streams on
//! the game thread, before the engine moves them, and empties a refused one:
//! no crafter and no amount, so the crafting system has nothing to make and
//! the ingredients stay where they are.
//!
//! A building placement passes the game's cursor check, which the server
//! runs on the cursor each player's client sends and the client runs for its
//! own prediction, on worker threads. The loader runs the filters after the
//! check and adds the game's own "no permission" flag to a refused
//! placement, which the building systems refuse to place and the client's
//! cursor shows as not allowed. The builder's account comes from the copy of
//! the player slots the loader takes before each server step
//! ([`crate::engine::world::accounts`]), since world reads are refused on
//! those threads; a client's own prediction is its local player's, so a
//! client that hosts the game gives it its own player's account.

use core::ffi::c_void;
use core::ops::ControlFlow;
use std::sync::OnceLock;

use enshwick_core::abi::{
    BuildCheck, BuildFilterFn, CraftCheck, CraftFilterFn, Header, HookFlags, Str,
};
use enshwick_core::raw::{read_at, read_ptr_at, write_at};

use crate::engine::filters::Filters;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::parts::EventFollower;
use crate::engine::pinned;
use crate::engine::profile::Feature;
use crate::engine::registry::JoinFeatureError;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::building::CURSOR_CHECK.id(),
    pinned::building::CURSOR_ITEM_INFO.id(),
    pinned::building::CURSOR_NO_PERMISSION.id(),
];

static CRAFT_FILTERS: Filters<CraftFilterFn> = Filters::new(Feature::Vetoes);
static BUILD_FILTERS: Filters<BuildFilterFn> = Filters::new(Feature::Vetoes);
static CRAFT: OnceLock<CraftLayout> = OnceLock::new();
static BUILD: OnceLock<BuildLayout> = OnceLock::new();
/// The switch of the craft filters, and of the screen of craft requests
/// disabled recipes rely on too.
static CRAFT_FUSE: Fuse = Fuse::new("craft filters");
/// The switch of the build filters and their link on the cursor check.
static BUILD_FUSE: Fuse = Fuse::new("build filters");
/// The reflected type of a craft request.
const CRAFT_EVENT: &str = "keen::ecs::CraftRecipeEvent";

/// `CraftRecipeEvent`'s hash and fields, from reflection.
#[derive(Debug, Clone, Copy)]
struct CraftLayout {
    hash: u32,
    size: usize,
    player: usize,
    station: usize,
    recipe: usize,
    amount: usize,
}

/// The cursor check's placement and flags, from the build table.
#[derive(Debug, Clone, Copy)]
struct BuildLayout {
    item_info: usize,
    no_permission: u32,
}

/// Registers mod `owner`'s craft filter.
///
/// # Errors
///
/// [`JoinFeatureError`] after init or on a build without craft requests.
pub(crate) fn register_craft(
    loader: &Loader,
    owner: ModIndex,
    callback: CraftFilterFn,
    user: *mut c_void,
) -> Result<(), JoinFeatureError> {
    let available = || craft_layout(loader).is_some();
    CRAFT_FILTERS.register_when(available, owner, callback, user)
}

/// Registers mod `owner`'s build filter.
///
/// # Errors
///
/// [`JoinFeatureError`] after init or on a build without the cursor check.
pub(crate) fn register_build(
    loader: &Loader,
    owner: ModIndex,
    callback: BuildFilterFn,
    user: *mut c_void,
) -> Result<(), JoinFeatureError> {
    let available = || {
        build_layout(loader).is_some()
            && loader.address(pinned::building::CURSOR_CHECK).is_some()
            && crate::engine::ecs::engine(loader).is_some()
    };
    BUILD_FILTERS.register_when(available, owner, callback, user)
}

fn craft_layout(loader: &Loader) -> Option<CraftLayout> {
    let name = CRAFT_EVENT;
    let reflection = &loader.reflection;
    let info = reflection.type_info(name).ok()?;
    let field = |field: &str| {
        reflection
            .field_offset(name, field)
            .ok()
            .and_then(|v| usize::try_from(v).ok())
    };
    Some(CraftLayout {
        hash: crate::engine::world::type_hash(info.va())?,
        size: usize::try_from(info.size()).ok()?,
        player: field("craftingOperatorId")?,
        station: field("craftingStationId")?,
        recipe: field("recipeId")?,
        amount: field("recipeAmount")?,
    })
}

fn build_layout(loader: &Loader) -> Option<BuildLayout> {
    Some(BuildLayout {
        item_info: loader.offset(pinned::building::CURSOR_ITEM_INFO)?,
        no_permission: u32::try_from(
            loader
                .build
                .constant(pinned::building::CURSOR_NO_PERMISSION.id())?,
        )
        .ok()?,
    })
}

/// The craft request screen in the event move: it reads the craft request
/// once the build reflects it, for mods' filters and disabled recipes alike,
/// so with no filter too, since recipe rules and the admin command may
/// disable a recipe at any world load and a client may host one. Only the
/// move screens craft requests, so its being off lists the filters off.
pub(crate) const FOLLOWER: EventFollower = EventFollower {
    types: |_| CRAFT.get().map(|layout| vec![layout.hash]),
    screen: Some(screen),
    follow: None,
    fuse: Some(&CRAFT_FUSE),
};

/// Seals the build and craft filters and finds the craft request's layout,
/// before any link: game events link the event move when craft requests are
/// screened. Craft requests are screened whenever the build reads them, as
/// any recipe may be disabled later.
pub(crate) fn seal(loader: &Loader) {
    let _builds = BUILD_FILTERS.seal();
    let crafts = CRAFT_FILTERS.seal();
    if let Some(layout) = craft_layout(loader) {
        let _first = CRAFT.set(layout);
        if !crafts.is_empty() {
            crate::log!(INFO, "craft filters on: {}", crafts.len());
        }
    } else {
        let _off = loader.fuse_off(
            &CRAFT_FUSE,
            CRAFT_EVENT,
            "or a field of it is not reflected on this build; craft requests are not screened",
        );
    }
}

/// Links the cursor check when a mod registered a build filter before the
/// seal phase.
pub(crate) fn link(loader: &'static Loader) {
    let builds = BUILD_FILTERS.sealed().unwrap_or_default();
    if builds.is_empty() {
        return;
    }
    // A filter registers only on a build with the layout.
    let Some(layout) = build_layout(loader) else {
        return;
    };
    let _first = BUILD.set(layout);
    let target = Target::Pinned(pinned::building::CURSOR_CHECK);
    if loader.link_feature(&BUILD_FUSE, target, &CURSOR_NEXT, cursor, HookFlags::NONE) {
        crate::engine::world::accounts::want();
        crate::log!(INFO, "build filters on: {}", builds.len());
    }
}

/// Screens one event record of the server's streams on the game thread:
/// empties a craft request a filter refuses.
///
/// # Safety
///
/// - `payload` is a live event of `type_hash`, readable and writable for
///   `size` bytes.
/// - Called on the game thread before the engine moves the streams, so
///   nothing else reads or writes the event during the call.
unsafe fn screen(type_hash: u32, payload: *mut u8, size: usize) {
    let Some(layout) = CRAFT.get() else {
        return;
    };
    if type_hash != layout.hash || size < layout.size {
        return;
    }
    // SAFETY: the caller's contract, for a `CraftRecipeEvent` of at least
    // its reflected size (`size < layout.size` returns first).
    let _screened = CRAFT_FUSE.run("the craft request", || unsafe { request(layout, payload) });
}

/// Empties the craft request `payload` when a filter refuses it or its
/// recipe is disabled.
///
/// # Safety
///
/// - `payload` is a live `CraftRecipeEvent`, readable and writable for
///   `layout.size` bytes, which nothing else reads or writes during the
///   call.
unsafe fn request(layout: &CraftLayout, payload: *mut u8) {
    if !CRAFT_FILTERS.is_sealed() {
        return;
    }
    // SAFETY: the event is a `CraftRecipeEvent` readable for its reflected
    // size (the caller's contract), and reflection places its `u32` `craftingOperatorId` at
    // `player`.
    let player: u32 = unsafe { read_at(payload, layout.player) };
    // SAFETY: the event is a `CraftRecipeEvent` readable for its reflected
    // size (the caller's contract), and reflection places its `u32` `craftingStationId` at
    // `station`.
    let station: u32 = unsafe { read_at(payload, layout.station) };
    // SAFETY: the event is a `CraftRecipeEvent` readable for its reflected
    // size (the caller's contract), and reflection places its `u32` `recipeId` at `recipe`.
    let recipe: u32 = unsafe { read_at(payload, layout.recipe) };
    // SAFETY: the event is a `CraftRecipeEvent` readable for its reflected
    // size (the caller's contract), and reflection places its `u16` `recipeAmount` at `amount`.
    let amount: u16 = unsafe { read_at(payload, layout.amount) };
    let call = CraftCheck {
        header: Header::of::<CraftCheck>(),
        crafting_operator_id: enshwick_core::Entity(player),
        crafting_station_id: enshwick_core::Entity(station),
        recipe_id: enshwick_core::RecipeId::new(recipe),
        recipe_amount: amount,
        reserved: 0,
    };
    if call.crafting_operator_id == enshwick_core::Entity(0) {
        return;
    }
    let disabled = crate::content::recipes::is_disabled(call.recipe_id.get());
    let allowed = !disabled
        && CRAFT_FILTERS
            // SAFETY: a filter's mod runs, so its DLL that holds the callback
            // stays loaded; it gets the user pointer its mod registered, and
            // `call` lives on this stack for the call.
            .refuser(|filter, user| unsafe { filter(&raw const call, user) != enshwick_core::abi::FILTER_REFUSE })
            .is_none();
    if !allowed {
        // SAFETY: the event is a `CraftRecipeEvent` writable for its reflected
        // size, which nothing else touches during the call (the caller's
        // contract), with its `u32` `craftingOperatorId` at `player`.
        unsafe { write_at(payload, layout.player, 0u32) };
        // SAFETY: the event is a `CraftRecipeEvent` writable for its reflected
        // size, which nothing else touches during the call (the caller's
        // contract), with its `u16` `recipeAmount` at `amount`.
        unsafe { write_at(payload, layout.amount, 0u16) };
        crate::log!(
            DEBUG,
            "craft of recipe {} by player {} refused{}",
            crate::content::recipes::recipe_words(call.recipe_id.0),
            call.crafting_operator_id.0,
            if disabled {
                ": the recipe is disabled"
            } else {
                ""
            }
        );
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `building.cursor_check`: after the game's own check, a
    /// placement a filter refuses gets the game's "no permission" flag.
    fn cursor(
        flags: *mut u32,
        cursor_context: *mut u8,
        placement: *mut u8,
        transform: *const i64,
        a: u8,
        b: u8,
        range: f32,
    ) -> ();
    next CURSOR_NEXT;
    fuse &BUILD_FUSE, "the cursor";
    |onward| {
        onward.call();
        // SAFETY: the game's arguments to `building.cursor_check`, after its
        // own check returned, on the thread that ran it.
        unsafe { refuse(flags, cursor_context, placement, transform) };
    }
}

/// Adds the "no permission" flag to a placement a filter refuses.
///
/// # Safety
///
/// - `flags`, `cursor`, `placement` and `transform` are the arguments the
///   game passed to `building.cursor_check`, each null or live.
/// - Called on the thread that ran the check, right after it returned, so
///   nothing else writes them during the call.
unsafe fn refuse(flags: *mut u32, cursor: *mut u8, placement: *mut u8, transform: *const i64) {
    let (Some(layout), true, Some(engine)) = (
        BUILD.get(),
        BUILD_FILTERS.has_sealed(),
        crate::engine::ecs::resolved(),
    ) else {
        return;
    };
    if flags.is_null() || cursor.is_null() || placement.is_null() || transform.is_null() {
        return;
    }
    // SAFETY: `flags` is non-null (the null checks return first) and the
    // check's live `u32` flags word (the caller's contract).
    let current: u32 = unsafe { read_at(flags.cast(), 0) };
    if current & layout.no_permission != 0 {
        return;
    }
    // SAFETY: `cursor` is non-null (the null checks return first) and the
    // check's live cursor context (the caller's contract), which starts with
    // its system context pointer.
    let system = unsafe { read_ptr_at::<u64>(cursor, 0) };
    // SAFETY: `system` is the cursor's system context, null or live, on the
    // thread running that system (the caller's contract).
    let player = unsafe { engine.entity(system) };
    // SAFETY: `placement` is non-null (the null checks return first) and the
    // check's live placement (the caller's contract); the table's
    // `building.cursor_item_info` offset holds the placed item's static data
    // pointer, or null.
    let info = unsafe { read_ptr_at::<u8>(placement, layout.item_info) }.cast_const();
    let item = if info.is_null() {
        0
    } else {
        // SAFETY: `info` is non-null (the `is_null` branch gives 0) and an
        // item's static data, which starts with its `u32` id and stays loaded.
        unsafe { read_at::<u32>(info, 0) }
    };
    let server_world = crate::engine::world::server_world();
    let world = if crate::engine::loader::loader()
        .is_some_and(|loader| loader.side == enshwick_core::abi::Side::SERVER)
        // SAFETY: `system` is the cursor's system context, null or live, on
        // the thread running that system (the caller's contract), all
        // `is_world` asks for.
        || unsafe { engine.is_world(system, server_world) }
    {
        CursorWorld::Server
    } else if server_world != 0 {
        CursorWorld::Client
    } else {
        CursorWorld::Unknown
    };
    let server = world == CursorWorld::Server;
    let character = enshwick_core::Entity(player);
    let mut reason = Str::new("");
    let call = BuildCheck {
        header: Header::of::<BuildCheck>(),
        character,
        item: enshwick_core::ItemId::new(item),
        // SAFETY: `transform` is non-null (the null checks return first) and
        // the check's live cursor transform (the caller's contract), which
        // starts with its 32.32 position, three `i64`.
        position: enshwick_core::ecs::metres(unsafe { read_at::<[i64; 3]>(transform.cast(), 0) }),
        reason: &raw mut reason,
        account: builder_account(world, character),
    };
    // SAFETY: `call.reason` points at `reason`, live on this stack for the
    // whole chain.
    if let Some(filter) = unsafe { refuser(&BUILD_FILTERS, &call) } {
        // SAFETY: only the refusing filter wrote `reason`, a text it keeps
        // until its next call on this thread, which comes after this read.
        if let Some(text) = unsafe { reason.as_str() } {
            crate::actions::build_reasons::refused(filter.owner, call.character, text, server);
        }
        // SAFETY: `flags` is non-null (the null checks return first) and the
        // check's live `u32` flags word, which nothing else writes during the
        // call (the caller's contract) and the building systems read after it.
        unsafe { write_at(flags.cast(), 0, current | layout.no_permission) };
    }
}

/// Which world's cursor check runs, as far as the loader can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorWorld {
    /// The server's world, which judges every player's placement.
    Server,
    /// A client's own world, while the server's world is known to be
    /// another.
    Client,
    /// Not the server's world, but with no server world known to tell the
    /// two apart, such as while a hosted game's server starts.
    Unknown,
}

/// The account of `character`, whose cursor the check of `world` runs on;
/// zero when unknown.
fn builder_account(
    world: CursorWorld,
    character: enshwick_core::Entity,
) -> enshwick_core::PlayerKey {
    match world {
        CursorWorld::Server => crate::engine::world::accounts::account_of(character),
        // The client world checks only its local player's cursor: its
        // `client_cursor` skips every row but the one its local player check
        // passes, the check `player_ui` makes (client 26430625).
        CursorWorld::Client => crate::base::carrier::own_account(),
        // Perhaps the server's world misread: never a guest judged as the host.
        CursorWorld::Unknown => None,
    }
    .unwrap_or_default()
}

/// The first of `filters` that refuses `call`, with its reason emptied
/// before each asks, so a reason an allowing filter wrote never counts as
/// the refusing one's.
///
/// # Safety
///
/// - `call.reason` is valid for writes of one `Str` throughout the call.
unsafe fn refuser<'a>(
    filters: &'a Filters<BuildFilterFn>,
    call: &BuildCheck,
) -> Option<&'a Registered<BuildFilterFn>> {
    filters
        .ask(|filter| {
            // SAFETY: the caller keeps `reason` writable for the call.
            unsafe { call.reason.write(Str::EMPTY) };
            // SAFETY: a filter's mod runs, so its DLL that holds the callback
            // stays loaded; it gets the user pointer its mod registered, and
            // `call` and the reason it points at live for the call.
            if filter.call(|filter, user| unsafe {
                filter(call, user) != enshwick_core::abi::FILTER_REFUSE
            }) {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        })
        .map(|(refuser, ())| refuser)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: CraftLayout = CraftLayout {
        hash: 0x1234,
        size: 16,
        player: 0,
        station: 4,
        recipe: 8,
        amount: 12,
    };

    fn request(player: u32, recipe: u32, amount: u16) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&player.to_le_bytes());
        bytes[8..12].copy_from_slice(&recipe.to_le_bytes());
        bytes[12..14].copy_from_slice(&amount.to_le_bytes());
        bytes
    }

    #[test]
    fn a_request_for_a_disabled_recipe_reaches_the_game_empty() {
        let _first = CRAFT.set(LAYOUT);
        let _sealed = CRAFT_FILTERS.seal();
        crate::content::recipes::disable_for_test(&[7]);
        let mut refused = request(5, 7, 3);
        let mut allowed = request(5, 8, 3);
        for bytes in [&mut refused, &mut allowed] {
            // SAFETY: `bytes` is a request of the layout's 16 bytes on this
            // stack, readable and writable, which only this thread touches.
            unsafe { screen(LAYOUT.hash, bytes.as_mut_ptr(), bytes.len()) };
        }
        assert_eq!(refused, request(0, 7, 0));
        assert_eq!(allowed, request(5, 8, 3));
    }

    #[test]
    fn the_craft_screen_reads_the_craft_request_once_found_under_the_craft_filters_switch() {
        let loader = crate::start::tests::bare_loader();
        let _first = CRAFT.set(LAYOUT);
        assert_eq!((FOLLOWER.types)(&loader), Some(vec![LAYOUT.hash]));
        assert!(FOLLOWER.screen.is_some() && FOLLOWER.follow.is_none());
        assert!(
            FOLLOWER
                .fuse
                .is_some_and(|fuse| core::ptr::eq(fuse, &raw const CRAFT_FUSE)),
            "the move lists the craft filters off with it"
        );
    }

    #[test]
    fn a_build_whose_event_move_does_not_link_lists_the_craft_filters_off_by_the_move() {
        use crate::engine::pinned::ecs;
        let layout = [
            ecs::EVENT_STREAMS,
            ecs::EVENT_STREAM_COUNT,
            ecs::EVENT_STREAM_SIZE,
            ecs::EVENT_STREAM_USED,
            ecs::EVENT_RECORD_SIZE,
            ecs::EVENT_RECORD_HEADER,
            ecs::EVENT_RECORD_ALIGN,
        ]
        .map(|constant| (constant, 8));
        let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::pinning_loader(
            enshwick_core::abi::Side::SERVER,
            &[],
            &layout,
        )));
        let _first = CRAFT.set(LAYOUT);
        crate::base::game_events::link(loader);
        let off = crate::base::off_reason(&CRAFT_FUSE);
        assert!(
            off.as_deref()
                .is_some_and(|r| r.starts_with(ecs::EVENT_MERGE.id())),
            "{off:?}"
        );
    }

    #[test]
    fn a_build_without_the_craft_request_lists_the_craft_filters_off() {
        seal(&crate::start::tests::bare_loader());
        assert!(CRAFT_FUSE.is_off());
        let off = crate::base::off_reason(&CRAFT_FUSE);
        assert!(
            off.as_deref().is_some_and(|r| r.starts_with(CRAFT_EVENT)),
            "{off:?}"
        );
    }

    #[test]
    fn a_hosting_client_s_own_prediction_gets_its_own_player_s_account() {
        let _carrier = crate::base::carrier::tests::alone();
        let local = enshwick_core::Entity(40);
        assert_eq!(
            builder_account(CursorWorld::Client, local),
            enshwick_core::PlayerKey::default(),
            "a client that hosts nothing"
        );
        crate::base::carrier::host_for_test(7);
        let hosted = builder_account(CursorWorld::Client, local);
        let unknown = builder_account(CursorWorld::Unknown, local);
        crate::base::carrier::unhost_for_test();
        assert_eq!(hosted, enshwick_core::PlayerKey(7));
        assert_eq!(
            unknown,
            enshwick_core::PlayerKey::default(),
            "no server world known to tell a guest's check from the host's"
        );
        assert_eq!(
            builder_account(CursorWorld::Client, local),
            enshwick_core::PlayerKey::default()
        );
    }

    #[test]
    fn a_reason_an_allowing_filter_wrote_never_counts_for_the_refusing_one() {
        unsafe extern "system" fn chatty(call: *const BuildCheck, _: *mut c_void) -> u8 {
            // SAFETY: the test passes a live `BuildCheck` on its stack.
            let reason = unsafe { (*call).reason };
            // SAFETY: the check's reason points at a `Str` on the test's stack.
            unsafe { reason.write(Str::new("chatty but allowing")) };
            enshwick_core::abi::FILTER_PASS
        }
        unsafe extern "system" fn silent(_: *const BuildCheck, _: *mut c_void) -> u8 {
            enshwick_core::abi::FILTER_REFUSE
        }
        let filters = Filters::<BuildFilterFn>::new(Feature::Vetoes);
        let (first, second) = (
            core::ptr::without_provenance_mut(1),
            core::ptr::without_provenance_mut(2),
        );
        let loader = ModIndex::LOADER;
        assert_eq!(filters.register(loader, chatty, first), Ok(()));
        assert_eq!(filters.register(loader, silent, second), Ok(()));
        let _sealed = filters.seal();
        let mut reason = Str::EMPTY;
        let call = BuildCheck {
            header: Header::of::<BuildCheck>(),
            character: enshwick_core::Entity(1),
            item: enshwick_core::ItemId::new(2),
            position: [0.0; 3],
            reason: &raw mut reason,
            account: enshwick_core::PlayerKey::default(),
        };
        // SAFETY: `call.reason` points at `reason` on this stack for the call.
        let refused_by = unsafe { refuser(&filters, &call) };
        assert_eq!(refused_by.map(Registered::user), Some(second));
        // SAFETY: `reason` holds `Str::EMPTY`, a static text.
        assert_eq!(unsafe { reason.as_str() }.unwrap_or_default(), "");
    }
}
