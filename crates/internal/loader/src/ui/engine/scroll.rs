//! The game's scroll area, as the world settings, the server list and its
//! details panel draw it: a sized frame, the scroll inside it with its
//! scrollbar and navigation, and a column of rows inside the scroll.
//!
//! Engine facts from client 426, the same as on 1269579: the calls, their
//! order and every offset below are the world settings' own; a probe saw
//! the begin and end pairs live on 426.

use super::{
    BUNDLE, Buf, Engine, FRAME_SIZE, Loader, Missing, NAV_CONFIG, OneFn, Ptr, game_fn, path, sound,
    stretch,
};
use crate::pinned;

type PersistFn = unsafe extern "system" fn(
    node: usize,
    key: u64,
    size: u64,
    construct: usize,
    count: u32,
) -> *mut u8;
type FirstUseFn = unsafe extern "system" fn(state: *mut u8) -> u8;
type InitFn = unsafe extern "system" fn(object: Ptr) -> Ptr;
type ScrollBeginFn = unsafe extern "system" fn(
    scroll: Ptr,
    vertical: u8,
    state: *mut u8,
    unused: usize,
    config: *const u8,
) -> Ptr;
type ScrollFocusFn = unsafe extern "system" fn(scroll: Ptr) -> f32;
type ScrollSetFocusFn = unsafe extern "system" fn(scroll: Ptr, focus: f32);
type ScrollNavFn = unsafe extern "system" fn(scroll: Ptr, nav: *const u8);
type ScrollInputFn = unsafe extern "system" fn(scroll: Ptr, input: *const [u64; 2]);
type ColumnFn = unsafe extern "system" fn(column: Ptr, gap: f32) -> Ptr;

/// The scroll object: its start writes up to 0x180 bytes, its end reaches
/// 0x468.
const SCROLL_OBJECT: usize = 0x600;
/// The scroll config; the game's is 0x108 bytes.
const SCROLL_CONFIG: usize = 0x110;
/// The scrollbar look, copied whole into the config.
const SCROLLBAR_LOOK: usize = 0xc0;
/// The kept state of a scroll area, which holds its offset; the game's
/// first use zeroes part of it, the loader all of it.
const STATE_SIZE: usize = 0x18;
/// The loader's key for its scroll area's kept state among the game's
/// states of a node: `enshwick` in ASCII.
const STATE_KEY: u64 = 0x656e_7368_7769_636b;

/// The scrollbar look's fields, as the world settings fill them.
const LOOK_TINT: usize = 0x04;
const LOOK_RANGE: usize = 0x24;
const LOOK_SLIDER: usize = 0x44;
const LOOK_SHOWN: usize = 0x7c;
/// Four floats from here: the bar's widths and its slider's least length.
const LOOK_SIZES: usize = 0x80;
const LOOK_SIZE_VALUES: [f32; 4] = [24.0, 24.0, 24.0, 20.0];
const LOOK_FILLED: usize = 0x90;
/// A nine-slice texture of the look bundle, copied into the look.
const TEXTURE_SIZE: usize = 32;

/// The scroll config's fields, as the world settings fill them.
const CONFIG_FLAGS: usize = 0x40;
const CONFIG_FLAGS_SET: u8 = 0x18;
const CONFIG_AXES: usize = 0x2e;
const CONFIG_AXES_ON: u16 = 0x101;
const CONFIG_BAR: usize = 0x32;
const CONFIG_LOOK: usize = 0x44;

/// The navigation config's highlight sound, which the window's navigation
/// leaves empty.
const SOUND_SIZE: usize = 16;
/// The navigation kind of a scrolled column.
const NAV_SCROLLED: u32 = 3;

const _: () = assert!(CONFIG_LOOK + SCROLLBAR_LOOK <= SCROLL_CONFIG);
const _: () = assert!(LOOK_SLIDER + TEXTURE_SIZE <= SCROLLBAR_LOOK);

/// The scroll area's functions and the bundle's looks for its scrollbar.
#[derive(Debug)]
pub(super) struct Scrolling {
    persist: PersistFn,
    first_use: FirstUseFn,
    config: InitFn,
    look: InitFn,
    begin: ScrollBeginFn,
    focus: ScrollFocusFn,
    set_focus: ScrollSetFocusFn,
    nav: ScrollNavFn,
    input: ScrollInputFn,
    end: OneFn,
    column: ColumnFn,
    no_construct: usize,
    slider: usize,
    range: usize,
    tint: usize,
    highlight: usize,
    /// Where the highlight sound sits in the navigation config, from the
    /// table's `ui.nav_highlight`.
    nav_highlight: usize,
}

fn scrollbar(loader: &Loader, name: &str) -> Option<usize> {
    path(
        loader,
        &[
            (BUNDLE, "menu"),
            ("keen::FbUiMenu", "scrollbar"),
            ("keen::FbUiMenuScrollbar", name),
        ],
    )
}

/// The highlight sound's offset in the navigation config, kept inside the
/// loader's config buffer.
fn nav_highlight(loader: &Loader) -> Result<usize, Missing> {
    let id = pinned::ui::NAV_HIGHLIGHT.id();
    let field = |id: &'static str| {
        loader
            .build
            .constant(id)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(Missing::Entry(id))
    };
    let at = field(id)?
        .checked_sub(field(pinned::ui::NAV_CONFIG.id())?)
        .ok_or(Missing::Room(id))?;
    if at + SOUND_SIZE > NAV_CONFIG {
        return Err(Missing::Room(id));
    }
    Ok(at)
}

impl Scrolling {
    /// Resolves every function and look of the scroll area.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        macro_rules! function {
            ($name:expr, $ty:ty) => {
                // SAFETY: `$ty` is the signature of the game function `$name`
                // names, from the world settings' calls on client 426.
                unsafe { game_fn::<$ty>(loader, $name) }?
            };
        }
        Ok(Self {
            persist: function!(pinned::ui::PERSIST, PersistFn),
            first_use: function!(pinned::ui::FIRST_USE, FirstUseFn),
            config: function!(pinned::ui::SCROLL_CONFIG, InitFn),
            look: function!(pinned::ui::SCROLLBAR_LOOK, InitFn),
            begin: function!(pinned::ui::SCROLL_BEGIN, ScrollBeginFn),
            focus: function!(pinned::ui::SCROLL_FOCUS, ScrollFocusFn),
            set_focus: function!(pinned::ui::SCROLL_SET_FOCUS, ScrollSetFocusFn),
            nav: function!(pinned::ui::SCROLL_NAV, ScrollNavFn),
            input: function!(pinned::ui::SCROLL_INPUT, ScrollInputFn),
            end: function!(pinned::ui::SCROLL_END, OneFn),
            column: function!(pinned::ui::COLUMN, ColumnFn),
            no_construct: super::verify(loader, pinned::ui::NO_CONSTRUCT)?,
            slider: scrollbar(loader, "slider").ok_or(Missing::Looks)?,
            range: scrollbar(loader, "range").ok_or(Missing::Looks)?,
            tint: scrollbar(loader, "rangeTint").ok_or(Missing::Looks)?,
            highlight: sound(loader, "highlightChange").ok_or(Missing::Looks)?,
            nav_highlight: nav_highlight(loader)?,
        })
    }
}

/// A scroll area's own objects, kept in place from its start to its end:
/// the sized frame around it, the scroll and the column inside.
pub(in crate::ui) struct Scroll {
    parent: Buf<FRAME_SIZE>,
    area: Buf<SCROLL_OBJECT>,
    column: Buf<FRAME_SIZE>,
    /// What the start opened, for the end to close.
    opened: Opened,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Opened {
    Nothing,
    Parent,
    All,
}

impl Scroll {
    pub(in crate::ui) const fn new() -> Self {
        Self {
            parent: Buf::zeroed(),
            area: Buf::zeroed(),
            column: Buf::zeroed(),
            opened: Opened::Nothing,
        }
    }
}

impl Engine {
    /// Starts a scroll area `height` high and as wide as the window, as the
    /// world settings scroll their tabs: rows drawn until
    /// [`Engine::scroll_end`] go into it, `gap` apart, and the game keeps
    /// its offset from frame to frame. True when the rows scroll; false on
    /// a build without scroll areas, which opens nothing, and when the game
    /// keeps no state for the area this frame, which leaves only a frame
    /// `height` high open.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - `scroll` stays in place until [`Engine::scroll_end`] ends it in the
    ///   same frame.
    pub(in crate::ui) unsafe fn scroll_begin(
        &self,
        context: usize,
        scroll: &mut Scroll,
        height: f32,
        gap: f32,
    ) -> bool {
        scroll.opened = Opened::Nothing;
        let Some(fns) = &self.scroll else {
            return false;
        };
        // SAFETY: `self.frame_open` is the game's `ui.frame_open`, checked in
        // `Engine::resolve`, inside the window this function's `# Safety`
        // gives; `scroll.parent` holds the table's `ui.frame_extent` bytes
        // (`Layout::read`) and stays in place until `scroll_end`.
        unsafe { (self.frame_open)(scroll.parent.ptr(), 0) };
        scroll.opened = Opened::Parent;
        let parent = self.frame_node(&scroll.parent);
        // SAFETY: `self.node_height` is the game's `ui.node_height`, checked
        // in `Engine::resolve`, on the node the game just wrote into the
        // parent frame.
        unsafe { (self.node_height)(parent, height) };
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
        // checked in `Engine::resolve`, on the parent frame's node.
        unsafe { (self.node_stretch)(parent, stretch(1.0, 0.0)) };
        // SAFETY: `fns.persist` is the game's `ui.persist`, checked in
        // `Scrolling::resolve`, on the parent frame's live node; it hands out
        // `STATE_SIZE` bytes the game keeps for the node under the loader's
        // key, built by `ui.no_construct`, as the world settings ask.
        let state =
            unsafe { (fns.persist)(parent, STATE_KEY, STATE_SIZE as u64, fns.no_construct, 1) };
        if state.is_null() {
            return false;
        }
        // SAFETY: `fns.first_use` is the game's `ui.first_use`, checked in
        // `Scrolling::resolve`, on the non-null state `persist` just handed
        // out, as the world settings call it.
        if unsafe { (fns.first_use)(state) } != 0 {
            // SAFETY: `state` is non-null, checked by the guard above, and
            // points at the `STATE_SIZE` bytes `persist` handed out for this
            // node, which nothing else touches on this thread now.
            unsafe { state.write_bytes(0, STATE_SIZE) };
        }
        let bundle = self.bundle(context);
        let mut config = Self::scroll_config(fns, bundle);
        let mut nav = self.scroll_nav(fns, bundle);
        let area = scroll.area.ptr();
        // SAFETY: `fns.begin` is the game's `ui.scroll_begin`, checked in
        // `Scrolling::resolve`, inside the parent frame; `scroll.area` holds
        // `SCROLL_OBJECT` bytes, more than the 0x468 the scroll reaches on
        // 426, and stays in place until `scroll_end`; `state` is the kept
        // state and `config` a filled local the game reads.
        unsafe { (fns.begin)(area, 1, state, 0, config.ptr()) };
        // SAFETY: `fns.focus` is the game's `ui.scroll_focus`, checked in
        // `Scrolling::resolve`, on the scroll begun one call earlier.
        let focus = unsafe { (fns.focus)(area) };
        // SAFETY: `fns.set_focus` is the game's `ui.scroll_set_focus`,
        // checked in `Scrolling::resolve`, on the begun scroll.
        unsafe { (fns.set_focus)(area, focus) };
        let node = self.frame_node(&scroll.area);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
        // checked in `Engine::resolve`, on the node the scroll's begin wrote.
        unsafe { (self.node_stretch)(node, stretch(1.0, 1.0)) };
        // SAFETY: `fns.nav` is the game's `ui.scroll_nav`, checked in
        // `Scrolling::resolve`, on the begun scroll with a filled navigation
        // config of the table's `ui.nav_config_size` bytes.
        unsafe { (fns.nav)(area, nav.ptr()) };
        let mut input = [0u64; 2];
        // SAFETY: `self.nav_input` is the game's `ui.nav_input`, checked in
        // `Engine::resolve`; it writes sixteen bytes to the live `input` and
        // reads the navigation config.
        unsafe { (self.nav_input)(&raw mut input, nav.ptr(), 0) };
        // SAFETY: `fns.input` is the game's `ui.scroll_input`, checked in
        // `Scrolling::resolve`, on the begun scroll with the live `input`.
        unsafe { (fns.input)(area, &raw const input) };
        // SAFETY: `fns.column` is the game's `ui.column`, checked in
        // `Scrolling::resolve`, inside the begun scroll; `scroll.column`
        // holds the table's `ui.frame_extent` bytes and stays in place until
        // `scroll_end`.
        unsafe { (fns.column)(scroll.column.ptr(), gap) };
        scroll.opened = Opened::All;
        let column = self.frame_node(&scroll.column);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
        // checked in `Engine::resolve`, on the node the column just wrote.
        unsafe { (self.node_stretch)(column, stretch(1.0, 0.0)) };
        true
    }

    /// Ends the scroll area [`Engine::scroll_begin`] started, closing what
    /// it opened.
    ///
    /// # Safety
    ///
    /// - Called in the frame of the [`Engine::scroll_begin`] that started
    ///   `scroll`, on its thread, inside the same window.
    /// - `scroll` did not move since, and every frame opened inside it is
    ///   closed.
    pub(in crate::ui) unsafe fn scroll_end(&self, scroll: &mut Scroll) {
        let opened = core::mem::replace(&mut scroll.opened, Opened::Nothing);
        if opened == Opened::Nothing {
            return;
        }
        if opened == Opened::All
            && let Some(fns) = &self.scroll
        {
            // SAFETY: `self.frame_close` is the game's `ui.frame_close`,
            // checked in `Engine::resolve`, on the column `scroll_begin`
            // opened in this frame, unmoved, with every frame inside closed
            // by this function's `# Safety`.
            unsafe { (self.frame_close)(scroll.column.ptr()) };
            // SAFETY: `fns.end` is the game's `ui.scroll_end`, checked in
            // `Scrolling::resolve`, on the scroll `scroll_begin` began in this
            // frame, whose column closed one call earlier; it keeps the offset
            // and drops the scroll's navigation group.
            unsafe { (fns.end)(scroll.area.ptr()) };
        }
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the parent frame `scroll_begin` opened in
        // this frame, whose scroll ended above.
        unsafe { (self.frame_close)(scroll.parent.ptr()) };
    }

    /// The scroll config with the game's scrollbar look, as the world
    /// settings fill it.
    fn scroll_config(fns: &Scrolling, bundle: usize) -> Buf<SCROLL_CONFIG> {
        let mut look = Buf::<SCROLLBAR_LOOK>::zeroed();
        // SAFETY: `fns.look` is the game's `ui.scrollbar_look`, checked in
        // `Scrolling::resolve`; it fills the defaults of the `SCROLLBAR_LOOK`
        // bytes of `look`.
        unsafe { (fns.look)(look.ptr()) };
        look.u8(LOOK_SHOWN, 1);
        for (at, from) in [(LOOK_SLIDER, fns.slider), (LOOK_RANGE, fns.range)] {
            // SAFETY: `bundle` is the frame's look bundle, read from the live
            // context, and `from` the reflected offset of a nine-slice
            // texture of `TEXTURE_SIZE` bytes inside it.
            let texture = unsafe { crate::memory::read::<[u8; TEXTURE_SIZE]>(bundle, from) };
            look.put(at, texture);
        }
        for (i, value) in LOOK_SIZE_VALUES.into_iter().enumerate() {
            look.f32(LOOK_SIZES + 4 * i, value);
        }
        // SAFETY: `bundle` is the frame's look bundle, read from the live
        // context, and `fns.tint` the reflected offset of a colour inside it.
        let tint = unsafe { crate::memory::read::<u64>(bundle, fns.tint) };
        look.u64(LOOK_TINT, tint);
        look.u8(LOOK_FILLED, 1);
        let mut config = Buf::<SCROLL_CONFIG>::zeroed();
        // SAFETY: `fns.config` is the game's `ui.scroll_config`, checked in
        // `Scrolling::resolve`; it fills the defaults of the game's 0x108
        // bytes of config, inside the `SCROLL_CONFIG` bytes of `config`.
        unsafe { (fns.config)(config.ptr()) };
        config.put(CONFIG_LOOK, look.0);
        let flags = config.0[CONFIG_FLAGS] | CONFIG_FLAGS_SET;
        config.u8(CONFIG_FLAGS, flags);
        config.u16(CONFIG_AXES, CONFIG_AXES_ON);
        config.u8(CONFIG_BAR, 1);
        config
    }

    /// The scroll's navigation config: the window's, as a scrolled column
    /// with the highlight sound.
    fn scroll_nav(&self, fns: &Scrolling, bundle: usize) -> Buf<NAV_CONFIG> {
        let mut nav = Buf::<NAV_CONFIG>::zeroed();
        let [kind, wrap, axis, flags, input, layer, sound] = self.layout.nav;
        nav.u32(kind, NAV_SCROLLED);
        nav.u8(wrap, 1);
        nav.u32(axis, 3);
        nav.u16(flags, 0x101);
        nav.u32(input, 2);
        nav.u16(layer, 0x1b);
        nav.u16(sound, 1);
        // SAFETY: `bundle` is the frame's look bundle, read from the live
        // context, and `fns.highlight` the reflected offset of a 16-byte
        // sound reference inside it.
        let highlight = unsafe { crate::memory::read::<[u8; 16]>(bundle, fns.highlight) };
        nav.put(fns.nav_highlight, highlight);
        nav
    }
}
