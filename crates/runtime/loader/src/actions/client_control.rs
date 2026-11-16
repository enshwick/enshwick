//! Where a client's own controls are found on the running build: the input
//! builder and the stick and look it writes, the HUD's own screen toggle,
//! the in-game menu's tab and the crafting menu's recipe browser, each by
//! an engine route, so every kept client finds them.
//!
//! Each group resolves once at link and turns off alone with a log line
//! when the build lacks one of its entries, so a build without the recipe
//! browser still reads the menu's tab.

use std::sync::OnceLock;

use enshwick_core::RecipeId;
use enshwick_core::abi::Side;
use enshwick_gamedata::catalogue;

use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::input::BLOCKED.id(),
    pinned::input::CAMERA_ROTATION.id(),
    pinned::input::CLIENT_BUILD.id(),
    pinned::input::MOVEMENT_STICK.id(),
    pinned::input::TARGET_SELECTION.id(),
    pinned::input::DIGITAL_INPUT.id(),
    pinned::ui::MENU_TAB.id(),
    pinned::ui::RECIPE_DIRECTORY.id(),
    pinned::ui::RECIPE_HIGHLIGHTED.id(),
    pinned::ui::RECIPE_SELECT.id(),
    pinned::ui::TOGGLE_HUD_SCREEN.id(),
];

/// The input builder's passes, by its mode argument (client 26430625,
/// seen live).
pub(crate) mod pass {
    /// The pass that writes the stick, once per game step.
    pub(crate) const STEP: u32 = 0;
    /// The pass that writes only the look, once per frame.
    pub(crate) const FRAME: u32 = 1;
}

/// Why a group's entry fails: the table lacks it.
const ABSENT: &str = "this build lacks it";

/// `MultiDeviceDirectionalInput`: digital, analog, mouse, touch pad and
/// gyro, a float pair each (client 26430625); `resolve` checks the
/// reflected size against it.
const LOOK_DEVICES: usize = 5;

/// The screens of the in-game menu, whose tab index names the tab it shows
/// while one is open (`0x140d65cb0`, client 26430625): the index stays set
/// after the menu closes.
const MENU_SCREENS: [&str; 8] = [
    "InGameMenu",
    "Map",
    "Journal",
    "Character",
    "PlayerCrafting",
    "Backpack",
    "SkillTree",
    "Social",
];

/// One group of the client's controls: its layout once linked, and the
/// fuse that turns it alone off.
#[derive(Debug)]
pub(crate) struct Group<T> {
    layout: OnceLock<T>,
    fuse: Fuse,
}

impl<T> Group<T> {
    const fn new(feature: &'static str) -> Self {
        Self {
            layout: OnceLock::new(),
            fuse: Fuse::new(feature),
        }
    }

    /// Keeps `found`, or turns the group off for the entry it lacks.
    fn link(&'static self, loader: &Loader, found: Result<T, &'static str>) {
        match found {
            Ok(layout) => {
                let _first = self.layout.set(layout);
            }
            Err(id) => {
                let _off = loader.fuse_off(&self.fuse, id, ABSENT);
            }
        }
    }

    /// The layout, once linked on a client whose build has every entry of
    /// the group and while the group is on.
    pub(crate) fn get(&self) -> Option<&T> {
        self.layout.get().filter(|_| !self.fuse.is_off())
    }
}

/// The client's control groups, one state per domain.
#[derive(Debug)]
pub(crate) struct Groups {
    /// The in-game menu's tab.
    pub(crate) tab: Group<Tab>,
    /// The HUD's own screen toggle.
    pub(crate) toggle: Group<Toggle>,
    /// The input builder's stick and look.
    pub(crate) input: Group<InputLayout>,
    /// The crafting menu's recipe browser.
    pub(crate) recipes: Group<Recipes>,
}

/// The groups of this process.
pub(crate) static GROUPS: Groups = Groups {
    tab: Group::new("the in-game menu's tab"),
    toggle: Group::new("the HUD's screen toggle"),
    input: Group::new("the client's look and stick input"),
    recipes: Group::new("the crafting menu's recipe browser"),
};

/// Where the in-game menu keeps its tab, and which screen bytes are the
/// menu's.
#[derive(Debug)]
pub(crate) struct Tab {
    /// The tab index in the interface state.
    menu_tab: usize,
    /// A bit per screen byte of [`MENU_SCREENS`].
    menu_screens: [u64; 4],
}

/// The HUD's screen toggle.
#[derive(Debug)]
pub(crate) struct Toggle {
    /// `toggleHudScreen(ui context, HudScreenType)`.
    pub(crate) toggle_hud_screen: usize,
}

/// What the input builder writes, at their offsets in `ClientPlayerInput`,
/// and the mapping byte it checks first; the group needs the builder too,
/// which `client_input` hooks.
#[derive(Debug)]
pub(crate) struct InputLayout {
    /// `ClientPlayerInput.data.movementStick`, a float pair.
    movement_stick: usize,
    /// `ClientPlayerInput.data.cameraRotationInput`.
    camera_rotation: usize,
    /// `ClientPlayerInput.data.targetSelectionInput`, a float pair.
    target_selection: usize,
    /// `ClientPlayerInput.data.digitalInput`, the 64-bit mask of the
    /// buttons held: interact, attack, jump, dodge and the rest.
    digital_input: usize,
    /// The input mapping's byte that is 1 while the game blocks the
    /// player's input, which the builder checks first.
    blocked: usize,
}

/// The crafting menu's recipe browser, whose block `client_recipes` takes
/// at `doRecipeDirectoryColumn(node, book, browser, ctx)`, which the group
/// needs too.
#[derive(Debug)]
pub(crate) struct Recipes {
    /// The browser's pointer to the highlighted recipe's record.
    highlighted: usize,
    /// The browser's recipe to bring into focus, 0 for none.
    select: usize,
}

/// The table entry `id` as an offset, or its id when the build lacks it.
fn offset(loader: &Loader, id: pinned::Constant) -> Result<usize, &'static str> {
    loader.offset(id).ok_or(id.id())
}

/// The table entry `id` as an address, or its id when the build lacks it.
fn address(loader: &Loader, id: pinned::Symbol) -> Result<u64, &'static str> {
    loader.address(id).ok_or(id.id())
}

/// The reflected size of `id`'s type, or the id when the build lacks it.
fn reflected_size(loader: &Loader, id: &'static str) -> Result<usize, &'static str> {
    let size = loader
        .reflection
        .type_info(catalogue::type_name(id))
        .map_err(|_| id)?
        .size();
    usize::try_from(size).map_err(|_| id)
}

impl Tab {
    fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
        let value = |name: &str| {
            loader
                .reflection
                .enum_value(screens, name)
                .ok()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or(catalogue::ids::HUD_SCREEN_TYPE)
        };
        // Every tab names a menu screen, so this finds each tab's too.
        let mut menu_screens = [0; 4];
        for name in MENU_SCREENS {
            let screen = value(name)?;
            menu_screens[usize::from(screen / 64)] |= 1 << (screen % 64);
        }
        Ok(Self {
            menu_tab: offset(loader, pinned::ui::MENU_TAB)?,
            menu_screens,
        })
    }

    /// Sets the tab the in-game menu draws by; the menu shows it from its
    /// next frame (index read every frame after its first, client
    /// 26430625).
    ///
    /// # Safety
    ///
    /// - `state` is the interface state the in-game menu draws from, valid
    ///   for a write of a `u64` at the routed `ui.menu_tab`, written in the
    ///   interface frame on its thread.
    pub(crate) unsafe fn choose(&self, state: *mut u8, tab: MenuTab) {
        // SAFETY: the caller's contract.
        unsafe { enshwick_core::raw::write_at(state, self.menu_tab, tab.index()) };
    }

    /// Whether screen byte `screen` is one of the in-game menu's.
    fn is_menu(&self, screen: u8) -> bool {
        self.menu_screens[usize::from(screen / 64)] & 1 << (screen % 64) != 0
    }

    /// The tab the in-game menu shows on screen byte `screen`; `None` while
    /// no menu screen is open, whatever index the state still holds.
    ///
    /// # Safety
    ///
    /// - `state` is the interface state the in-game menu draws from, valid
    ///   for reads of a `u64` at the routed `ui.menu_tab`, read in the
    ///   interface frame on its thread.
    pub(crate) unsafe fn shown(&self, state: *const u8, screen: u8) -> Option<MenuTab> {
        if !self.is_menu(screen) {
            return None;
        }
        // SAFETY: the caller's contract.
        MenuTab::from_index(unsafe { enshwick_core::raw::read_at(state, self.menu_tab) })
    }
}

impl InputLayout {
    fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let input = reflected_size(loader, catalogue::ids::CLIENT_PLAYER_INPUT)?;
        let look = reflected_size(loader, catalogue::ids::MULTI_DEVICE_DIRECTIONAL_INPUT)?;
        if look != size_of::<Look>() {
            return Err(catalogue::ids::MULTI_DEVICE_DIRECTIONAL_INPUT);
        }
        let inside = |id: pinned::Constant, size: usize| {
            offset(loader, id).and_then(|at| fits(at, size, input).then_some(at).ok_or(id.id()))
        };
        address(loader, pinned::input::CLIENT_BUILD)?;
        Ok(Self {
            movement_stick: inside(pinned::input::MOVEMENT_STICK, size_of::<[f32; 2]>())?,
            camera_rotation: inside(pinned::input::CAMERA_ROTATION, size_of::<Look>())?,
            target_selection: inside(pinned::input::TARGET_SELECTION, size_of::<[f32; 2]>())?,
            digital_input: inside(pinned::input::DIGITAL_INPUT, size_of::<u64>())?,
            blocked: offset(loader, pinned::input::BLOCKED)?,
        })
    }

    /// Holds the player's input the builder just wrote in pass `mode`, as
    /// the builder leaves it while the game blocks the input: no buttons, no
    /// target selection, and the stick in the step pass or the look in the
    /// frame pass. Each of those two is cleared only in the pass that writes
    /// it, so a mod's move or look command written after this pass stays.
    ///
    /// # Safety
    ///
    /// - `input` is valid for writes of the build's `ClientPlayerInput`,
    ///   which nothing else touches during the call.
    pub(crate) unsafe fn hold(&self, input: *mut u8, mode: u32) {
        // SAFETY: the caller's record, which `resolve` checked holds a
        // 64-bit mask at `digital_input` and a float pair at
        // `target_selection`.
        unsafe {
            enshwick_core::raw::write_at(input, self.digital_input, 0u64);
            enshwick_core::raw::write_at(input, self.target_selection, [0f32; 2]);
        }
        match mode {
            // SAFETY: the caller's record, by this function's `# Safety`.
            pass::STEP => unsafe { self.set_movement_stick(input, [0.0; 2]) },
            // SAFETY: the caller's record, by this function's `# Safety`.
            pass::FRAME => unsafe { self.set_camera_rotation(input, Look::default()) },
            _ => {}
        }
    }

    /// Whether the game blocks the player's input, as the builder's own
    /// first check reads it (1 is blocked, client 26430625).
    ///
    /// # Safety
    ///
    /// - `mapping` is the input mapping runtime the builder takes, valid
    ///   for a read of a byte at the routed `input.blocked`.
    pub(crate) unsafe fn blocked(&self, mapping: *const u8) -> bool {
        // SAFETY: the caller's contract.
        unsafe { enshwick_core::raw::read_at::<u8>(mapping, self.blocked) == 1 }
    }

    /// Writes the local player's movement stick.
    ///
    /// # Safety
    ///
    /// - `input` is valid for writes of the build's `ClientPlayerInput`,
    ///   which nothing else touches during the call.
    pub(crate) unsafe fn set_movement_stick(&self, input: *mut u8, stick: [f32; 2]) {
        // SAFETY: the caller's record, which `resolve` checked holds the
        // pair at `movement_stick`.
        unsafe { enshwick_core::raw::write_at(input, self.movement_stick, stick) };
    }

    /// Writes the local player's look input.
    ///
    /// # Safety
    ///
    /// - `input` is valid for writes of the build's `ClientPlayerInput`,
    ///   which nothing else touches during the call.
    pub(crate) unsafe fn set_camera_rotation(&self, input: *mut u8, look: Look) {
        // SAFETY: the caller's record, which `resolve` checked holds the
        // look at `camera_rotation`.
        unsafe { enshwick_core::raw::write_at(input, self.camera_rotation, look) };
    }

    /// The local player's movement stick in its `ClientPlayerInput`.
    ///
    /// # Safety
    ///
    /// - `input` is valid for reads of the build's `ClientPlayerInput`.
    pub(crate) unsafe fn movement_stick(&self, input: *const u8) -> [f32; 2] {
        // SAFETY: the caller's record, which `resolve` checked holds the
        // pair at `movement_stick`.
        unsafe { enshwick_core::raw::read_at(input, self.movement_stick) }
    }

    /// The local player's look input in its `ClientPlayerInput`.
    ///
    /// # Safety
    ///
    /// - `input` is valid for reads of the build's `ClientPlayerInput`.
    pub(crate) unsafe fn camera_rotation(&self, input: *const u8) -> Look {
        // SAFETY: the caller's record, which `resolve` checked holds the
        // look at `camera_rotation`.
        unsafe { enshwick_core::raw::read_at(input, self.camera_rotation) }
    }
}

/// `toggleHudScreen(ui context, HudScreenType)`: appends the screen change
/// to the interface's command queue, 1 when taken, 0 when the queue is full
/// (client 26430625).
type ToggleFn = unsafe extern "system" fn(context: usize, screen: u8) -> u8;

impl Toggle {
    fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let id = pinned::ui::TOGGLE_HUD_SCREEN.id();
        // Called, not hooked: only a route whose own check matched the
        // function's first instructions counts (`check` in its route).
        if !loader.build.routed(id) {
            return Err(id);
        }
        Ok(Self {
            toggle_hud_screen: usize::try_from(address(loader, pinned::ui::TOGGLE_HUD_SCREEN)?)
                .map_err(|_| id)?,
        })
    }

    /// Asks the interface for screen byte `screen`; whether its queue took
    /// it.
    ///
    /// # Safety
    ///
    /// - Called inside the interface frame on its thread, with `context` the
    ///   frame's own live UI context, which the global holds only then.
    pub(crate) unsafe fn toggle(&self, context: usize, screen: u8) -> bool {
        // SAFETY: the routed `toggleHudScreen`, its first instructions
        // checked by its route at link, of this type on every kept client.
        let toggle: ToggleFn =
            unsafe { core::mem::transmute::<usize, ToggleFn>(self.toggle_hud_screen) };
        // SAFETY: the caller's contract: the live context on its thread.
        unsafe { toggle(context, screen) != 0 }
    }
}

impl Recipes {
    fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        address(loader, pinned::ui::RECIPE_DIRECTORY)?;
        Ok(Self {
            highlighted: offset(loader, pinned::ui::RECIPE_HIGHLIGHTED)?,
            select: offset(loader, pinned::ui::RECIPE_SELECT)?,
        })
    }

    /// The recipe the browser highlights; `None` while it highlights none.
    ///
    /// # Safety
    ///
    /// - `browser` is the recipe browser block `doRecipeDirectoryColumn`
    ///   takes, valid for reads of a pointer at the routed
    ///   `ui.recipe_highlighted`, which is null or points at a recipe
    ///   record whose first `u32` is its id.
    pub(crate) unsafe fn highlighted_recipe(&self, browser: *const u8) -> Option<RecipeId> {
        // SAFETY: the caller's contract.
        let record: *const u8 =
            unsafe { enshwick_core::raw::read_ptr_at(browser, self.highlighted) };
        if record.is_null() {
            return None;
        }
        // SAFETY: a non-null record pointer the browser keeps, whose first
        // `u32` is its id, by the caller's contract.
        let id: u32 = unsafe { enshwick_core::raw::read_at(record, 0) };
        (id != 0).then_some(RecipeId(id))
    }

    /// The recipe the browser brings into focus next; `None` for none.
    ///
    /// # Safety
    ///
    /// - `browser` is the recipe browser block, valid for reads of a `u32`
    ///   at the routed `ui.recipe_select`.
    pub(crate) unsafe fn recipe_to_select(&self, browser: *const u8) -> Option<RecipeId> {
        // SAFETY: the caller's contract.
        let id: u32 = unsafe { enshwick_core::raw::read_at(browser, self.select) };
        (id != 0).then_some(RecipeId(id))
    }

    /// Asks the browser to bring `recipe` into focus, `None` for none: the
    /// tile row focuses the tile of that id and zeroes the slot once the
    /// tile is in the visible list (client 26430625, seen live).
    ///
    /// # Safety
    ///
    /// - `browser` is the recipe browser block, valid for a write of a
    ///   `u32` at the routed `ui.recipe_select`, in the interface frame on
    ///   its thread.
    pub(crate) unsafe fn select(&self, browser: *mut u8, recipe: Option<RecipeId>) {
        let id = recipe.map_or(0, |recipe| recipe.0);
        // SAFETY: the caller's contract.
        unsafe { enshwick_core::raw::write_at(browser, self.select, id) };
    }
}

/// Whether `size` bytes at `at` lie inside a record of `record` bytes.
fn fits(at: usize, size: usize, record: usize) -> bool {
    at.checked_add(size).is_some_and(|end| end <= record)
}

/// The input layout the builder's benchmark runs on, client 26430625's,
/// over buffers of its own; never linked in a game.
pub(crate) const BENCH_INPUT: InputLayout = InputLayout {
    movement_stick: 0x2b8,
    camera_rotation: 0x2c4,
    target_selection: 0x2ec,
    digital_input: 0x3a8,
    blocked: 0xad5a,
};

/// Links the input group with [`BENCH_INPUT`], for the builder's benchmark.
pub(crate) fn bench_input() {
    let _first = GROUPS.input.layout.set(BENCH_INPUT);
}

impl InputLayout {
    /// The offset of the movement stick, for a stand-in of the game's pass.
    pub(crate) const fn stick_offset(&self) -> usize {
        self.movement_stick
    }
}

pub(crate) fn link(loader: &Loader) {
    if loader.side != Side::CLIENT {
        return;
    }
    GROUPS.tab.link(loader, Tab::resolve(loader));
    GROUPS.toggle.link(loader, Toggle::resolve(loader));
    GROUPS.input.link(loader, InputLayout::resolve(loader));
    GROUPS.recipes.link(loader, Recipes::resolve(loader));
}

/// A look input per device, each a float pair, as the input builder writes
/// `cameraRotationInput` for this frame.
pub(crate) type Look = [[f32; 2]; LOOK_DEVICES];

/// A tab of the in-game menu, by the index it draws by (client 26430625,
/// the order seen live).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuTab {
    Social,
    Map,
    Journal,
    PlayerCrafting,
    Backpack,
    Character,
    SkillTree,
}

impl MenuTab {
    /// Every tab, in the menu's index order.
    pub(crate) const ALL: [Self; 7] = [
        Self::Social,
        Self::Map,
        Self::Journal,
        Self::PlayerCrafting,
        Self::Backpack,
        Self::Character,
        Self::SkillTree,
    ];

    /// The tab at the menu's index; `None` for one the build adds.
    fn from_index(index: u64) -> Option<Self> {
        usize::try_from(index)
            .ok()
            .and_then(|at| Self::ALL.get(at).copied())
    }

    /// The menu's index of the tab.
    pub(crate) fn index(self) -> u64 {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0) as u64
    }

    /// The `HudScreenType` member the tab shows, which `link` checks the
    /// build reflects.
    pub(crate) const fn name(self) -> &'static str {
        // Declared in the menu's index order, as the rules list the names.
        enshwick_core::__private::rules::MENU_TABS[self as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tab of client 26430625, its menu screens by their bytes there.
    fn tab_26430625() -> Tab {
        let mut menu_screens = [0; 4];
        for screen in [4u8, 5, 6, 7, 8, 9, 10, 0x2c] {
            menu_screens[usize::from(screen / 64)] |= 1 << (screen % 64);
        }
        Tab {
            menu_tab: 0x3_1e88,
            menu_screens,
        }
    }

    #[test]
    fn a_field_counts_only_when_its_bytes_lie_inside_the_record() {
        assert!(fits(0x2b8, 8, 0x600));
        assert!(fits(0x5f8, 8, 0x600), "the last eight bytes");
        assert!(!fits(0x5f9, 8, 0x600));
        assert!(!fits(usize::MAX, 8, 0x600), "an overflowing end");
    }

    /// Writes `value` at `at` of `record`, inside it.
    fn put<T: bytemuck::NoUninit>(record: &mut [u8], at: usize, value: T) {
        assert!(fits(at, size_of::<T>(), record.len()));
        // SAFETY: the check above keeps the write inside `record`.
        unsafe { enshwick_core::raw::write_at(record.as_mut_ptr(), at, value) };
    }

    #[test]
    fn the_stick_and_the_look_read_from_the_input_record() {
        let mut input = vec![0u8; 0x600];
        let look: Look = [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0], [7.0, 8.0], [9.0, 10.0]];
        put(&mut input, 0x2b8, [0.5f32, -1.0]);
        put(&mut input, 0x2c4, look);
        let layout = BENCH_INPUT;
        // SAFETY: the record above, of the build's size.
        let stick = unsafe { layout.movement_stick(input.as_ptr()) };
        // SAFETY: the same record.
        let read = unsafe { layout.camera_rotation(input.as_ptr()) };
        assert_eq!(stick, [0.5, -1.0]);
        assert_eq!(read, look);
    }

    #[test]
    fn the_hold_clears_the_buttons_every_pass_and_the_stick_or_look_only_in_its_own() {
        let look: Look = [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0], [7.0, 8.0], [9.0, 10.0]];
        let written = || {
            let mut input = vec![0u8; 0x600];
            put(&mut input, 0x2b8, [0.5f32, -1.0]);
            put(&mut input, 0x2c4, look);
            put(&mut input, 0x2ec, [0.25f32, 0.75]);
            put(&mut input, 0x3a8, u64::MAX);
            input
        };
        let layout = BENCH_INPUT;
        let read = |input: &[u8]| {
            // SAFETY: each read lies inside the record, of the build's size.
            unsafe {
                (
                    layout.movement_stick(input.as_ptr()),
                    layout.camera_rotation(input.as_ptr()),
                    enshwick_core::raw::read_at::<[f32; 2]>(input.as_ptr(), 0x2ec),
                    enshwick_core::raw::read_at::<u64>(input.as_ptr(), 0x3a8),
                )
            }
        };
        let mut step = written();
        // SAFETY: the record above, of the build's size, not shared.
        unsafe { layout.hold(step.as_mut_ptr(), pass::STEP) };
        assert_eq!(read(&step), ([0.0; 2], look, [0.0; 2], 0), "the step pass");
        let mut frame = written();
        // SAFETY: as above.
        unsafe { layout.hold(frame.as_mut_ptr(), pass::FRAME) };
        assert_eq!(
            read(&frame),
            ([0.5, -1.0], Look::default(), [0.0; 2], 0),
            "the frame pass"
        );
    }

    #[test]
    fn the_menu_tab_reads_by_index_and_an_unknown_index_is_none() {
        let mut state = vec![0u8; 0x3_1e90];
        let tab = tab_26430625();
        for (index, shown) in [
            (0u64, Some(MenuTab::Social)),
            (4, Some(MenuTab::Backpack)),
            (6, Some(MenuTab::SkillTree)),
            (7, None),
        ] {
            put(&mut state, 0x3_1e88, index);
            // SAFETY: the state holds a u64 at the tab index.
            assert_eq!(unsafe { tab.shown(state.as_ptr(), 4) }, shown, "{index}");
        }
    }

    #[test]
    fn the_tab_names_nothing_while_no_menu_screen_is_open() {
        let mut state = vec![0u8; 0x3_1e90];
        put(&mut state, 0x3_1e88, 4u64);
        let tab = tab_26430625();
        // SAFETY: the state holds a u64 at the tab index.
        let shown = |screen| unsafe { tab.shown(state.as_ptr(), screen) };
        assert_eq!(shown(1), None, "the world, after Esc closed the menu");
        assert_eq!(shown(0x15), None, "a storage chest");
        assert_eq!(shown(8), Some(MenuTab::Backpack), "V, then E");
        assert_eq!(shown(0x2c), Some(MenuTab::Backpack), "the social screen");
    }

    #[test]
    fn each_tab_names_its_screen_and_keeps_its_index() {
        for (index, tab) in MenuTab::ALL.into_iter().enumerate() {
            assert_eq!(tab.index(), index as u64);
            assert_eq!(MenuTab::from_index(tab.index()), Some(tab));
            assert!(MENU_SCREENS.contains(&tab.name()), "{tab:?}");
        }
    }

    #[test]
    fn the_browser_reads_its_highlighted_recipe_and_none_without_one() {
        let record = [0x1234_u32, 7];
        let mut browser = vec![0u8; 0xf0];
        let layout = Recipes {
            highlighted: 0xe8,
            select: 0x48,
        };
        let read = |browser: &[u8]| {
            // SAFETY: the block holds 0xf0 bytes, past both fields, and its
            // record pointer is null or `record`, whose first u32 is its id.
            let highlighted = unsafe { layout.highlighted_recipe(browser.as_ptr()) };
            // SAFETY: the same block.
            let selected = unsafe { layout.recipe_to_select(browser.as_ptr()) };
            (highlighted, selected)
        };
        assert_eq!(read(&browser), (None, None), "null and 0 are none");
        put(&mut browser, 0xe8, record.as_ptr() as usize);
        put(&mut browser, 0x48, 0x99_u32);
        assert_eq!(
            read(&browser),
            (Some(RecipeId(0x1234)), Some(RecipeId(0x99)))
        );
    }

    #[test]
    fn a_server_links_nothing_and_a_client_build_without_the_entries_turns_each_group_off() {
        let loader = crate::start::tests::bare_loader();
        link(&loader);
        assert!(
            !GROUPS.tab.fuse.is_off() && GROUPS.tab.layout.get().is_none(),
            "a server"
        );
        let mut loader = crate::start::tests::bare_loader();
        loader.side = Side::CLIENT;
        link(&loader);
        for fuse in [
            &GROUPS.tab.fuse,
            &GROUPS.toggle.fuse,
            &GROUPS.input.fuse,
            &GROUPS.recipes.fuse,
        ] {
            assert!(fuse.is_off(), "{}", fuse.feature());
        }
        assert!(GROUPS.tab.get().is_none());
        let listed = crate::engine::diag::parts_off();
        let parts: Vec<&str> = listed.iter().map(|p| p.part.as_str()).collect();
        assert!(parts.contains(&"the in-game menu's tab"), "{parts:?}");
    }
}
