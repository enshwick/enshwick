//! The mod's interface: what it draws, its markers, dialogs and the player's input.

use enshwick_core::abi::Header;

mod sound;

use super::table;
use super::{FakeHost, Installed};
#[cfg(feature = "unstable")]
pub use sound::FakeSound;

/// What the fake's interface answers, as the test set it.
#[derive(Clone, Debug, Default)]
pub(crate) struct UiPresets {
    pub(crate) fog: Option<(enshwick_core::abi::FogInfo, Vec<u8>)>,
    pub(crate) map_marker_types: Option<Vec<enshwick_core::abi::MapMarkerTypeInfo>>,
}

/// The prompt and the screen's host the fake's frames hear, as a test set
/// them.
#[derive(Clone, Debug, Default)]
struct Reads {
    title: String,
    verb: u32,
    host: u32,
    host_read: Option<(enshwick_core::content::Guid, [i64; 3])>,
    hovered: Option<(u64, enshwick_core::ItemId, u32, enshwick_core::Entity)>,
}

/// An interface step before a draw, as the game's `player_ui` runs before
/// the interface draws: the held and snapshot reads a mod started read from
/// now on.
fn step() {
    table::with((), |s| {
        s.ui.local_held.stepped |= s.ui.local_held.asked;
        s.ui.snapshot_reads.1 |= s.ui.snapshot_reads.0;
        s.actions.bindings.step();
    });
    sound::frame();
}

fn reads() -> Reads {
    table::with(Reads::default(), |s| Reads {
        title: s
            .ui
            .prompt
            .as_ref()
            .map_or_else(String::new, |(t, _)| t.clone()),
        verb: s.ui.prompt.as_ref().map_or(0, |&(_, v)| v),
        host: s.ui.screen_host,
        host_read: s.ui.screen_host_read.filter(|_| s.ui.screen_host != 0),
        hovered: s.ui.hovered,
    })
}

/// An in-game frame `token` with `open` the screen and `reads` its prompt
/// and host, which it borrows. The hovered slot shows only while a screen is
/// open, since the loader notes it from the slots the game's screens draw.
fn in_game(token: u64, open: &str, reads: &Reads) -> enshwick_core::abi::UiFrame {
    let hovered = reads.hovered.filter(|_| !open.is_empty());
    enshwick_core::abi::UiFrame {
        header: Header::of::<enshwick_core::abi::UiFrame>(),
        token,
        screen: enshwick_core::abi::UiFrame::IN_GAME,
        reserved: 0,
        hud_screen: enshwick_core::abi::Str::new(open),
        prompt_title: enshwick_core::abi::Str::new(&reads.title),
        prompt_verb: reads.verb,
        screen_host: enshwick_core::Entity(reads.host),
        screen_host_template: reads
            .host_read
            .map_or(enshwick_core::content::Guid::NIL, |(t, _)| t),
        screen_host_position: reads
            .host_read
            .map_or([0.0; 3], |(_, p)| enshwick_core::ecs::metres(p)),
        hovered_slot: hovered.map_or(0, |(slot, ..)| slot),
        hovered_item: hovered.map_or(enshwick_core::ItemId::new(0), |(_, item, ..)| item),
        hovered_count: hovered.map_or(0, |(_, _, count, _)| count),
        hovered_pide: hovered.map_or(enshwick_core::Entity(0), |(.., pide)| pide),
        reserved_hovered: 0,
    }
}

/// The text the fake keys an item slot's inputs by, the slot showing
/// `item`.
pub(crate) fn slot_key(item: enshwick_core::ItemId) -> String {
    format!("item slot {}", item.get())
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Runs the mod's frame callbacks once, in game, and returns what they
    /// drew, in order; inputs given with [`Installed::input`] reach their
    /// widgets in this frame.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    #[must_use]
    pub fn draw_frame(&self) -> Vec<Drawn> {
        step();
        let (frames, open) = table::with((Vec::new(), ""), |s| {
            (s.ui.frames.clone(), s.ui.open_screen)
        });
        let mut drawn = Vec::new();
        for (token, handler) in (1u64..).zip(frames) {
            table::with((), |s| {
                s.ui.frame = Some((token, false, Vec::new()));
                s.ui.drawing = handler.owner();
            });
            let reads = reads();
            let frame = in_game(token, open, &reads);
            // SAFETY: a callback the mod registered with its own pointer.
            handler.run(|callback, user| unsafe { callback(&raw const frame, user) });
            let list = table::with(None, |s| {
                s.ui.drawing = None;
                s.ui.frame.take()
            });
            if let Some((_, _, list)) = list {
                drawn.extend(list);
            }
        }
        // The loader draws its dialog after the windows, so Back taken under
        // it closes it without an answer in the same frame.
        if table::with(false, |s| core::mem::take(&mut s.ui.dialog_backed)) {
            self.answer(None);
        }
        drawn
    }

    /// Runs the mod's HUD callbacks once and returns what they drew, each
    /// item with its anchor (`UiWidget::TOP_LEFT` and on), in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_hud().is_empty());
    /// ```
    #[must_use]
    pub fn draw_hud(&self) -> Vec<(u32, Drawn)> {
        step();
        let (huds, open) = table::with((Vec::new(), ""), |s| (s.ui.huds.clone(), s.ui.open_screen));
        let mut drawn = Vec::new();
        for (token, handler) in (1u64..).zip(huds) {
            table::with((), |s| {
                s.ui.hud = Some((token, Vec::new()));
                s.ui.drawing = handler.owner();
            });
            let reads = reads();
            let frame = in_game(token, open, &reads);
            // SAFETY: a callback the mod registered with its own pointer.
            handler.run(|callback, user| unsafe { callback(&raw const frame, user) });
            let list = table::with(None, |s| {
                s.ui.drawing = None;
                s.ui.hud.take()
            });
            if let Some((_, list)) = list {
                drawn.extend(list);
            }
        }
        drawn
    }

    /// Runs the callbacks of the mod's extensions of the game screen
    /// `screen` once, as when the player has that screen open, and returns
    /// what each drew, with its edge (`UiExtension::LEFT` and on), in the
    /// loader's order: edge by edge, within an edge place by place as each
    /// place was first registered, and within a place in registration
    /// order. The loader opens each place's window for its panels, so a
    /// window widget is refused there, and a panel that drew nothing is left
    /// out, as it shows no window. [`Self::screen_placements`] says
    /// where each panel sits.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    /// use enshwick::ui::HudScreenType;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.draw_screen(HudScreenType::StorageChest).is_empty());
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn draw_screen(&self, screen: enshwick_core::ui::HudScreenType) -> Vec<(u32, Vec<Drawn>)> {
        step();
        let mut extensions = table::with(Vec::new(), |s| s.ui.extensions.clone());
        let first: Vec<_> = extensions
            .iter()
            .map(|(_, place, ..)| extensions.iter().position(|e| e.1 == *place))
            .collect();
        let mut ranked: Vec<_> = first.into_iter().zip(extensions.drain(..)).collect();
        ranked.sort_by_key(|(first, (_, place, ..))| (place.edge, *first));
        let open = screen.name();
        let template = reads().host_read.map(|(t, _)| t);
        let mut drawn = Vec::new();
        for (token, (_, (name, place, handler, hosts))) in (1u64..).zip(ranked) {
            let edge = place.edge;
            let for_host = hosts.is_empty() || template.is_some_and(|t| hosts.contains(&t));
            if name != screen.name() || !for_host {
                continue;
            }
            table::with((), |s| {
                s.ui.frame = Some((token, true, Vec::new()));
                s.ui.extending = Some(token);
                s.ui.drawing = handler.owner();
            });
            let reads = reads();
            let frame = in_game(token, open, &reads);
            // SAFETY: a callback the mod registered with its own pointer.
            handler.run(|callback, user| unsafe { callback(&raw const frame, user) });
            let list = table::with(None, |s| {
                s.ui.extending = None;
                s.ui.drawing = None;
                s.ui.frame.take()
            });
            if let Some((_, _, list)) = list.filter(|(_, _, list)| !list.is_empty()) {
                drawn.push((edge, list));
            }
        }
        drawn
    }

    /// Where the mod's panels of the game screen `screen` sit, in
    /// registration order, as the loader places them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    /// use enshwick::ui::HudScreenType;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// assert!(loader.screen_placements(HudScreenType::Backpack).is_empty());
    /// ```
    #[cfg(feature = "unstable")]
    #[must_use]
    pub fn screen_placements(
        &self,
        screen: enshwick_core::ui::HudScreenType,
    ) -> Vec<enshwick_core::ui::PanelPlacement> {
        table::with(Vec::new(), |s| {
            s.ui.extensions
                .iter()
                .filter(|(name, ..)| name == screen.name())
                .filter_map(|&(_, place, ..)| {
                    enshwick_core::__fake::placement(
                        place.edge,
                        place.align,
                        place.shift,
                        place.inset,
                    )
                })
                .collect()
        })
    }

    /// Opens the game screen `screen` for the frames and HUDs drawn from
    /// now on, as when the player opens it; `None` closes it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    /// use enshwick::ui::HudScreenType;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.open_screen(Some(HudScreenType::Map));
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    #[cfg(feature = "unstable")]
    pub fn open_screen(&self, screen: Option<enshwick_core::ui::HudScreenType>) {
        table::with((), |s| {
            s.ui.open_screen = screen.map_or("", enshwick_core::ui::HudScreenType::name);
        });
    }

    /// Shows the interaction prompt `title` with the verb `LocaTagId`
    /// `verb` in the frames drawn from now on, as when the player looks at
    /// an object; `None` shows none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.show_prompt(Some(("Chest", 7)));
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn show_prompt(&self, prompt: Option<(&str, u32)>) {
        table::with((), |s| {
            s.ui.prompt = prompt.map(|(title, verb)| (title.to_owned(), verb));
        });
    }

    /// Makes `host`, an entity of the client's world, the object the open
    /// screen belongs to in the frames drawn from now on; `None` for none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.host_screen(Some(Entity(512)));
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn host_screen(&self, host: Option<enshwick_core::Entity>) {
        table::with((), |s| {
            let host = host.map_or(0, |e| e.0);
            // As in game, a new host has no template until a step reads it.
            if s.ui.screen_host != host {
                s.ui.screen_host_read = None;
            }
            s.ui.screen_host = host;
        });
    }

    /// Makes the open screen's host one made from `template`, standing at
    /// `position` in metres, as the client's last step read it, for the
    /// frames and panels drawn from now on while a host is set with
    /// [`Installed::host_screen`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::abi::Side;
    /// use enshwick::content::Guid;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.host_screen(Some(Entity(512)));
    /// loader.read_screen_host(Guid::named("you.safes:safe"), [1.0, 2.0, 3.0]);
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn read_screen_host(&self, template: enshwick_core::content::Guid, position: [f64; 3]) {
        let fixed = enshwick_core::ecs::fixed(position);
        table::with((), |s| s.ui.screen_host_read = Some((template, fixed)));
    }

    /// Names the game action `action` on the fake build, its reflected
    /// name, with a key `bound` to it or none, for the input hints drawn
    /// from now on; an action never named is one the build lacks, which
    /// the loader refuses.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.game_action("ContextualAction", true);
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn game_action(&self, action: &str, bound: bool) {
        let first = if bound {
            enshwick_core::abi::GameBinding::OTHER
        } else {
            0
        };
        table::with((), |s| {
            crate::test::actions::set_player_keys(s, action, [first, 0]);
        });
    }

    /// Makes `held`, a `Bitmask64` of the game's `PlayerInputType`, the
    /// controls the local player holds from now on, as the client's last
    /// step read them; `None` as outside a game. As in the game, a mod's
    /// first ask, which opening `Input` makes, starts the reads, and they
    /// answer from the next draw on: a frame, the HUD or a screen.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.hold_locally(Some(1 << 30));
    /// ```
    pub fn hold_locally(&self, held: Option<u64>) {
        table::with((), |s| s.ui.local_held.bits = held);
    }

    /// Sets cached local sampler diagnostics without changing held input.
    pub fn set_local_input_status(&self, info: enshwick_core::abi::LocalInputInfo) {
        table::with((), |s| s.ui.local_input_info = info);
    }

    /// Sets the client snapshot the loader would write, as one
    /// [`ClientSnapshot`](enshwick_core::abi::ClientSnapshot) with its
    /// header set; a server still answers unavailable. As the loader, the
    /// first ask, which opening `Input` makes, starts the reads, and the
    /// snapshot answers from the next draw on: before it, it is empty, its
    /// state never read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ClientSnapshot, Side, Str};
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.set_client_snapshot(ClientSnapshot {
    ///     state: ClientSnapshot::FRESH,
    ///     available: ClientSnapshot::SCREENS,
    ///     session: 1,
    ///     age_ms: 16,
    ///     hud_screen: Str::new("Backpack"),
    ///     ..ClientSnapshot::EMPTY
    /// });
    /// ```
    pub fn set_client_snapshot(&self, snapshot: enshwick_core::abi::ClientSnapshot) {
        table::with((), |s| s.ui.client_snapshot = snapshot);
    }

    /// Sets the items of the carried windows the client holds, as the
    /// loader lists them, each a
    /// [`ClientWindowItem`](enshwick_core::abi::ClientWindowItem) whose
    /// strings live as long as the test; a server still answers
    /// unavailable.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ClientWindowItem, Side, Str};
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.set_client_windows(vec![ClientWindowItem {
    ///     kind: ClientWindowItem::BUTTON,
    ///     flags: ClientWindowItem::SHOWN,
    ///     window: Str::new("you.admin:panel"),
    ///     item: Str::new("@close"),
    ///     label: Str::new("Close"),
    ///     ..ClientWindowItem::EMPTY
    /// }]);
    /// ```
    pub fn set_client_windows(&self, items: Vec<enshwick_core::abi::ClientWindowItem>) {
        table::with((), |s| s.ui.client_windows = items);
    }

    /// The client commands mods queued, in order: each id, raw kind (such
    /// as [`ClientCommand::OPEN_SCREEN`](enshwick_core::abi::ClientCommand))
    /// and target.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.client_commands().is_empty());
    /// ```
    #[must_use]
    pub fn client_commands(&self) -> Vec<(u64, u32, String)> {
        table::with(Vec::new(), |s| {
            s.ui.client_commands
                .iter()
                .map(|(id, kind, target, _)| (*id, *kind, target.clone()))
                .collect()
        })
    }

    /// Moves client command `id` to `state` for `reason`, as the loader's
    /// frame would ([`ClientCommandStatus`](enshwick_core::abi::ClientCommandStatus)
    /// constants).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ClientCommandStatus;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_command_status(1, ClientCommandStatus::OBSERVED, 0);
    /// ```
    pub fn set_command_status(&self, id: u64, state: u32, reason: u32) {
        table::with((), |s| {
            if let Some((.., status)) =
                s.ui.client_commands
                    .iter_mut()
                    .find(|(queued, ..)| *queued == id)
            {
                status.state = state;
                status.reason = reason;
            }
        });
    }

    /// Moves join or leave `id` to `phase`, and a failed join to `failure`,
    /// as the loader's frame would ([`ClientCommandStatus`](enshwick_core::abi::ClientCommandStatus)
    /// constants); its state stays as [`Installed::set_command_status`] set it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ClientCommandStatus;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_command_phase(1, ClientCommandStatus::PHASE_LOADING, 0);
    /// ```
    pub fn set_command_phase(&self, id: u64, phase: u32, failure: u32) {
        table::with((), |s| {
            if let Some((.., status)) =
                s.ui.client_commands
                    .iter_mut()
                    .find(|(queued, ..)| *queued == id)
            {
                status.phase = phase;
                status.failure = failure;
            }
        });
    }

    /// The mod's markers on the map, as it set them last.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.markers().is_empty());
    /// ```
    #[must_use]
    pub fn markers(&self) -> Vec<FakeMarker> {
        table::with(Vec::new(), |s| s.ui.markers.clone())
    }

    /// Gives the widget whose text or label is `text` an input from the
    /// player in the next frame drawn.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::test::Input;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.input("Close", Input::Click);
    /// ```
    pub fn input(&self, text: &str, input: Input) {
        table::with((), |s| s.ui.inputs.push((text.to_owned(), input)));
    }

    /// Gives the item slot showing `item` an input from the player in the
    /// next frame drawn: [`Input::Click`], which also points at it, or
    /// [`Input::Hover`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::test::Input;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.slot_input(enshwick::ItemId::new(0x5e01), Input::Hover);
    /// ```
    pub fn slot_input(&self, item: enshwick_core::ItemId, input: Input) {
        table::with((), |s| s.ui.inputs.push((slot_key(item), input)));
    }

    /// Points the cursor at a slot of the game's open inventory screen in
    /// the frames drawn from now on: the slot, the item it holds, how many
    /// and the item's own entity, entity 0 for none; `None` for no slot.
    /// Frames read it only while a game screen is open.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::inventory::SlotRef;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let slot = SlotRef::new(Entity(41), 3);
    /// loader.hover_slot(Some((slot.0, enshwick::ItemId::new(0x5e01), 2, Entity(0))));
    /// assert!(loader.draw_frame().is_empty());
    /// ```
    pub fn hover_slot(
        &self,
        slot: Option<(u64, enshwick_core::ItemId, u32, enshwick_core::Entity)>,
    ) {
        table::with((), |s| s.ui.hovered = slot);
    }

    /// Opens the tooltip of the item `item` with its own entity `pide`, as
    /// when the player points at it, and returns the lines each of the mod's
    /// tooltip callbacks added, in registration order: the loader calls
    /// them once per item and keeps their lines while the tooltip shows. An
    /// item with no entity of its own (`pide` 0, such as a stack of
    /// materials) gets no lines, as in the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// let lines = loader.open_tooltip(enshwick::ItemId::new(0x5e01), enshwick::Entity(900));
    /// assert!(lines.is_empty());
    /// ```
    #[must_use]
    pub fn open_tooltip(
        &self,
        item: enshwick_core::ItemId,
        pide: enshwick_core::Entity,
    ) -> Vec<Drawn> {
        if pide == enshwick_core::Entity(0) {
            return Vec::new();
        }
        let tooltips = table::with(Vec::new(), |s| s.ui.tooltips.clone());
        let mut lines = Vec::new();
        for (token, handler) in (1u64..).zip(tooltips) {
            table::with((), |s| {
                s.ui.tooltip = Some((token, Vec::new()));
                s.ui.drawing = handler.owner();
            });
            let opened = enshwick_core::abi::UiTooltipItem {
                header: Header::of::<enshwick_core::abi::UiTooltipItem>(),
                token,
                item,
                pide,
            };
            // SAFETY: a callback the mod registered with its own pointer.
            handler.run(|callback, user| unsafe { callback(&raw const opened, user) });
            let added = table::with(None, |s| {
                s.ui.drawing = None;
                s.ui.tooltip.take()
            });
            if let Some((_, added)) = added {
                lines.extend(added);
            }
        }
        lines
    }

    /// The dialogs waiting for an answer, oldest first.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.dialogs().is_empty());
    /// ```
    #[must_use]
    pub fn dialogs(&self) -> Vec<ShownDialog> {
        table::with(Vec::new(), |s| {
            s.ui.dialogs.iter().map(|(d, _)| d.clone()).collect()
        })
    }

    /// Answers the oldest waiting dialog with `answer`, `None` for closed
    /// without one; false when none waits.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::Response;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.answer(Some(Response::Ok)));
    /// ```
    pub fn answer(&self, answer: Option<enshwick_core::ui::Response>) -> bool {
        let first = table::with(None, |s| {
            (!s.ui.dialogs.is_empty()).then(|| s.ui.dialogs.remove(0))
        });
        let Some((_, answered)) = first else {
            return false;
        };
        let raw = answer.map_or(
            enshwick_core::abi::UiDialog::CLOSED,
            enshwick_core::ui::Response::to_raw,
        );
        // SAFETY: the mod's answer callback with its own pointer, once.
        answered.run(|callback, user| unsafe { callback(raw, user) });
        true
    }
}

/// A widget a frame callback drew, as [`Installed::draw_frame`] reports it.
///
/// # Examples
///
/// ```
/// use enshwick::test::Drawn;
///
/// let drawn = Drawn::Button { text: "Close".into(), enabled: true };
/// assert!(matches!(drawn, Drawn::Button { .. }));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Drawn {
    /// A window started.
    Window {
        /// Its id.
        id: String,
        /// Its title.
        title: String,
    },
    /// The window ended.
    End,
    /// A label.
    Label {
        /// Its text.
        text: String,
        /// Its style, `UiWidget::STYLE_*`.
        style: u32,
    },
    /// A button.
    Button {
        /// Its text.
        text: String,
        /// Whether it takes clicks.
        enabled: bool,
    },
    /// A toggle row.
    Toggle {
        /// Its label.
        label: String,
        /// Its state after the frame.
        on: bool,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A slider row.
    Slider {
        /// Its label.
        label: String,
        /// Its value after the frame.
        value: f32,
        /// Its least value.
        min: f32,
        /// Its greatest value.
        max: f32,
        /// Its stops, 0 for continuous.
        steps: u32,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A dropdown row.
    Dropdown {
        /// Its label.
        label: String,
        /// Its options.
        options: Vec<String>,
        /// Its option after the frame.
        index: usize,
        /// Whether it takes changes.
        read_only: bool,
    },
    /// A progress bar.
    Progress {
        /// How full, from 0 to 1.
        fraction: f32,
    },
    /// A sound, `UiWidget::SOUND_*`.
    Sound(u32),
    /// A sound container played with
    /// [`Frame::play`](enshwick_core::ui::Frame::play), by its GUID.
    Play(enshwick_core::content::Guid),
    /// The key hint of a game action.
    InputHint {
        /// The action's reflected name.
        action: String,
        /// The label beside the key, empty for none.
        label: String,
        /// Whether a key is bound to the action, so the game drew one.
        bound: bool,
    },
    /// An item slot in the game's style.
    ItemSlot {
        /// The item it shows.
        item: enshwick_core::ItemId,
        /// How many it shows.
        count: u32,
    },
}

/// A marker the mod set on the map, as [`Installed::markers`] reports it.
///
/// # Examples
///
/// See [`Installed::markers`].
#[derive(Clone, Debug, PartialEq)]
pub struct FakeMarker {
    /// Where, in metres.
    pub position: [f64; 3],
    /// The label.
    pub label: String,
    /// The colour, `UiMarker::WHITE` and on.
    pub color: u32,
    /// The registry key of its marker type; 0 for the loader's choice.
    pub marker_type: u32,
}

/// What the player does to a widget, given with [`Installed::input`].
///
/// # Examples
///
/// See [`Installed::input`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Input {
    /// Clicks a button.
    Click,
    /// Switches a toggle row.
    Toggle(bool),
    /// Moves a slider row.
    Value(f32),
    /// Picks a dropdown row's option.
    Pick(usize),
    /// Presses Back (Escape, B) over a window, given by its id.
    Back,
    /// Points the cursor at an item slot, given with
    /// [`Installed::slot_input`].
    Hover,
}

/// A dialog the mod queued, as [`Installed::dialogs`] reports it.
///
/// # Examples
///
/// See [`Installed::dialogs`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownDialog {
    /// The title.
    pub title: String,
    /// The message.
    pub message: String,
    /// The detail.
    pub detail: String,
    /// The buttons, bit `i` for answer `i`.
    pub buttons: u32,
}

impl FakeHost {
    /// The client's marker type registry, as the loader reads it from the
    /// game's archive for `Ui::map_marker_types`; without one the registry
    /// does not read, as on a dedicated server.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{MapMarkerTypeInfo, Side};
    ///
    /// let altar = MapMarkerTypeInfo { marker_id: 0x0a0b_0c0d, ..MapMarkerTypeInfo::EMPTY };
    /// let loader = enshwick::test::FakeHost::new()
    ///     .side(Side::CLIENT)
    ///     .map_marker_types(vec![altar])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn map_marker_types(mut self, types: Vec<enshwick_core::abi::MapMarkerTypeInfo>) -> Self {
        self.ui.map_marker_types = Some(types);
        self
    }

    /// The client's fog of war as the loader keeps it: `info` and its
    /// `info.width` × `info.height` pixels, row by row.
    ///
    /// # Panics
    ///
    /// When `pixels` does not hold the image `info` describes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{FogInfo, Side};
    ///
    /// let info = FogInfo { width: 2, height: 2, metres_per_pixel: 8.0, size_z: 16.0, ..FogInfo::EMPTY };
    /// let loader = enshwick::test::FakeHost::new()
    ///     .side(Side::CLIENT)
    ///     .fog(info, vec![0, 255, 0, 0])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn fog(mut self, info: enshwick_core::abi::FogInfo, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            info.width as usize * info.height as usize,
            "the pixels of the image the info describes"
        );
        self.ui.fog = Some((info, pixels));
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Replaces the fog of war, as the loader does after a reveal.
    ///
    /// # Panics
    ///
    /// As [`FakeHost::fog`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{FogInfo, Side};
    ///
    /// let info = FogInfo { width: 1, height: 1, ..FogInfo::EMPTY };
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).install();
    /// loader.set_fog(info, vec![255]);
    /// ```
    pub fn set_fog(&self, info: enshwick_core::abi::FogInfo, pixels: Vec<u8>) {
        assert_eq!(pixels.len(), info.width as usize * info.height as usize);
        table::with((), |s| s.host.ui.fog = Some((info, pixels)));
    }
}
