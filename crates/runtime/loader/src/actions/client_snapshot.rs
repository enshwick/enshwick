//! The client's state in one copy for any thread: the session, the open
//! screens and the in-game menu's tab, kept by the interface frame, which
//! alone may read the interface, and the local player's held controls from
//! [`local_held`].
//!
//! Until a mod asks, the frame keeps nothing; the first ask starts it.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use enshwick_core::abi::{ClientSnapshot, Str};
use enshwick_core::{Entity, RecipeId};

use crate::actions::client_control::MenuTab;
use crate::actions::local_held::{self, Sample};
use crate::actions::player_state::{self, Parts};
use crate::engine::copied::Copied;
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "the client snapshot";
/// The tab code of no tab.
const NO_TAB: u64 = 0xff;

/// What one interface frame read of the game the player is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameRead {
    /// The play data's address, which names the client's game object.
    pub(crate) play: usize,
    /// The HUD screen byte.
    pub(crate) screen: u8,
    /// The screen byte a stacked screen returns to, 0 for the world.
    pub(crate) return_to: u8,
    /// The open screen's host entity, 0 for none.
    pub(crate) host: u32,
    /// The in-game menu's shown tab.
    pub(crate) tab: TabRead,
    /// The crafting menu's highlighted recipe.
    pub(crate) recipe: RecipeRead,
    /// Whether the interaction prompt shows; `None` on a build without the
    /// prompt's reads.
    pub(crate) prompt: Option<bool>,
}

/// What a frame read of the main menu's selected character: `None` on a
/// build without the read, else its place in the menu's list, `None`
/// while none is selected and outside the main menu.
pub(crate) type CharacterRead = Option<Option<u16>>;

/// The bit of a read word a frame wrote, above the 32 bits of its value.
const WORD_READ: u64 = 1 << 32;

/// The character word of `read`: [`WORD_READ`] with the place plus 1, 0
/// for none; 0 unread.
fn pack_character(read: CharacterRead) -> u64 {
    read.map_or(0, |place| {
        WORD_READ | place.map_or(0, |at| u64::from(at) + 1)
    })
}

/// The read a frame packed into `word`.
fn unpack_character(word: u64) -> CharacterRead {
    (word & WORD_READ != 0).then(|| {
        let place = word & 0xffff_ffff;
        place.checked_sub(1).and_then(|at| u16::try_from(at).ok())
    })
}

/// What the loader's own Mods screen shows: its page, as
/// [`ClientSnapshot::loader_screen`] numbers it, and the mod the page names
/// by its load-order index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LoaderScreen {
    pub(crate) page: u32,
    pub(crate) mod_index: Option<usize>,
}

impl LoaderScreen {
    /// The bit of a word a frame wrote, which no page number reaches.
    const READ: u64 = 1 << 31;

    /// The screen in one word: the page with [`Self::READ`], then the mod's
    /// index plus 1.
    fn pack(self) -> u64 {
        let index = self
            .mod_index
            .map_or(0, |i| u64::try_from(i).map_or(0, |i| i + 1));
        u64::from(self.page) | Self::READ | index << 32
    }

    /// The screen a frame wrote into `word`; `None` before one did.
    fn unpack(word: u64) -> Option<Self> {
        if word & Self::READ == 0 {
            return None;
        }
        let index = word >> 32;
        Some(Self {
            page: u32::try_from(word & (Self::READ - 1)).unwrap_or(0),
            mod_index: (index != 0).then(|| usize::try_from(index - 1).unwrap_or(usize::MAX)),
        })
    }
}

/// What a frame read of the crafting menu's highlighted recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecipeRead {
    /// No recipe browser drew, or the build lacks the read.
    Unread,
    /// The browser drew, highlighting this recipe or none.
    Shown(Option<RecipeId>),
}

/// The recipe word of `read`: the id with the read bit, 0 unread.
fn pack_recipe(read: RecipeRead) -> u64 {
    match read {
        RecipeRead::Unread => 0,
        RecipeRead::Shown(recipe) => 1 << 32 | u64::from(recipe.map_or(0, |r| r.0)),
    }
}

/// What a frame read of the in-game menu's tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabRead {
    /// The build lacks the tab read, or it is off.
    Unread,
    /// No menu screen is open.
    NoMenu,
    /// The menu shows this tab.
    Shown(MenuTab),
    /// A menu screen is open on a tab the build adds; mods read no tab.
    Other,
}

/// What the frame keeps, written only on the interface thread.
struct State {
    asked: AtomicBool,
    /// The session, the screens' packed word, the highlighted recipe's
    /// word, the prompt's word and the frame's time as
    /// [`local_held::now_ms`] counts.
    copy: Copied<5>,
    /// The loader's own screen, packed: it reads in every frame, at the
    /// main menu too, apart from the game's reads.
    loader: AtomicU64,
    /// The main menu's selected character, packed: it reads in every frame
    /// as the loader's screen does.
    character: AtomicU64,
    /// The prompt's target and its verb above it, as its site last kept them
    /// on the game's thread, 0 for none.
    prompt: AtomicU64,
    /// The play data, the local player and the world count the session was
    /// counted on.
    play: AtomicU64,
    player: AtomicU32,
    worlds: AtomicU64,
    session: AtomicU64,
}

static STATE: State = State {
    asked: AtomicBool::new(false),
    copy: Copied::new(),
    loader: AtomicU64::new(0),
    character: AtomicU64::new(0),
    prompt: AtomicU64::new(0),
    play: AtomicU64::new(0),
    player: AtomicU32::new(0),
    worlds: AtomicU64::new(0),
    session: AtomicU64::new(0),
};
static FUSE: Fuse = Fuse::new(FEATURE);

/// The screens of `read` in one word: the screen, the return-to screen,
/// the tab code ([`NO_TAB`] for none), the tab-read bit and the prompt-read
/// bit, then the host.
fn pack(read: &FrameRead) -> u64 {
    let (tab, tab_read) = match read.tab {
        TabRead::NoMenu | TabRead::Other => (NO_TAB, 1),
        TabRead::Shown(tab) => (tab.index(), 1),
        TabRead::Unread => (NO_TAB, 0),
    };
    u64::from(read.screen)
        | u64::from(read.return_to) << 8
        | tab << 16
        | tab_read << 24
        | u64::from(read.prompt.is_some()) << 25
        | u64::from(read.host) << 32
}

/// The prompt's word for a frame that read `shows`: what its site last
/// kept while it shows, else 0, and the kept word cleared while it does
/// not, so a prompt that shows next never names the last one's target; a
/// new session clears it too, before this.
fn prompt_word(shows: Option<bool>, kept: &AtomicU64) -> u64 {
    match shows {
        Some(true) => kept.load(Ordering::Relaxed),
        Some(false) => {
            kept.store(0, Ordering::Relaxed);
            0
        }
        None => 0,
    }
}

/// Keeps the target and the verb of the prompt that shows, as its site
/// reads them on the game's thread, once a mod asked.
pub(crate) fn note_prompt(target: u32, verb: u32) {
    STATE
        .prompt
        .store(u64::from(target) | u64::from(verb) << 32, Ordering::Relaxed);
}

/// The session and the player it keeps for a frame on `play` with `player`
/// in world count `worlds`, given the last counted; a new session when any
/// of the three moved. A player 0 is one not read yet: the held reads find
/// it a step after an ask or a world's start, so neither its absence nor
/// its first read starts a session.
fn next_session(last: (u64, u32, u64, u64), now: (u64, u32, u64)) -> (u64, u32) {
    let (play, player, worlds, session) = last;
    let (now_play, now_player, now_worlds) = now;
    let same_player = player == now_player || player == 0 || now_player == 0;
    if session != 0 && play == now_play && worlds == now_worlds && same_player {
        (session, if now_player == 0 { player } else { now_player })
    } else {
        (session + 1, now_player)
    }
}

/// Keeps the frame's `read` of the game, the `loader`'s own screen and the
/// main menu's selected `character` once a mod asked; on the interface
/// thread, inside its frame. Each runs only then.
pub(crate) fn frame(
    read: impl FnOnce() -> Option<FrameRead>,
    loader: impl FnOnce() -> LoaderScreen,
    character: impl FnOnce() -> CharacterRead,
) {
    if !STATE.asked.load(Ordering::Relaxed) {
        return;
    }
    let _kept = FUSE.run(FEATURE, || {
        STATE.loader.store(loader().pack(), Ordering::Relaxed);
        STATE
            .character
            .store(pack_character(character()), Ordering::Relaxed);
        let Some(read) = read() else {
            return;
        };
        let now = (
            read.play as u64,
            local_held::player().map_or(0, Entity::get),
            crate::base::saves::worlds(),
        );
        let last = STATE.session.load(Ordering::Relaxed);
        let (session, player) = next_session(
            (
                STATE.play.load(Ordering::Relaxed),
                STATE.player.load(Ordering::Relaxed),
                STATE.worlds.load(Ordering::Relaxed),
                last,
            ),
            now,
        );
        if session != last {
            // A target the site kept in the last session names an entity of
            // its world, not this one's.
            STATE.prompt.store(0, Ordering::Relaxed);
        }
        STATE.play.store(now.0, Ordering::Relaxed);
        STATE.player.store(player, Ordering::Relaxed);
        STATE.worlds.store(now.2, Ordering::Relaxed);
        STATE.session.store(session, Ordering::Relaxed);
        STATE.copy.write([
            session,
            pack(&read),
            pack_recipe(read.recipe),
            prompt_word(read.prompt, &STATE.prompt),
            local_held::now_ms(),
        ]);
    });
}

/// Starts the frame's reads, which a client command checks itself against,
/// and a hosted world's map frame read, which a waypoint or ping does.
pub(crate) fn ask() {
    STATE.asked.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    crate::world::waypoints::want_frame();
}

/// Whether a mod asked for the frame's reads.
pub(crate) fn asked() -> bool {
    STATE.asked.load(Ordering::Relaxed)
}

/// The session the last frame counted, 0 before one; on the interface
/// thread, after [`frame`].
pub(crate) fn session() -> u64 {
    STATE.session.load(Ordering::Relaxed)
}

/// The client's state now on `loader`, whose mods the loader's screen
/// names; asking starts the reads.
pub(crate) fn snapshot(loader: &Loader, scene: Option<&'static str>) -> ClientSnapshot {
    ask();
    let off = FUSE.is_off() || crate::engine::screens::READS_FUSE.is_off();
    let snapshot = compose(
        Kept {
            words: STATE.copy.load(),
            loader: STATE.loader.load(Ordering::Relaxed),
            character: STATE.character.load(Ordering::Relaxed),
            off,
            held: local_held::sample(),
            player: local_held::player(),
            state: player_state::last(),
            scene,
        },
        local_held::now_ms(),
        crate::engine::screens::name,
        // The records live as long as the loader, the process.
        |index| loader.mods.get(index).map(|m| Str::new(&m.id)),
    );
    let mut snapshot = enshwick_abi::headed!(ClientSnapshot {
        server: crate::actions::client_commands::server_str(),
        ..snapshot
    });
    if let Some(shown) = crate::actions::client_commands::dialogs::shown() {
        snapshot.available |= ClientSnapshot::DIALOG;
        snapshot.dialog = shown.number;
        snapshot.dialog_buttons = shown.buttons;
        snapshot.dialog_mod = shown
            .owner
            .and_then(|index| loader.mods.get(index))
            .map_or(Str::EMPTY, |m| Str::new(&m.id));
    }
    snapshot
}

/// What a snapshot is made of: the frame's last whole copy, the loader's
/// screen word, the selected character's word, whether the reads are off,
/// the held controls' sample, the local player, its own state with when it
/// was read, and the world's name.
#[derive(Clone, Copy, Debug)]
struct Kept {
    words: Option<[u64; 5]>,
    loader: u64,
    character: u64,
    off: bool,
    held: Sample,
    player: Option<Entity>,
    state: Option<(player_state::Read, u64)>,
    scene: Option<&'static str>,
}

/// The snapshot of `kept` at `now`, naming screen bytes with `name` and
/// mods by their load-order index with `mod_id`.
fn compose(
    kept: Kept,
    now: u64,
    name: impl Fn(u8) -> &'static str,
    mod_id: impl Fn(usize) -> Option<Str>,
) -> ClientSnapshot {
    let mut snapshot = enshwick_abi::headed!(ClientSnapshot {
        scene: kept.scene.map_or(Str::EMPTY, Str::new),
        ..ClientSnapshot::EMPTY
    });
    if let Sample::Fresh { bits, .. } = kept.held {
        snapshot.held = bits;
        snapshot.available |= ClientSnapshot::HELD;
        if let Some(player) = kept.player {
            snapshot.player = player;
            snapshot.available |= ClientSnapshot::PLAYER;
        }
    }
    if let Some((state, at)) = kept.state
        && local_held::fresh(at, now)
    {
        player_parts(&mut snapshot, &state);
    }
    // The loader's own screen reads at the main menu too, before any game.
    if !kept.off
        && let Some(screen) = LoaderScreen::unpack(kept.loader)
    {
        snapshot.available |= ClientSnapshot::LOADER_SCREEN;
        snapshot.loader_screen = screen.page;
        snapshot.loader_screen_mod = screen.mod_index.and_then(mod_id).unwrap_or(Str::EMPTY);
    }
    // The main menu's selected character reads, as the loader's screen does.
    if !kept.off
        && let Some(place) = unpack_character(kept.character)
    {
        snapshot.available |= ClientSnapshot::CHARACTER;
        snapshot.character = place.map_or(ClientSnapshot::NO_CHARACTER, u32::from);
    }
    // A copy mid-write through every try keeps the screens out of this one.
    let [session, screens, recipe, prompt, read] = kept.words.unwrap_or_default();
    snapshot.state = if kept.off {
        ClientSnapshot::OFF
    } else if read == 0 {
        ClientSnapshot::NEVER
    } else if local_held::fresh(read, now) {
        ClientSnapshot::FRESH
    } else {
        ClientSnapshot::STALE
    };
    if kept.off || read == 0 {
        return snapshot;
    }
    snapshot.session = session;
    snapshot.age_ms = now.saturating_sub(read);
    let byte = |shift: u32| u8::try_from(screens >> shift & 0xff).unwrap_or(0);
    let named = |screen: u8| Str::new(name(screen));
    snapshot.hud_screen = named(byte(0));
    if byte(8) != 0 {
        snapshot.return_screen = named(byte(8));
    }
    snapshot.host = Entity::from_engine(u32::try_from(screens >> 32).unwrap_or(0));
    snapshot.available |= ClientSnapshot::SCREENS;
    if screens >> 24 & 1 == 1 {
        snapshot.available |= ClientSnapshot::MENU_TAB;
        if let Some(tab) = MenuTab::ALL
            .into_iter()
            .find(|tab| tab.index() == screens >> 16 & 0xff)
        {
            snapshot.menu_tab = Str::new(tab.name());
        }
    }
    if recipe >> 32 & 1 == 1 {
        snapshot.available |= ClientSnapshot::RECIPE;
        snapshot.highlighted_recipe = RecipeId(u32::try_from(recipe & 0xffff_ffff).unwrap_or(0));
    }
    if screens >> 25 & 1 == 1 {
        snapshot.available |= ClientSnapshot::PROMPT;
        let target = u32::try_from(prompt & 0xffff_ffff).unwrap_or(0);
        if target != 0 {
            snapshot.prompt_target = Entity::from_engine(target);
            snapshot.prompt_verb = u32::try_from(prompt >> 32).unwrap_or(0);
        }
    }
    snapshot
}

/// Sets the groups of `snapshot` that `state`, a fresh read of the local
/// player, holds.
fn player_parts(snapshot: &mut ClientSnapshot, state: &player_state::Read) {
    let parts = state.parts;
    if parts.contains(Parts::POSITION) {
        snapshot.position = enshwick_core::ecs::metres(state.position);
        snapshot.available |= ClientSnapshot::POSITION;
    }
    if parts.contains(Parts::LOOK) {
        snapshot.look = state.look;
        snapshot.available |= ClientSnapshot::LOOK;
    }
    if parts.contains(Parts::VITALS) {
        [
            snapshot.health,
            snapshot.health_max,
            snapshot.stamina,
            snapshot.stamina_max,
        ] = state.vitals.map(u32::from);
        snapshot.available |= ClientSnapshot::VITALS;
    }
    if parts.contains(Parts::ACTIONBAR) {
        snapshot.actionbar_slot = u32::from(state.actionbar);
        snapshot.available |= ClientSnapshot::ACTIONBAR;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_local_player_s_state_counts_only_while_fresh_and_only_its_read_parts() {
        let read = player_state::Read {
            parts: Parts::POSITION.with(Parts::VITALS),
            position: [3 << 32, -(1 << 31), 0],
            look: [0.0, 0.0, 0.0, 1.0],
            vitals: [37, 120, 50, 100],
            actionbar: 2,
        };
        let fresh = compose(
            Kept {
                state: Some((read, 100)),
                ..kept(None)
            },
            150,
            name,
        );
        assert_eq!(
            fresh.available,
            ClientSnapshot::POSITION | ClientSnapshot::VITALS
        );
        assert_eq!(fresh.position, [3.0, -0.5, 0.0]);
        assert_eq!(
            (
                fresh.health,
                fresh.health_max,
                fresh.stamina,
                fresh.stamina_max
            ),
            (37, 120, 50, 100)
        );
        assert_eq!(fresh.actionbar_slot, 0, "an unread part stays empty");
        let stale = compose(
            Kept {
                state: Some((read, 100)),
                ..kept(None)
            },
            100 + local_held::FRESH_MS + 1,
            name,
        );
        assert_eq!(stale.available, 0, "no step read it in the last second");
    }

    /// The text of a snapshot's name.
    fn text(name: Str) -> Option<&'static str> {
        // SAFETY: every name a snapshot holds is static text.
        unsafe { name.as_str() }
    }

    /// The names of client 26430625's screens the tests use.
    fn name(screen: u8) -> &'static str {
        match screen {
            1 => "InGame",
            4 => "InGameMenu",
            8 => "PlayerCrafting",
            9 => "Backpack",
            0x15 => "StorageChest",
            _ => "",
        }
    }

    /// The snapshot of `kept` at `now`, with no mod to name.
    fn compose(kept: Kept, now: u64, name: impl Fn(u8) -> &'static str) -> ClientSnapshot {
        super::compose(kept, now, name, |_| None)
    }

    /// What a snapshot keeps of a frame's session, screens, recipe and time,
    /// with no prompt.
    fn kept(words: Option<[u64; 4]>) -> Kept {
        Kept {
            words: words
                .map(|[session, screens, recipe, read]| [session, screens, recipe, 0, read]),
            loader: 0,
            character: 0,
            off: false,
            held: Sample::Never { miss: None },
            player: None,
            state: None,
            scene: None,
        }
    }

    fn read(screen: u8, tab: TabRead) -> FrameRead {
        FrameRead {
            play: 0x1000,
            screen,
            return_to: 0,
            host: 0,
            tab,
            recipe: RecipeRead::Unread,
            prompt: None,
        }
    }

    #[test]
    fn a_shown_prompt_names_its_target_and_verb_and_a_hidden_one_clears_it() {
        let kept_word = AtomicU64::new(0);
        kept_word.store(1052 + (0x4e82_e309 << 32), Ordering::Relaxed);
        let shows = FrameRead {
            prompt: Some(true),
            ..read(1, TabRead::NoMenu)
        };
        let words =
            |read: &FrameRead| Some([1, pack(read), 0, prompt_word(read.prompt, &kept_word), 100]);
        let facing = compose(
            Kept {
                words: words(&shows),
                ..kept(None)
            },
            150,
            name,
        );
        assert_ne!(facing.available & ClientSnapshot::PROMPT, 0);
        assert_eq!(
            (facing.prompt_target, facing.prompt_verb),
            (Entity::from_engine(1052), 0x4e82_e309)
        );
        let hidden = FrameRead {
            prompt: Some(false),
            ..shows
        };
        let away = compose(
            Kept {
                words: words(&hidden),
                ..kept(None)
            },
            150,
            name,
        );
        assert_ne!(
            away.available & ClientSnapshot::PROMPT,
            0,
            "read, none shows"
        );
        assert_eq!(
            (away.prompt_target, away.prompt_verb),
            (Entity::from_engine(0), 0)
        );
        assert_eq!(
            kept_word.load(Ordering::Relaxed),
            0,
            "the last target is gone"
        );
        let again = compose(
            Kept {
                words: words(&shows),
                ..kept(None)
            },
            150,
            name,
        );
        assert_eq!(
            again.prompt_target,
            Entity::from_engine(0),
            "no stale target before the site runs"
        );
        let unread = compose(
            kept(Some([1, pack(&read(1, TabRead::NoMenu)), 0, 100])),
            150,
            name,
        );
        assert_eq!(unread.available & ClientSnapshot::PROMPT, 0);
    }

    #[test]
    fn the_selected_character_reads_by_its_place_and_none_outside_the_menu() {
        let at = |read: CharacterRead| {
            compose(
                Kept {
                    character: pack_character(read),
                    ..kept(None)
                },
                150,
                name,
            )
        };
        let picked = at(Some(Some(2)));
        assert_ne!(picked.available & ClientSnapshot::CHARACTER, 0);
        assert_eq!(picked.character, 2);
        let first = at(Some(Some(0)));
        assert_eq!(first.character, 0, "the first place is no none");
        let none = at(Some(None));
        assert_ne!(none.available & ClientSnapshot::CHARACTER, 0);
        assert_eq!(none.character, ClientSnapshot::NO_CHARACTER);
        let unread = at(None);
        assert_eq!(unread.available & ClientSnapshot::CHARACTER, 0);
        assert_eq!(
            unpack_character(pack_character(Some(Some(u16::MAX)))),
            Some(Some(u16::MAX))
        );
    }

    #[test]
    fn the_loader_s_screen_names_its_page_and_its_mod_by_id() {
        let page = LoaderScreen {
            page: ClientSnapshot::MOD_PAGE,
            mod_index: Some(2),
        };
        let kept = Kept {
            loader: page.pack(),
            ..kept(Some([1, 1, 0, 100]))
        };
        let snapshot = super::compose(kept, 150, name, |index| {
            (index == 2).then(|| Str::new("you.trail"))
        });
        assert_ne!(snapshot.available & ClientSnapshot::LOADER_SCREEN, 0);
        assert_eq!(snapshot.loader_screen, ClientSnapshot::MOD_PAGE);
        assert_eq!(text(snapshot.loader_screen_mod), Some("you.trail"));
        let none = LoaderScreen::unpack(LoaderScreen::default().pack());
        assert_eq!(none, Some(LoaderScreen::default()), "no page, no mod");
        assert_eq!(LoaderScreen::unpack(0), None, "no frame wrote it");
    }

    #[test]
    fn the_loader_s_screen_reads_at_the_main_menu_before_any_game() {
        let list = LoaderScreen {
            page: ClientSnapshot::MOD_LIST,
            mod_index: None,
        };
        let menu = Kept {
            loader: list.pack(),
            ..kept(None)
        };
        let snapshot = compose(menu, 150, name);
        assert_eq!(snapshot.state, ClientSnapshot::NEVER, "no game drew");
        assert_eq!(snapshot.available, ClientSnapshot::LOADER_SCREEN);
        assert_eq!(snapshot.loader_screen, ClientSnapshot::MOD_LIST);
        let off = Kept { off: true, ..menu };
        assert_eq!(compose(off, 150, name).available, 0, "reads off");
        let unread = compose(kept(Some([1, 1, 0, 100])), 150, name);
        assert_eq!(
            unread.available & ClientSnapshot::LOADER_SCREEN,
            0,
            "no frame read the loader's screen"
        );
    }

    #[test]
    fn the_highlighted_recipe_counts_only_while_a_browser_drew() {
        let torch = RecipeId(0x7cd8_0a04);
        let words = |recipe| [1, pack(&read(8, TabRead::NoMenu)), pack_recipe(recipe), 100];
        let shown = compose(kept(Some(words(RecipeRead::Shown(Some(torch))))), 150, name);
        assert_ne!(shown.available & ClientSnapshot::RECIPE, 0);
        assert_eq!(shown.highlighted_recipe, torch);
        let none = compose(kept(Some(words(RecipeRead::Shown(None)))), 150, name);
        assert_ne!(
            none.available & ClientSnapshot::RECIPE,
            0,
            "a menu with none"
        );
        assert_eq!(none.highlighted_recipe, RecipeId(0));
        let closed = compose(kept(Some(words(RecipeRead::Unread))), 150, name);
        assert_eq!(closed.available & ClientSnapshot::RECIPE, 0);
    }

    #[test]
    fn a_snapshot_before_any_frame_is_never_read_and_names_no_screen() {
        let snapshot = compose(kept(None), 50, name);
        assert_eq!(snapshot.state, ClientSnapshot::NEVER);
        assert_eq!(snapshot.available, 0);
        assert_eq!(snapshot.age_ms, u64::MAX);
        assert_eq!(snapshot.session, 0);
        let zero = compose(kept(Some([0; 4])), 50, name);
        assert_eq!(zero.state, ClientSnapshot::NEVER, "a copy never written");
    }

    #[test]
    fn the_menu_tab_shows_apart_from_the_screen_byte() {
        let words = [3, pack(&read(4, TabRead::Shown(MenuTab::Backpack))), 0, 100];
        let snapshot = compose(kept(Some(words)), 150, name);
        assert_eq!(snapshot.state, ClientSnapshot::FRESH);
        assert_eq!(snapshot.session, 3);
        assert_eq!(snapshot.age_ms, 50);
        assert_eq!(text(snapshot.hud_screen), Some("InGameMenu"));
        assert_eq!(text(snapshot.menu_tab), Some("Backpack"));
        assert_eq!(
            snapshot.available,
            ClientSnapshot::SCREENS | ClientSnapshot::MENU_TAB
        );
        let crafting = [3, pack(&read(8, TabRead::Shown(MenuTab::Backpack))), 0, 100];
        let snapshot = compose(kept(Some(crafting)), 150, name);
        assert_eq!(
            (text(snapshot.hud_screen), text(snapshot.menu_tab)),
            (Some("PlayerCrafting"), Some("Backpack")),
            "V, then E"
        );
    }

    #[test]
    fn no_menu_names_no_tab_and_a_build_without_the_tab_read_leaves_its_bit_clear() {
        let world = compose(
            kept(Some([1, pack(&read(1, TabRead::NoMenu)), 0, 100])),
            150,
            name,
        );
        assert_eq!(text(world.menu_tab), Some(""));
        assert_ne!(world.available & ClientSnapshot::MENU_TAB, 0);
        let unread = compose(
            kept(Some([1, pack(&read(4, TabRead::Unread)), 0, 100])),
            150,
            name,
        );
        assert_eq!(unread.available & ClientSnapshot::MENU_TAB, 0);
        assert_eq!(text(unread.hud_screen), Some("InGameMenu"));
    }

    #[test]
    fn a_stacked_screen_names_where_it_returns_and_its_host() {
        let read = FrameRead {
            return_to: 9,
            host: 77,
            ..read(0x15, TabRead::NoMenu)
        };
        let snapshot = compose(kept(Some([1, pack(&read), 0, 100])), 150, name);
        assert_eq!(text(snapshot.hud_screen), Some("StorageChest"));
        assert_eq!(text(snapshot.return_screen), Some("Backpack"));
        assert_eq!(snapshot.host, Entity::from_engine(77));
        let world = compose(kept(Some([1, pack(&read_world()), 0, 100])), 150, name);
        assert_eq!(text(world.return_screen), Some(""), "back to the world");
    }

    fn read_world() -> FrameRead {
        read(1, TabRead::NoMenu)
    }

    #[test]
    fn a_paused_game_reports_its_age_as_stale_not_as_absent() {
        let snapshot = compose(kept(Some([2, pack(&read_world()), 0, 100])), 1200, name);
        assert_eq!(snapshot.state, ClientSnapshot::STALE);
        assert_eq!(snapshot.age_ms, 1100);
        assert_eq!(snapshot.session, 2);
        assert_eq!(text(snapshot.hud_screen), Some("InGame"));
    }

    #[test]
    fn reads_turned_off_answer_off_with_no_screens() {
        let snapshot = compose(
            Kept {
                off: true,
                ..kept(Some([2, pack(&read_world()), 0, 100]))
            },
            150,
            name,
        );
        assert_eq!(snapshot.state, ClientSnapshot::OFF);
        assert_eq!(snapshot.available & ClientSnapshot::SCREENS, 0);
    }

    #[test]
    fn the_held_controls_and_the_player_count_only_from_a_fresh_sample() {
        let fresh = Kept {
            held: Sample::Fresh {
                bits: 1 << 30,
                age_ms: 5,
            },
            player: Some(Entity::from_engine(9)),
            scene: Some("world1"),
            ..kept(None)
        };
        let snapshot = compose(fresh, 150, name);
        assert_eq!(snapshot.held, 1 << 30);
        assert_eq!(snapshot.player, Entity::from_engine(9));
        assert_eq!(text(snapshot.scene), Some("world1"));
        assert_eq!(
            snapshot.available,
            ClientSnapshot::HELD | ClientSnapshot::PLAYER
        );
        let stale = Kept {
            held: Sample::Stale {
                bits: 1,
                age_ms: 2000,
                miss: None,
            },
            ..fresh
        };
        assert_eq!(compose(stale, 150, name).available, 0);
    }

    #[test]
    fn a_session_lasts_while_the_game_object_the_player_and_the_world_stay() {
        let next = |last, now| next_session(last, now).0;
        assert_eq!(next((0, 0, 0, 0), (0x1000, 7, 1)), 1, "the first");
        assert_eq!(next((0x1000, 7, 1, 1), (0x1000, 7, 1)), 1);
        assert_eq!(next((0x1000, 7, 1, 1), (0x2000, 7, 1)), 2, "object");
        assert_eq!(next((0x1000, 7, 1, 1), (0x1000, 8, 1)), 2, "player");
        assert_eq!(next((0x1000, 7, 1, 1), (0x1000, 7, 2)), 2, "world");
    }

    #[test]
    fn a_player_read_a_step_after_the_ask_or_lost_meanwhile_keeps_the_session() {
        assert_eq!(
            next_session((0x1000, 0, 1, 1), (0x1000, 7, 1)),
            (1, 7),
            "first read"
        );
        assert_eq!(
            next_session((0x1000, 7, 1, 1), (0x1000, 0, 1)),
            (1, 7),
            "not read this frame"
        );
        assert_eq!(
            next_session((0x1000, 7, 1, 1), (0x1000, 0, 2)),
            (2, 0),
            "a new world forgets the player"
        );
    }

    #[test]
    fn the_frame_keeps_nothing_until_a_mod_asks_and_then_counts_a_session() {
        let ran = core::cell::Cell::new(false);
        frame(
            || {
                ran.set(true);
                None
            },
            || {
                ran.set(true);
                LoaderScreen::default()
            },
            || {
                ran.set(true);
                None
            },
        );
        assert!(!ran.get(), "no read before an ask");
        // The frame's own copy, composed as `snapshot` does, without asking
        // for the held reads, which other tests in the process read as
        // never asked.
        let kept = || {
            let words = STATE.copy.load();
            let loader = STATE.loader.load(Ordering::Relaxed);
            let base = kept(None);
            compose(
                Kept {
                    words,
                    loader,
                    ..base
                },
                local_held::now_ms(),
                name,
            )
        };
        ask();
        assert_eq!(kept().state, ClientSnapshot::NEVER);
        let list = || LoaderScreen {
            page: ClientSnapshot::MOD_LIST,
            mod_index: None,
        };
        frame(|| None, list, || None);
        let menu = kept();
        assert_eq!(
            (menu.state, menu.loader_screen),
            (ClientSnapshot::NEVER, ClientSnapshot::MOD_LIST),
            "the main menu's Mods screen, before any game"
        );
        frame(
            || Some(read(4, TabRead::Shown(MenuTab::Map))),
            list,
            || None,
        );
        let second = kept();
        assert_eq!(second.state, ClientSnapshot::FRESH);
        assert_eq!(second.session, 1);
        frame(
            || Some(read(4, TabRead::Shown(MenuTab::Map))),
            list,
            || None,
        );
        assert_eq!(kept().session, 1, "the same session");
        let facing = |play| FrameRead {
            play,
            prompt: Some(true),
            ..read(4, TabRead::NoMenu)
        };
        note_prompt(1052, 0x4e82_e309);
        frame(|| Some(facing(0x1000)), list, || None);
        assert_eq!(
            kept().prompt_target,
            Entity::from_engine(1052),
            "kept in its session"
        );
        frame(|| Some(facing(0x2000)), list, || None);
        let next = kept();
        assert_eq!(next.session, 2, "another game object");
        assert_eq!(
            next.prompt_target,
            Entity::from_engine(0),
            "the last session's target is gone before the site runs again"
        );
    }
}
