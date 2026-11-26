//! Client commands as Luau gives them, read into [`ClientCommand`].

use super::{ClientCommand, CommandAction, Password};
use crate::controls::GameAction;
use crate::{Entity, RecipeId};
use enshwick_base::ui::Response;
use enshwick_inventory::inventory::SlotRef;

/// [`ClientCommand`] as Luau gives it; its `Debug` shows none of the
/// password.
#[derive(Clone, PartialEq, crate::Script)]
#[script(name = "ClientCommand")]
pub(super) struct LuauCommand {
    /// `"open_screen"`, `"close_screen"`, `"choose_tab"`, `"look"`,
    /// `"move"`, `"highlight_recipe"`, `"join"`, `"leave"`, `"press"`,
    /// `"set_waypoint"`, `"clear_waypoint"`, `"ping"`, `"host"`,
    /// `"open_mods"`, `"open_mod"`, `"open_mod_settings"`,
    /// `"press_window_item"`, `"pick_window_item"`, `"set_window_text"` or
    /// `"answer_dialog"`, `"craft"`, `"move_item"`, `"equip"`, `"take_all"`,
    /// `"deposit_all"` or `"trash"`.
    pub kind: String,
    /// The session, as `client_snapshot().session` gives it.
    pub session: u64,
    /// The HUD screen the client must show.
    pub expected_screen: Option<String>,
    /// The screen to open, the tab to choose, the server to join or the
    /// mod whose page to open, by its id.
    pub target: Option<String>,
    /// A look's yaw and pitch in radians.
    pub look: Option<[f32; 2]>,
    /// A move's direction.
    pub direction: Option<[f32; 2]>,
    /// A move's or a press's hold in milliseconds.
    pub hold_ms: Option<u32>,
    /// The recipe to highlight.
    pub recipe: Option<RecipeId>,
    /// A join's server password.
    pub password: Option<String>,
    /// The game action a press holds, by its game name, as a control's
    /// `link` names it.
    pub action: Option<String>,
    /// The world position, x, y and z in metres, a waypoint or a ping
    /// marks.
    pub position: Option<[f64; 3]>,
    /// A host's world, by its place in the main menu's list from 0.
    pub world: Option<u16>,
    /// A host's character, by its place in the main menu's list from 0.
    pub character: Option<u16>,
    /// Whether a host opens its world online; offline without it.
    pub online: Option<bool>,
    /// A window command's window, as `mod:window`.
    pub window: Option<String>,
    /// A window command's item, by its id in the window.
    pub item: Option<String>,
    /// The option a pick picks or the text a window's field takes.
    pub text: Option<String>,
    /// The response a dialog's answer gives.
    pub answer: Option<Response>,
    /// The dialog an answer answers, as `client_snapshot().dialog` numbers
    /// it.
    pub dialog: Option<u32>,
    /// A craft's count, or the items a move moves; a move without one
    /// moves the whole stack.
    pub count: Option<u16>,
    /// A craft's station, by hand without one; the inventory a take all
    /// takes from or a deposit all fills.
    pub entity: Option<Entity>,
    /// The slot a move leaves, an equip takes from or a trash destroys.
    pub slot: Option<SlotRef>,
    /// The slot a move fills.
    pub target_slot: Option<SlotRef>,
    /// The equipment slot an equip fills, by the game's `EquipmentSlot`
    /// member.
    pub equipment: Option<String>,
}

impl core::fmt::Debug for LuauCommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LuauCommand")
            .field("kind", &self.kind)
            .field("session", &self.session)
            .field("expected_screen", &self.expected_screen)
            .field("target", &self.target)
            .field("look", &self.look)
            .field("direction", &self.direction)
            .field("hold_ms", &self.hold_ms)
            .field("recipe", &self.recipe)
            .field("password", &self.password.as_ref().map(|_| "<secret>"))
            .field("action", &self.action)
            .field("position", &self.position)
            .field("world", &self.world)
            .field("character", &self.character)
            .field("online", &self.online)
            .field("window", &self.window)
            .field("item", &self.item)
            .field("text", &self.text)
            .field("answer", &self.answer)
            .field("dialog", &self.dialog)
            .field("count", &self.count)
            .field("entity", &self.entity)
            .field("slot", &self.slot)
            .field("target_slot", &self.target_slot)
            .field("equipment", &self.equipment)
            .finish()
    }
}

impl TryFrom<LuauCommand> for ClientCommand {
    type Error = crate::script::Error;

    fn try_from(luau: LuauCommand) -> Result<Self, Self::Error> {
        let missing =
            |what: &str| crate::script::Error::new(format!("a {} command needs {what}", luau.kind));
        let action = match luau.kind.as_str() {
            "open_screen" => {
                CommandAction::OpenScreen(luau.target.clone().ok_or_else(|| missing("a target"))?)
            }
            "close_screen" => CommandAction::CloseScreen,
            "choose_tab" => {
                CommandAction::ChooseTab(luau.target.clone().ok_or_else(|| missing("a target"))?)
            }
            "look" => {
                let [yaw, pitch] = luau.look.ok_or_else(|| missing("a look"))?;
                CommandAction::Look { yaw, pitch }
            }
            "move" => CommandAction::Move {
                direction: luau.direction.ok_or_else(|| missing("a direction"))?,
                hold_ms: luau.hold_ms.ok_or_else(|| missing("a hold"))?,
            },
            "highlight_recipe" => {
                CommandAction::HighlightRecipe(luau.recipe.ok_or_else(|| missing("a recipe"))?)
            }
            "join" => CommandAction::Join {
                server: luau.target.clone().ok_or_else(|| missing("a target"))?,
                password: luau.password.clone().map(Password),
            },
            "leave" => CommandAction::Leave,
            "press" => {
                let name = luau.action.as_deref().ok_or_else(|| missing("an action"))?;
                let action = GameAction::ALL
                    .iter()
                    .copied()
                    .find(|action| action.name() == name)
                    .ok_or_else(|| crate::script::Error::new(format!("no game action {name:?}")))?;
                CommandAction::Press {
                    action,
                    hold_ms: luau.hold_ms.ok_or_else(|| missing("a hold"))?,
                }
            }
            "set_waypoint" => {
                CommandAction::SetWaypoint(luau.position.ok_or_else(|| missing("a position"))?)
            }
            "clear_waypoint" => CommandAction::ClearWaypoint,
            "ping" => CommandAction::Ping(luau.position.ok_or_else(|| missing("a position"))?),
            "open_mods" => CommandAction::OpenMods,
            "open_mod" => {
                CommandAction::OpenMod(luau.target.clone().ok_or_else(|| missing("a target"))?)
            }
            "open_mod_settings" => CommandAction::OpenModSettings(
                luau.target.clone().ok_or_else(|| missing("a target"))?,
            ),
            "host" => CommandAction::Host {
                world: luau.world.ok_or_else(|| missing("a world"))?,
                character: luau.character.ok_or_else(|| missing("a character"))?,
                online: luau.online.unwrap_or(false),
            },
            "press_window_item" => CommandAction::PressWindowItem {
                window: luau.window.clone().ok_or_else(|| missing("a window"))?,
                item: luau.item.clone().ok_or_else(|| missing("an item"))?,
            },
            "pick_window_item" => CommandAction::PickWindowItem {
                window: luau.window.clone().ok_or_else(|| missing("a window"))?,
                item: luau.item.clone().ok_or_else(|| missing("an item"))?,
                option: luau.text.clone().ok_or_else(|| missing("a text"))?,
            },
            "set_window_text" => CommandAction::SetWindowText {
                window: luau.window.clone().ok_or_else(|| missing("a window"))?,
                item: luau.item.clone().ok_or_else(|| missing("an item"))?,
                text: luau.text.clone().ok_or_else(|| missing("a text"))?,
            },
            "answer_dialog" => CommandAction::AnswerDialog {
                dialog: luau.dialog.ok_or_else(|| missing("a dialog"))?,
                response: luau.answer.ok_or_else(|| missing("an answer"))?,
            },
            "craft" | "move_item" | "equip" | "take_all" | "deposit_all" | "trash" => {
                item_action(&luau)?
            }
            other => {
                return Err(crate::script::Error::new(format!(
                    "no client command kind {other:?}"
                )));
            }
        };
        Ok(Self {
            action,
            session: luau.session,
            expected_screen: luau.expected_screen,
        })
    }
}

impl crate::script::Typed for ClientCommand {
    const TY: &'static crate::script::Ty = <LuauCommand as crate::script::Typed>::TY;
}

impl crate::script::FromScript for ClientCommand {
    fn from_value(value: crate::script::Value) -> Result<Self, crate::script::Error> {
        LuauCommand::from_value(value)?.try_into()
    }
}

/// The item command `luau` gives, by its kind.
fn item_action(luau: &LuauCommand) -> Result<CommandAction, crate::script::Error> {
    let missing =
        |what: &str| crate::script::Error::new(format!("a {} command needs {what}", luau.kind));
    Ok(match luau.kind.as_str() {
        "craft" => CommandAction::Craft {
            recipe: luau.recipe.ok_or_else(|| missing("a recipe"))?,
            count: luau.count.ok_or_else(|| missing("a count"))?,
            station: luau.entity,
        },
        "move_item" => CommandAction::MoveItem {
            from: luau.slot.ok_or_else(|| missing("a slot"))?,
            to: luau.target_slot.ok_or_else(|| missing("a target slot"))?,
            count: luau
                .count
                .map(|count| {
                    core::num::NonZeroU16::new(count).ok_or_else(|| missing("a count above 0"))
                })
                .transpose()?,
        },
        "equip" => CommandAction::Equip {
            slot: luau.slot.ok_or_else(|| missing("a slot"))?,
            equipment: luau
                .equipment
                .clone()
                .ok_or_else(|| missing("an equipment slot"))?,
        },
        "take_all" => CommandAction::TakeAll(luau.entity.ok_or_else(|| missing("an entity"))?),
        "deposit_all" => {
            CommandAction::DepositAll(luau.entity.ok_or_else(|| missing("an entity"))?)
        }
        "trash" => CommandAction::Trash(luau.slot.ok_or_else(|| missing("a slot"))?),
        other => {
            return Err(crate::script::Error::new(format!(
                "no item command kind {other:?}"
            )));
        }
    })
}
