//! Lines mods add to the game's item tooltips and detail panels.
//!
//! Every item tooltip and detail panel is one immediate-mode body, drawn
//! each frame while it shows, with its descriptor in `rsi` from start to end:
//! the item at the reflected `ItemInstanceInfo.itemId`, its entity at `pide`
//! (client 26430625, decompiler). Two
//! mid-function hooks sit after the last vanilla row while the rows' frame is
//! open: the panel's site in the scroll area's column (layout 0), the plain
//! tooltip's in its main column (any other layout), which every layout passes,
//! so each draws only for its own. The mods' handlers run once each time the
//! body shows another item; the lines they gave are kept for the items one
//! frame draws (a detail panel, a tooltip and its compare view) and drawn each
//! frame.
//!
//! Per hit, a site pays the mid-hook stub, which keeps the full vector state
//! (`engine::mid`), three reads of the descriptor, and the lookup of the kept
//! lines, 1.6 ns (`benches/item_ui.rs`, `kept_tooltip_lines`); then a label
//! pair per line. Both sites hit once per frame a tooltip shows.

use core::cell::{Cell, RefCell};
use core::ops::ControlFlow;
use std::sync::OnceLock;

use enshwick_core::abi::{HookFlags, UiTooltipFn, UiTooltipItem, UiWidget};

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::profile::Feature;
use crate::engine::registry::{JoinFeatureError, Registry};
use crate::ui::widget_checks::WidgetError;

use super::{ENGINE, copy, next_token, read};

/// The tooltip lines: both sites and the mods' handlers.
pub(super) static FUSE: Fuse = Fuse::new("item tooltip lines");
/// Tooltip handlers: open while mods initialise, sealed after.
static HANDLERS: Registry<Registered<UiTooltipFn>> = Registry::new();
static LINKED: OnceLock<Linked> = OnceLock::new();

/// How many items the kept lines are for: one frame may draw a detail
/// panel's item, a hovered one and the equipped one its compare view shows,
/// with one to spare.
const KEPT: usize = 4;

/// Where the descriptor keeps what the sites read, on this build.
#[derive(Clone, Copy, Debug)]
struct Linked {
    layout: usize,
    unknown: usize,
    item: usize,
    pide: usize,
}

/// A line a handler gave: its style, its label's flags and its text.
#[derive(Clone, Debug)]
struct Line {
    style: u32,
    flags: u32,
    text: Box<[u8]>,
}

impl Line {
    /// Whether the line wraps and whether it is centred, as its label asks.
    const fn wrap_center(&self) -> (bool, bool) {
        (
            self.flags & UiWidget::WRAP != 0,
            self.flags & UiWidget::CENTER != 0,
        )
    }
}

/// The lines of one item, and the frame it last drew in.
#[derive(Clone, Debug, Default)]
struct Kept {
    item: u32,
    pide: u32,
    drawn: u64,
    lines: Vec<Line>,
}

/// The handler whose lines gather now: its token and mod, and how many it
/// gave.
#[derive(Clone, Copy)]
struct Gathering {
    token: u64,
    owner: ModIndex,
    count: u32,
}

thread_local! {
    static KEPT_LINES: RefCell<[Kept; KEPT]> = RefCell::new(Default::default());
    static GATHERING: Cell<Option<Gathering>> = const { Cell::new(None) };
    static GATHERED: RefCell<Vec<Line>> = const { RefCell::new(Vec::new()) };
    /// The interface frame, counted by [`end_frame`].
    static FRAME: Cell<u64> = const { Cell::new(1) };
    /// Whether each site logged its first hit: the panel's, then the plain
    /// one's.
    static FIRST_HIT: Cell<[bool; 2]> = const { Cell::new([false; 2]) };
}

/// Registers a mod's tooltip handler; client only, during init.
///
/// # Errors
///
/// [`JoinFeatureError`] off a client or after init.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    callback: UiTooltipFn,
    user: *mut core::ffi::c_void,
) -> Result<(), JoinFeatureError> {
    super::on_client(loader)?;
    Ok(HANDLERS.add(Registered::new(owner, callback, user))?)
}

/// Seals the handlers, before any link.
pub(super) fn seal() {
    let _sealed = HANDLERS.seal();
}

/// Counts an interface frame, so lines kept for an item whose tooltip
/// closed are asked again when it opens. On the interface thread.
pub(super) fn end_frame() {
    FRAME.set(FRAME.get() + 1);
}

/// Links both sites on a client build that has them, when a mod gave a
/// handler.
pub(super) fn link(loader: &Loader) {
    if HANDLERS.sealed().is_none_or(<[_]>::is_empty) && !crate::inventory::stats::wanted() {
        return;
    }
    let offset = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let linked = (|| {
        Ok::<_, &'static str>(Linked {
            layout: offset(pinned::ui::VIEWER_LAYOUT)?,
            unknown: offset(pinned::ui::VIEWER_UNKNOWN)?,
            item: offset(pinned::ui::VIEWER_ITEM)?,
            pide: offset(pinned::ui::VIEWER_PIDE)?,
        })
    })();
    let linked = match linked {
        Ok(linked) => linked,
        Err(id) => {
            loader.fuse_off(&FUSE, id, crate::engine::builds::ABSENT);
            return;
        }
    };
    let _first = LINKED.set(linked);
    let sites = [
        (
            pinned::ui::TOOLTIP_PANEL_SITE,
            panel_site as enshwick_core::abi::MidHookFn,
        ),
        (pinned::ui::TOOLTIP_PLAIN_SITE, plain_site),
    ];
    for (site, handler) in sites {
        if !loader.link_mid(&FUSE, Target::Pinned(site), handler, HookFlags::NONE) {
            return;
        }
    }
    let count = HANDLERS.sealed().map_or(0, <[_]>::len);
    crate::log!(INFO, "interface: {count} item tooltip handlers from mods");
}

crate::engine::mid::mid_handler! {
    /// The detail panel's site, after its last row in the scroll area's
    /// column: the mods' lines for a panel (layout 0).
    fn panel_site;
    fuse FUSE, "the item panel's lines";
    |registers| {
        // SAFETY: the hook at the table's `ui.tooltip_panel_site` runs inside
        // the item viewer's body with its descriptor in `rsi` and the
        // panel's column open, on the interface thread (client 26430625).
        unsafe { site(registers.rsi, Site::Panel) };
    }
}

crate::engine::mid::mid_handler! {
    /// The plain tooltip's site, after its last row in its main column:
    /// the mods' lines for a tooltip (any layout but 0).
    fn plain_site;
    fuse FUSE, "the item tooltip's lines";
    |registers| {
        // SAFETY: the hook at the table's `ui.tooltip_plain_site` runs inside
        // the item viewer's body with its descriptor in `rsi` and the main
        // column open, on the interface thread (client 26430625).
        unsafe { site(registers.rsi, Site::Plain) };
    }
}

/// Which of the two sites a hit is at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    Panel,
    Plain,
}

impl Site {
    /// Whether this site draws for the viewer's `layout`: the panel's for
    /// layout 0 only, the plain one's for every other, since every layout
    /// passes the plain site.
    const fn draws(self, layout: u32) -> bool {
        matches!((self, layout), (Self::Panel, 0)) || matches!(self, Self::Plain) && layout != 0
    }
}

/// Draws the mods' lines for the item the viewer's descriptor `desc` shows.
///
/// # Safety
///
/// - `desc` is the item viewer's descriptor at one of its sites, live for
///   the call, with the frame that holds its rows open, on the interface
///   thread.
unsafe fn site(desc: u64, at: Site) {
    use crate::engine::memory::read;
    let (Some(linked), Ok(desc)) = (LINKED.get(), usize::try_from(desc)) else {
        return;
    };
    // The interface frame turns the text arena and the kept lines' frame; without it a line would
    // stay kept forever and its text would fill the arena.
    if desc == 0 || !super::popup_runs() {
        return;
    }
    // SAFETY: the caller's contract: the descriptor is live, its layout at
    // the table's `ui.viewer_layout`.
    let layout = unsafe { read::<u32>(desc, linked.layout) };
    if !at.draws(layout) {
        return;
    }
    // SAFETY: the live descriptor's item at the reflected
    // `ItemInstanceInfo.itemId`.
    let item = unsafe { read::<u32>(desc, linked.item) };
    // SAFETY: the live descriptor's entity at the reflected
    // `ItemInstanceInfo.pide`.
    let pide = unsafe { read::<u32>(desc, linked.pide) };
    // SAFETY: the live descriptor's fixed-text byte at the table's
    // `ui.viewer_unknown`.
    let unknown = unsafe { read::<u8>(desc, linked.unknown) };
    let mut hit = FIRST_HIT.get();
    if !core::mem::replace(&mut hit[at as usize], true) {
        FIRST_HIT.set(hit);
        crate::log!(
            INFO,
            "interface: item tooltip {at:?} site first hit: item {item:#x}, pide {pide:#x}, layout {layout}"
        );
    }
    if pide == 0 || unknown != 0 {
        return;
    }
    let Some(engine) = ENGINE.get() else {
        return;
    };
    let context = engine.context();
    if context == 0 {
        return;
    }
    KEPT_LINES.with_borrow_mut(|kept| {
        let entry = lines_of(kept, item, pide);
        for line in &entry.lines {
            let text = copy(&[&line.text]);
            let (wrap, center) = line.wrap_center();
            // SAFETY: the caller's contract: the frame that holds the rows is
            // open on this thread; `context` is this frame's non-zero
            // context, and the text lives in the frame's arena.
            unsafe { engine.label(context, &text, line.style, wrap, center) };
        }
    });
}

/// The kept lines of `item` with entity `pide`, asked of the handlers when
/// they are not kept or did not draw in the last frame; the entry drawn
/// longest ago gives way.
fn lines_of(kept: &mut [Kept; KEPT], item: u32, pide: u32) -> &Kept {
    let frame = FRAME.get();
    let at = match kept.iter().position(|k| k.item == item && k.pide == pide) {
        Some(at) if kept[at].drawn + 1 >= frame => at,
        found => {
            let at = found.unwrap_or_else(|| {
                (0..KEPT)
                    .min_by_key(|&at| kept[at].drawn)
                    .unwrap_or_default()
            });
            kept[at] = Kept {
                item,
                pide,
                drawn: frame,
                lines: ask(item, pide),
            };
            at
        }
    };
    kept[at].drawn = frame;
    &kept[at]
}

/// The item's lines: the loader's own for its stat modifiers and affixes,
/// then every handler's of a mod that runs, in registration order.
fn ask(item: u32, pide: u32) -> Vec<Line> {
    GATHERED.with_borrow_mut(|lines| {
        lines.clear();
        lines.extend(stat_lines(item, pide));
    });
    let Some(handlers) = HANDLERS.sealed() else {
        return GATHERED.take();
    };
    let _none: Option<(_, ())> =
        crate::engine::filters::ask(handlers, Feature::Interface, |callback| {
            let token = next_token();
            GATHERING.set(Some(Gathering {
                token,
                owner: callback.owner(),
                count: 0,
            }));
            let opened = enshwick_abi::headed!(UiTooltipItem {
                token,
                item: enshwick_core::ItemId::new(item),
                pide: enshwick_core::Entity::from_engine(pide),
            });
            // SAFETY: the callback lives in a mod DLL that stays loaded, and it
            // gets a live `UiTooltipItem` and the pointer its mod registered with
            // it; an `extern "system"` function aborts rather than unwinds.
            callback.call(|callback, user| unsafe { callback(&raw const opened, user) });
            GATHERING.set(None);
            ControlFlow::Continue(())
        });
    GATHERED.take()
}

/// The lines of the item's stat modifiers and affixes, in the player's
/// language, a gain in the game's positive style and a loss in its negative
/// one.
fn stat_lines(item: u32, pide: u32) -> Vec<Line> {
    let Some(loader) = crate::engine::loader::loader() else {
        return Vec::new();
    };
    let language = crate::engine::texts::game_language();
    crate::inventory::stats::item_line_texts(loader, pide, item, language)
        .into_iter()
        .map(|line| Line {
            style: match line.sign {
                core::cmp::Ordering::Greater => UiWidget::STYLE_POSITIVE,
                core::cmp::Ordering::Less => UiWidget::STYLE_NEGATIVE,
                core::cmp::Ordering::Equal => UiWidget::STYLE_BODY,
            },
            flags: 0,
            text: line.text.into_bytes().into(),
        })
        .collect()
}

/// Keeps a tooltip handler's line; `None` when no handler of `owner` with
/// `token` gathers now, so the widget is not a tooltip's.
pub(super) fn gather(
    owner: ModIndex,
    token: u64,
    raw: &UiWidget,
) -> Option<Result<(), WidgetError>> {
    let mut gathering = GATHERING.get().filter(|g| g.token == token)?;
    if gathering.owner != owner {
        return Some(Err(WidgetError::WrongThread));
    }
    if raw.kind != UiWidget::LABEL {
        return Some(Err(WidgetError::TooltipRefused));
    }
    if gathering.count >= UiTooltipItem::MAX_LINES {
        return Some(Err(WidgetError::TooltipFull));
    }
    let Some(text) = read(raw.text) else {
        return Some(Err(WidgetError::Malformed));
    };
    GATHERED.with_borrow_mut(|lines| {
        lines.push(Line {
            style: raw.style,
            flags: raw.flags,
            text: text.into(),
        });
    });
    gathering.count += 1;
    GATHERING.set(Some(gathering));
    Some(Ok(()))
}

/// Readies the kept lines for their benchmark: the closure is the per-frame
/// lookup a site makes for an item whose lines are kept.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_tooltip_lines() -> impl FnMut() -> usize {
    let mut kept: [Kept; KEPT] = Default::default();
    let _first = lines_of(&mut kept, 7, 900);
    move || lines_of(&mut kept, 7, 900).lines.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_site_draws_for_layout_zero_and_the_plain_one_for_the_rest() {
        assert!(Site::Panel.draws(0));
        assert!(!Site::Panel.draws(2));
        assert!(!Site::Plain.draws(0));
        assert!(Site::Plain.draws(1));
        assert!(Site::Plain.draws(2));
    }

    #[test]
    fn kept_lines_ask_again_once_a_tooltip_closed() {
        let mut kept: [Kept; KEPT] = Default::default();
        let first = lines_of(&mut kept, 7, 900).drawn;
        let _second = lines_of(&mut kept, 8, 901);
        end_frame();
        let kept_on = lines_of(&mut kept, 7, 900).drawn;
        assert_eq!(kept_on, first + 1, "drawn in the frame before, so kept");
        end_frame();
        end_frame();
        let again = lines_of(&mut kept, 8, 901).drawn;
        assert_eq!(again, first + 3, "closed for a frame, so asked again");
    }

    #[test]
    fn a_panel_a_tooltip_and_its_compare_view_stay_kept_frame_after_frame() {
        let items = [(7, 900), (8, 901), (9, 902)];
        let mut kept: [Kept; KEPT] = Default::default();
        for _frame in 0..3 {
            for (item, pide) in items {
                let _drawn = lines_of(&mut kept, item, pide);
            }
            end_frame();
        }
        for (item, pide) in items {
            assert!(
                kept.iter().any(|k| k.item == item && k.pide == pide),
                "item {item} kept"
            );
        }
        let _fifth = lines_of(&mut kept, 10, 903);
        let _sixth = lines_of(&mut kept, 11, 904);
        assert!(
            kept.iter().any(|k| k.item == 11) && kept.iter().any(|k| k.item == 10),
            "the entries drawn longest ago gave way"
        );
    }

    #[test]
    fn a_site_reads_nothing_while_the_interface_frame_does_not_run() {
        let _first = LINKED.set(Linked {
            layout: 0,
            unknown: 0x10,
            item: 8,
            pide: 0xc,
        });
        // Layout 1, a plain tooltip, of item 7 with entity 900.
        let desc: [u32; 6] = [1, 0, 7, 900, 0, 0];
        let at = core::ptr::from_ref(&desc).expose_provenance() as u64;
        // SAFETY: `desc` lives on this stack for the call and holds every
        // field `Linked` names.
        unsafe { site(at, Site::Plain) };
        assert_eq!(FIRST_HIT.get(), [false; 2], "no hit read, nothing kept");
    }

    #[test]
    fn a_tooltip_line_wraps_and_centres_as_its_label_asks() {
        let owner = ModIndex::new(0);
        GATHERED.with_borrow_mut(Vec::clear);
        GATHERING.set(Some(Gathering {
            token: 9,
            owner,
            count: 0,
        }));
        for flags in [0, UiWidget::WRAP | UiWidget::CENTER, UiWidget::CENTER] {
            let label = enshwick_abi::headed!(UiWidget {
                kind: UiWidget::LABEL,
                flags,
                text: enshwick_core::abi::Str::new("Legendary"),
                ..UiWidget::EMPTY
            });
            assert_eq!(gather(owner, 9, &label), Some(Ok(())));
        }
        GATHERING.set(None);
        let shown: Vec<_> = GATHERED.take().iter().map(Line::wrap_center).collect();
        assert_eq!(shown, [(false, false), (true, true), (false, true)]);
    }

    #[test]
    fn a_handler_s_lines_are_labels_up_to_the_most_and_only_its_own() {
        let owner = ModIndex::new(0);
        let label = |text: &'static str| {
            enshwick_abi::headed!(UiWidget {
                kind: UiWidget::LABEL,
                text: enshwick_core::abi::Str::new(text),
                ..UiWidget::EMPTY
            })
        };
        assert!(gather(owner, 5, &label("x")).is_none(), "nothing gathers");
        GATHERED.with_borrow_mut(Vec::clear);
        GATHERING.set(Some(Gathering {
            token: 5,
            owner,
            count: 0,
        }));
        assert!(gather(owner, 6, &label("x")).is_none(), "another token");
        let button = enshwick_abi::headed!(UiWidget {
            kind: UiWidget::BUTTON,
            ..label("b")
        });
        assert_eq!(
            gather(owner, 5, &button),
            Some(Err(WidgetError::TooltipRefused))
        );
        for _ in 0..UiTooltipItem::MAX_LINES {
            assert_eq!(gather(owner, 5, &label("line")), Some(Ok(())));
        }
        assert_eq!(
            gather(owner, 5, &label("one more")),
            Some(Err(WidgetError::TooltipFull))
        );
        assert_eq!(GATHERED.take().len(), UiTooltipItem::MAX_LINES as usize);
        GATHERING.set(None);
    }
}
