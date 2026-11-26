//! Players' interactions with entities.
//!
//! A mod's say before the game opens a chest, moves or sorts its items, lends
//! its stock to crafting, lets a piece be dismantled, or flips its Quick Stash
//! or upgrade switch.
//!
//! The platform asks the filters in the server's world only, on the thread of
//! the system that checks the interaction, and turns a refusal into the
//! game's own refusal, so nothing moves and nothing is lost.
//!
//! # Examples
//!
//! ```
//! use enshwick::interactions::Kind;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     const LOCKED_CHEST: enshwick::ItemId = enshwick::ItemId::new(0x5e01);
//!     // Nobody opens or empties a locked chest.
//!     load.on_interaction(|i| !(i.item == Some(LOCKED_CHEST) && matches!(i.kind, Kind::Open | Kind::Move)))
//! }
//! ```

use core::ffi::c_void;

use crate::Off;
use crate::abi::Interaction as RawInteraction;
#[cfg(feature = "unstable")]
use crate::abi::Status;
#[cfg(feature = "unstable")]
use crate::content::EngineFacts as _;
use crate::{Entity, ItemId};
#[cfg(feature = "unstable")]
use crate::{Host, WorldError, ecs::ComponentType, engine_ids as ids, resources::Registry};
#[cfg(feature = "unstable")]
use enshwick_world::__internal::world_status;

/// The game's name of the target its prompt offers, among its `TargetType`.
#[cfg(feature = "unstable")]
const INTERACTION_TARGET: &str = "Interaction";

/// The players' prompt targets in the server's world, from
/// [`Load::interaction_targets`](../struct.Load.html#method.interaction_targets); `Copy`, for any thread, read on the game
/// thread.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let targets = load.interaction_targets()?;
///     let world = load.world()?;
///     load.on_tick(Duration::from_secs(1), move |_| {
///         for player in world.players() {
///             if let Ok(Some(target)) = targets.target(player.entity) {
///                 enshwick::info!("{} looks at {target:?}", player.name);
///             }
///         }
///     })?;
///     Ok(())
/// }
/// ```
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug)]
pub struct Targets(&'static TargetsAt);

#[cfg(feature = "unstable")]
impl crate::script::Bound for Targets {
    const NAME: &'static str = "interactions";
    const DOC: &'static str =
        "Players' interactions with entities, and what each player's prompt offers.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_TARGET];

    fn resolve(load: &mut impl crate::Loading) -> Result<Self, Off> {
        Targets::open(load)
    }
}

/// Where `SelectedTargets` keeps the entity of the prompt's target on the
/// running build.
#[cfg(feature = "unstable")]
#[derive(Debug)]
struct TargetsAt {
    host: Host,
    ty: ComponentType,
    entity_at: usize,
}

#[cfg(feature = "unstable")]
static TARGETS: crate::state::ModLocal<TargetsAt> = crate::state::ModLocal::new();

#[cfg(feature = "unstable")]
impl TargetsAt {
    fn resolve(load: &impl crate::Loading) -> Result<Self, Off> {
        let load = load.base();
        let ty = ComponentType::resolve(load, ids::SELECTED_TARGETS)?;
        let slot = Registry::game()?
            .hashed(load.hash(ids::TARGET_TYPE)?)
            .and_then(|kinds| {
                kinds
                    .constants()
                    .iter()
                    .find(|c| c.name() == INTERACTION_TARGET)
                    .map(crate::resources::Constant::value)
            })
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| Off::new("interactions: the type registry lacks the prompt's target"))?;
        let (targets, info, entity) = (
            load.offset(ids::SELECTED_TARGETS_TARGETS)?,
            load.size(ids::TARGET_INFO)?,
            load.offset(ids::TARGET_INFO_ENTITY)?,
        );
        let entity_at = slot
            .checked_mul(info)
            .and_then(|at| at.checked_add(targets))
            .and_then(|at| at.checked_add(entity));
        let size = load.size(ids::SELECTED_TARGETS)?;
        match entity_at {
            Some(at)
                if at
                    .checked_add(size_of::<u32>())
                    .is_some_and(|end| end <= size) =>
            {
                Ok(Self {
                    host: load.host(),
                    ty,
                    entity_at: at,
                })
            }
            _ => Err(Off::new(format!(
                "interactions: the prompt's target lies past SelectedTargets' {size} bytes"
            ))),
        }
    }
}

#[cfg(feature = "unstable")]
#[crate::script(target)]
impl Targets {
    pub(crate) fn open(load: &impl crate::Loading) -> Result<Self, Off> {
        if let Some(at) = TARGETS.get() {
            return Ok(Self(at));
        }
        let at = TargetsAt::resolve(load)?;
        Ok(Self(TARGETS.get_or_init(|| at)))
    }

    /// The entity whose interaction `player`'s prompt offers now; `None`
    /// when it offers none or for an entity that is no player. Read on the
    /// server's game thread between steps, in the server's world, so a
    /// vanilla client's player reads the same.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Unavailable`] before a world runs.
    ///
    /// # Examples
    ///
    /// See [`Targets`].
    #[script]
    pub fn target(self, player: Entity) -> Result<Option<Entity>, WorldError> {
        let host = self.0.host;
        world_status((host.api().world_quiet)(host.handle()))?;
        let mut out: *const c_void = core::ptr::null();
        let status = (host.api().component)(host.handle(), player, self.0.ty.hash(), &raw mut out);
        if status == Status::MISSING {
            return Ok(None);
        }
        world_status(status)?;
        if out.is_null() {
            return Ok(None);
        }
        // SAFETY: `out` is the player's live, non-null SelectedTargets, read
        // right after `world_quiet` answered that this is the game thread at
        // a quiet moment, so no system writes it now; `resolve` checked that
        // the u32 at `entity_at` lies inside the type's size.
        let entity = unsafe { crate::raw::read_at::<u32>(out.cast(), self.0.entity_at) };
        Ok((entity != 0).then_some(Entity::from_engine(entity)))
    }
}

/// Declares [`Verb`] with one row per verb: its name, the text the game
/// gives it in English and its `LocaTagId`, so the three never part.
#[cfg(feature = "unstable")]
macro_rules! verbs {
    ($($name:ident = $text:literal, $id:literal;)*) => {
        /// A verb the game shows on interaction prompts, named after its
        /// English text, for [`ObjectPrompt::verb`].
        ///
        /// The game's own data gives each one; a vanilla client shows it in
        /// the player's language.
        ///
        /// Unstable: needs the `unstable` feature and may change in any release.
        ///
        /// # Examples
        ///
        /// ```
        /// use enshwick::interactions::Verb;
        ///
        /// assert_eq!(Verb::from_id(Verb::Use.id()), Some(Verb::Use));
        /// ```
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
        #[script(name = "InteractionVerb")]
        #[non_exhaustive]
        pub enum Verb {
            $(
                #[doc = concat!("\"", $text, "\".")]
                $name,
            )*
        }

        impl Verb {
            /// Every verb with the game's English text of it, for the
            /// loader's check against the game's own texts.
            #[cfg(feature = "test")]
            #[doc(hidden)]
            pub const ALL: &[(Self, &str)] = &[$((Self::$name, $text)),*];

            /// The game's `LocaTagId` of the verb, as
            /// `ui::Prompt`'s `verb` holds it.
            ///
            /// # Examples
            ///
            /// See [`Verb`].
            #[must_use]
            pub const fn id(self) -> u32 {
                match self {
                    $(Self::$name => $id,)*
                }
            }

            /// The verb whose `LocaTagId` is `id`; `None` for an id no verb
            /// here has.
            ///
            /// # Examples
            ///
            /// See [`Verb`].
            #[must_use]
            pub const fn from_id(id: u32) -> Option<Self> {
                match id {
                    $($id => Some(Self::$name),)*
                    _ => None,
                }
            }
        }
    };
}

// The ids are the same on every game version from 1269579 to 1287369, on
// client and server: each is the hash of its LocaTag's GUID.
#[cfg(feature = "unstable")]
verbs! {
    Open = "Open", 0x4e82_e309;
    Close = "Close", 0x3959_a87f;
    Use = "Use", 0xc22a_47e8;
    Activate = "Activate", 0xc3d9_44f7;
    Deactivate = "Deactivate", 0x4ac7_1b17;
    Flip = "Flip", 0x65bb_af0f;
    Talk = "Talk", 0xd986_1c4b;
    Read = "Read", 0x6319_37dc;
    Inspect = "Inspect", 0x2607_98eb;
    Search = "Search", 0x9a59_9d0b;
    Loot = "Loot", 0x0af6_b5cb;
    Collect = "Collect", 0x9095_e8bd;
    Harvest = "Harvest", 0x5e09_f8d2;
    Pick = "Pick", 0x02e9_2eeb;
    Craft = "Craft", 0xe0a9_e31d;
    Cook = "Cook", 0x750a_1fe9;
    Sit = "Sit", 0xdfb7_e778;
    Sleep = "Sleep", 0xc4e0_f1cb;
    Enter = "Enter", 0x785f_f179;
    Edit = "Edit", 0xfc75_ab30;
    Recover = "Recover", 0x2399_de44;
    RepairEquipment = "Repair Equipment", 0xf9c7_6d99;
    DepositItems = "Deposit Items", 0xe8ed_8099;
    DrinkFrom = "Drink from", 0xc2f7_9d24;
    Upgrade = "Upgrade", 0xa13e_994a;
    Disarm = "Disarm", 0x3044_8d91;
    Pet = "Pet", 0x6d5b_6297;
    Chop = "Chop", 0x2d38_daea;
    Cleanse = "Cleanse", 0xaf97_08ac;
    Adopt = "Adopt", 0xe57a_9226;
    PickLock = "Pick Lock", 0xdd3c_0694;
    Repot = "Repot", 0x85f7_005e;
    TearOut = "Tear out", 0x2cbe_e107;
    PlaceInto = "Place Into", 0xf111_b8dd;
}

/// What the prompt of every object placed from one item shows and does, for
/// [`Load::prompt`](../struct.Load.html#method.prompt).
///
/// The game verb and the replaced action are the server's, so every client
/// sees them, with or without Enshwick; a client with Enshwick shows the
/// mod's [`text`](Self::text) in the verb's place instead. The prompt's title
/// stays the object's label ([`World::set_label`](crate::world::World::set_label)).
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::interactions::{ObjectPrompt, Verb};
///
/// let prompt = ObjectPrompt::new().verb(Verb::Use).replace_action();
/// assert_eq!(prompt.verb, Some(Verb::Use));
/// assert!(prompt.replace_action);
/// ```
#[cfg(feature = "unstable")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct ObjectPrompt {
    /// The verb every client shows; `None` keeps the object's own.
    pub verb: Option<Verb>,
    /// One of the mod's own texts, by its tag ([`Load::declare`](../struct.Load.html#method.declare),
    /// [`Load::add_texts`](../struct.Load.html#method.add_texts)), shown in
    /// the verb's place on clients with Enshwick, in the player's language.
    pub text: Option<crate::content::Guid>,
    /// The press runs the game's empty interaction, a short reach, instead
    /// of the object's own action, such as opening a chest; the mod's action
    /// on the game event `AcceptOfferedInteractionEvent` runs alone.
    #[script(default)]
    pub replace_action: bool,
}

#[cfg(feature = "unstable")]
impl ObjectPrompt {
    /// A prompt that changes nothing yet.
    ///
    /// # Examples
    ///
    /// See [`ObjectPrompt`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            verb: None,
            text: None,
            replace_action: false,
        }
    }

    /// Shows `verb` on every client.
    ///
    /// # Examples
    ///
    /// See [`ObjectPrompt`].
    #[must_use]
    pub const fn verb(mut self, verb: Verb) -> Self {
        self.verb = Some(verb);
        self
    }

    /// Shows the mod's text `tag` in the verb's place on clients with
    /// Enshwick.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::interactions::{ObjectPrompt, Verb};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let pray = load.declare(enshwick::content::id!("pray"))?;
    ///     load.prompt(enshwick::ItemId::new(0x5e01), ObjectPrompt::new().verb(Verb::Use).text(pray))
    /// }
    /// ```
    #[must_use]
    pub const fn text(mut self, tag: crate::content::Guid) -> Self {
        self.text = Some(tag);
        self
    }

    /// Runs the game's empty interaction on the press instead of the
    /// object's own action.
    ///
    /// # Examples
    ///
    /// See [`ObjectPrompt`].
    #[must_use]
    pub const fn replace_action(mut self) -> Self {
        self.replace_action = true;
        self
    }

    /// The ABI form for objects placed from `item`.
    fn raw(self, item: ItemId) -> crate::abi::InteractionPrompt {
        enshwick_abi::headed!(crate::abi::InteractionPrompt {
            item,
            verb: self.verb.map_or(0, Verb::id),
            text: self.text.map_or(0, crate::content::Guid::fnv1a),
            flags: if self.replace_action {
                crate::abi::InteractionPrompt::REPLACE_ACTION
            } else {
                0
            },
        })
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::interaction_targets`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn interaction_targets(&mut self) -> Result<Targets, Off>;

    /// As `Load::prompt`.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    fn prompt(&mut self, item: ItemId, prompt: ObjectPrompt) -> Result<(), Off>;

    /// As `Load::on_interaction`.
    #[doc(hidden)]
    fn on_interaction<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Interaction) -> bool + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    #[cfg(feature = "unstable")]
    fn interaction_targets(&mut self) -> Result<Targets, Off> {
        Targets::open(self)
    }

    #[cfg(feature = "unstable")]
    fn prompt(&mut self, item: ItemId, prompt: ObjectPrompt) -> Result<(), Off> {
        let host = self.host();
        let raw = prompt.raw(item);
        let status = (host.api().interaction_prompt)(host.handle(), &raw const raw);
        crate::load::ok(status, (), || format!("prompt on item {:#x}", item.get()))
    }

    fn on_interaction<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Interaction) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().interaction_filter)(host.handle(), Some(run_filter::<F>), user),
            "interaction filter",
        )
    }
}

/// What a player does with the target.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[script(name = "InteractionKind")]
#[non_exhaustive]
pub enum Kind {
    /// The game offers to open it.
    Open,
    /// Items move out of or into it, or out of a slot it holds.
    Move,
    /// Its inventory is sorted.
    Sort,
    /// It lends its stock to crafting or building nearby; asked without the
    /// target's item or position.
    Stock,
    /// The cursor offers to pick it up or tear it down.
    Dismantle,
    /// Its Quick Stash switch is flipped.
    QuickStash,
    /// It is upgraded into another piece.
    Upgrade,
}

/// An interaction the game is about to allow, from [`Load::on_interaction`](../struct.Load.html#method.on_interaction).
///
/// The name is enshwick's: the loader hooks the game functions behind it.
///
/// # Examples
///
/// See the [module](self) and [`Interaction::new`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Interaction {
    /// What the player does.
    pub kind: Kind,
    /// The player's character.
    pub player: Entity,
    /// The entity acted on.
    pub target: Entity,
    /// The item the target was placed from, when the platform could read it.
    pub item: Option<ItemId>,
    /// Where the target stands, in metres, when the platform could read it.
    pub position: Option<[f64; 3]>,
    /// What a [`Kind::Move`] moves, when the platform could read it.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    #[cfg(feature = "unstable")]
    pub moved: Option<Move>,
}

impl Interaction {
    /// An interaction of `kind` by `player` on `target` with nothing read of
    /// the target or a move, for a test of a filter; the `with_` methods add the rest.
    ///
    /// Luau has no constructor: a script writes the record as a table and
    /// leaves out the optional fields.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::interactions::{Interaction, Kind};
    /// use enshwick::{Entity, ItemId};
    ///
    /// const LOCKED_CHEST: ItemId = ItemId::new(0x5e01);
    ///
    /// fn allowed(i: &Interaction) -> bool {
    ///     i.item != Some(LOCKED_CHEST)
    /// }
    ///
    /// let open = Interaction::new(Kind::Open, Entity::from_engine(4096), Entity::from_engine(5000));
    /// assert!(allowed(&open));
    /// assert!(!allowed(&open.with_item(LOCKED_CHEST).with_position([1.0, 2.0, 3.0])));
    /// ```
    #[must_use]
    pub const fn new(kind: Kind, player: Entity, target: Entity) -> Self {
        Self {
            kind,
            player,
            target,
            item: None,
            position: None,
            #[cfg(feature = "unstable")]
            moved: None,
        }
    }

    /// This interaction with the item the target was placed from.
    ///
    /// # Examples
    ///
    /// See [`Interaction::new`].
    #[must_use]
    pub const fn with_item(mut self, item: ItemId) -> Self {
        self.item = Some(item);
        self
    }

    /// This interaction with where the target stands, in metres.
    ///
    /// # Examples
    ///
    /// See [`Interaction::new`].
    #[must_use]
    pub const fn with_position(mut self, position: [f64; 3]) -> Self {
        self.position = Some(position);
        self
    }
}

#[cfg(feature = "unstable")]
mod moves;
#[cfg(feature = "unstable")]
pub use moves::{InventoryActionType, Move, Transfer};

unsafe extern "system" fn run_filter<F>(call: *const RawInteraction, user: *mut c_void) -> u8
where
    F: Fn(&Interaction) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one.
    let Some(call) = (unsafe { call.as_ref() }) else {
        return crate::abi::FILTER_PASS;
    };
    let kind = match call.kind {
        RawInteraction::OPEN => Kind::Open,
        RawInteraction::MOVE => Kind::Move,
        RawInteraction::SORT => Kind::Sort,
        RawInteraction::STOCK => Kind::Stock,
        RawInteraction::DISMANTLE => Kind::Dismantle,
        RawInteraction::QUICK_STASH => Kind::QuickStash,
        RawInteraction::UPGRADE => Kind::Upgrade,
        _ => return crate::abi::FILTER_PASS,
    };
    if call.character.is_none() || call.target.is_none() {
        return crate::abi::FILTER_PASS;
    }
    let known = call.known != 0;
    let interaction = Interaction {
        kind,
        player: call.character,
        target: call.target,
        item: known.then_some(call.item),
        position: known.then_some(call.position),
        #[cfg(feature = "unstable")]
        moved: Move::of(call),
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    crate::host::verdict(crate::host::guard(
        "panic in an interaction filter",
        || true,
        || filter(&interaction),
    ))
}
