//! Where a client's own controls are found on the running build: the input
//! builder and the stick and look it writes, the HUD's own screen toggle,
//! the in-game menu's tab and the crafting menu's recipe browser, each by
//! an engine route, so every kept client finds them.
//!
//! The layout is resolved once at link and read through the readers here;
//! a build that lacks an entry turns the part off with a log line. The part
//! joins the loader's parts with the host entries that read it, so until
//! then no build lists it off.
#![expect(
    dead_code,
    reason = "the host API entries that drive the client's controls read the layout through these"
)]

use std::sync::OnceLock;

use enshwick_core::RecipeId;
use enshwick_core::abi::Side;
use enshwick_gamedata::catalogue;

use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::input::CAMERA_ROTATION.id(),
    pinned::input::CLIENT_BUILD.id(),
    pinned::input::MOVEMENT_STICK.id(),
    pinned::ui::MENU_TAB.id(),
    pinned::ui::RECIPE_DIRECTORY.id(),
    pinned::ui::RECIPE_HIGHLIGHTED.id(),
    pinned::ui::RECIPE_SELECT.id(),
    pinned::ui::TOGGLE_HUD_SCREEN.id(),
];

/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "the client's own controls";

/// `MultiDeviceDirectionalInput`: digital, analog, mouse, touch pad and
/// gyro, a float pair each (client 26430625); `resolve` checks the
/// reflected size against it.
const LOOK_DEVICES: usize = 5;

/// Where the client's controls live on this build: code at its address in
/// the running image, fields at their offset in their records.
#[derive(Debug)]
pub(crate) struct Layout {
    /// The function that builds the local player's `ClientPlayerInput`.
    pub(crate) client_build: u64,
    /// `ClientPlayerInput.data.movementStick`, a float pair.
    pub(crate) movement_stick: usize,
    /// `ClientPlayerInput.data.cameraRotationInput`.
    pub(crate) camera_rotation: usize,
    /// `toggleHudScreen(ui context, HudScreenType)`.
    pub(crate) toggle_hud_screen: u64,
    /// The in-game menu's tab index in the interface state.
    pub(crate) menu_tab: usize,
    /// `doRecipeDirectoryColumn(node, book, browser, ctx)`.
    pub(crate) recipe_directory: u64,
    /// The browser's pointer to the highlighted recipe's record.
    pub(crate) recipe_highlighted: usize,
    /// The browser's recipe to bring into focus, 0 for none.
    pub(crate) recipe_select: usize,
}

static LAYOUT: OnceLock<Layout> = OnceLock::new();
static FUSE: Fuse = Fuse::new(FEATURE);

impl Layout {
    /// The layout of `loader`'s build, or the id of what it lacks.
    fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let address = |id: pinned::Symbol| loader.address(id).ok_or(id.id());
        let offset = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
        let input = loader
            .reflection
            .type_info(catalogue::type_name(catalogue::ids::CLIENT_PLAYER_INPUT))
            .map_err(|_| catalogue::ids::CLIENT_PLAYER_INPUT)?
            .size();
        let input = usize::try_from(input).unwrap_or(0);
        let look = loader
            .reflection
            .type_info(catalogue::type_name(
                catalogue::ids::MULTI_DEVICE_DIRECTIONAL_INPUT,
            ))
            .map_err(|_| catalogue::ids::MULTI_DEVICE_DIRECTIONAL_INPUT)?
            .size();
        if !same_size(look, size_of::<Look>()) {
            return Err(catalogue::ids::MULTI_DEVICE_DIRECTIONAL_INPUT);
        }
        let inside = |id: pinned::Constant, size: usize| {
            offset(id).and_then(|at| fits(at, size, input).then_some(at).ok_or(id.id()))
        };
        Ok(Self {
            client_build: address(pinned::input::CLIENT_BUILD)?,
            movement_stick: inside(pinned::input::MOVEMENT_STICK, size_of::<[f32; 2]>())?,
            camera_rotation: inside(pinned::input::CAMERA_ROTATION, size_of::<Look>())?,
            toggle_hud_screen: address(pinned::ui::TOGGLE_HUD_SCREEN)?,
            menu_tab: offset(pinned::ui::MENU_TAB)?,
            recipe_directory: address(pinned::ui::RECIPE_DIRECTORY)?,
            recipe_highlighted: offset(pinned::ui::RECIPE_HIGHLIGHTED)?,
            recipe_select: offset(pinned::ui::RECIPE_SELECT)?,
        })
    }
}

/// Whether a reflected size of `reflected` bytes is the `ours` we read.
fn same_size(reflected: u32, ours: usize) -> bool {
    usize::try_from(reflected).is_ok_and(|reflected| reflected == ours)
}

/// Whether `size` bytes at `at` lie inside a record of `record` bytes.
fn fits(at: usize, size: usize, record: usize) -> bool {
    at.checked_add(size).is_some_and(|end| end <= record)
}

pub(crate) fn link(loader: &Loader) {
    if loader.side != Side::CLIENT {
        return;
    }
    match Layout::resolve(loader) {
        Ok(layout) => {
            let _first = LAYOUT.set(layout);
        }
        Err(id) => {
            let _off = loader.fuse_off(&FUSE, id, "this build lacks it");
        }
    }
}

/// The layout, once linked on a client whose build has every entry.
pub(crate) fn layout() -> Option<&'static Layout> {
    LAYOUT.get().filter(|_| !FUSE.is_off())
}

/// A look input per device, each a float pair, as the input builder writes
/// `cameraRotationInput` for this frame.
pub(crate) type Look = [[f32; 2]; LOOK_DEVICES];

/// A tab of the in-game menu, by the index it draws by (client 26430625).
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
    const ALL: [Self; 7] = [
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
}

impl Layout {
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

    /// The in-game menu's tab, from the interface state; the index names
    /// the shown tab only while a menu screen is open, so a caller asks
    /// only then.
    ///
    /// # Safety
    ///
    /// - `state` is the interface state the in-game menu draws from, valid
    ///   for reads of a `u64` at the routed `ui.menu_tab`.
    pub(crate) unsafe fn menu_tab(&self, state: *const u8) -> Option<MenuTab> {
        // SAFETY: the caller's contract.
        MenuTab::from_index(unsafe { enshwick_core::raw::read_at(state, self.menu_tab) })
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
            unsafe { enshwick_core::raw::read_ptr_at(browser, self.recipe_highlighted) };
        if record.is_null() {
            return None;
        }
        // SAFETY: a non-null record pointer the browser keeps, whose first
        // `u32` is the recipe's id, by the caller's contract.
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
        let id: u32 = unsafe { enshwick_core::raw::read_at(browser, self.recipe_select) };
        (id != 0).then_some(RecipeId(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout of client 26430625 at its preferred image base.
    fn client_26430625() -> Layout {
        Layout {
            client_build: 0x1_4024_77d0,
            movement_stick: 0x2b8,
            camera_rotation: 0x2c4,
            toggle_hud_screen: 0x1_40d7_4850,
            menu_tab: 0x3_1e88,
            recipe_directory: 0x1_40c7_5660,
            recipe_highlighted: 0xe8,
            recipe_select: 0x48,
        }
    }

    #[test]
    fn the_look_counts_only_when_the_build_reflects_its_size() {
        assert!(same_size(0x28, size_of::<Look>()));
        assert!(
            !same_size(0x8, size_of::<Look>()),
            "a build that drops devices"
        );
        assert!(!same_size(0x30, size_of::<Look>()), "a build that adds one");
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
        let layout = client_26430625();
        // SAFETY: the record above, of the build's size.
        let stick = unsafe { layout.movement_stick(input.as_ptr()) };
        // SAFETY: the same record.
        let read = unsafe { layout.camera_rotation(input.as_ptr()) };
        assert_eq!(stick, [0.5, -1.0]);
        assert_eq!(read, look);
    }

    #[test]
    fn the_menu_tab_reads_by_index_and_an_unknown_index_is_none() {
        let mut state = vec![0u8; 0x3_1e90];
        let layout = client_26430625();
        for (index, tab) in [
            (0u64, Some(MenuTab::Social)),
            (4, Some(MenuTab::Backpack)),
            (6, Some(MenuTab::SkillTree)),
            (7, None),
        ] {
            put(&mut state, 0x3_1e88, index);
            // SAFETY: the state holds a u64 at the tab index.
            assert_eq!(unsafe { layout.menu_tab(state.as_ptr()) }, tab, "{index}");
        }
    }

    #[test]
    fn the_browser_reads_its_highlighted_recipe_and_none_without_one() {
        let record = [0x1234_u32, 7];
        let mut browser = vec![0u8; 0xf0];
        let layout = client_26430625();
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
    fn a_server_links_nothing_and_a_client_build_without_the_entries_turns_the_part_off() {
        let loader = crate::start::tests::bare_loader();
        link(&loader);
        assert!(!FUSE.is_off() && LAYOUT.get().is_none(), "a server");
        let mut loader = crate::start::tests::bare_loader();
        loader.side = Side::CLIENT;
        link(&loader);
        assert!(FUSE.is_off());
        assert!(layout().is_none());
    }
}
