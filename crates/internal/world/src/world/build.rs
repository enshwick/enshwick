//! Whether a player may edit a box of the world, by the server's own rule.

use crate::PlayerKey;
use crate::abi::{BuildRights, Header, PlayerInfo};

use super::{Player, World, WorldError};

/// A permission the server grants a player through its user groups, one bit
/// of the game's `PlayerPermissions` (server 26430253).
///
/// The groups are `userGroups` in `enshrouded_server.json`, and a player
/// holds the permissions of the group whose password they joined with.
///
/// # Examples
///
/// See [`BuildRefusal`] and [`Player::has`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
#[expect(
    clippy::enum_variant_names,
    reason = "the game's reflected names, which all start with Can"
)]
pub enum PlayerPermissions {
    /// May kick and ban: the server's administrators.
    CanKickBan,
    /// May open containers in bases.
    CanAccessInventories,
    /// Edits inside a base: a Flame Altar's building zone.
    CanEditBase,
    /// Every edit of the world, inside a base or not.
    CanEditWorld,
    /// May place, remove and upgrade Flame Altars.
    CanExtendBase,
    /// Receives experience (the game's `CanReceiveEXP`).
    CanReceiveExp,
}

impl PlayerPermissions {
    /// Every permission, in the game's bit order.
    pub const ALL: [Self; 6] = [
        Self::CanKickBan,
        Self::CanAccessInventories,
        Self::CanEditBase,
        Self::CanEditWorld,
        Self::CanExtendBase,
        Self::CanReceiveExp,
    ];

    /// Every permission's bit at once, as the server's console holds them:
    /// [`PlayerInfo::ALL_PERMISSIONS`].
    pub const ALL_BITS: u32 = {
        let mut bits = 0;
        let mut i = 0;
        while i < Self::ALL.len() {
            bits |= Self::ALL[i].bit();
            i += 1;
        }
        assert!(
            bits == PlayerInfo::ALL_PERMISSIONS,
            "every permission, no other bit"
        );
        bits
    };

    /// The permission's bit in [`Player::permissions`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::PlayerPermissions;
    ///
    /// assert_eq!(PlayerPermissions::CanKickBan.bit(), 1);
    /// ```
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Self::CanKickBan => PlayerInfo::CAN_KICK_BAN,
            Self::CanAccessInventories => PlayerInfo::CAN_ACCESS_INVENTORIES,
            Self::CanEditBase => PlayerInfo::CAN_EDIT_BASE,
            Self::CanEditWorld => PlayerInfo::CAN_EDIT_WORLD,
            Self::CanExtendBase => PlayerInfo::CAN_EXTEND_BASE,
            Self::CanReceiveExp => PlayerInfo::CAN_RECEIVE_EXP,
        }
    }
}

/// Why the server refuses a player's edit, from [`World::can_build`]; the
/// variants carry the names of the game's `CursorFlag` refusals.
///
/// # Examples
///
/// ```
/// use enshwick::world::{BuildRefusal, PlayerPermissions};
///
/// let refusal = BuildRefusal::NoPermission(PlayerPermissions::CanEditBase);
/// assert_eq!(refusal.to_string(), "the player lacks CanEditBase");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, crate::Script)]
#[non_exhaustive]
pub enum BuildRefusal {
    /// The player lacks the permission: `CanEditWorld` for any edit,
    /// `CanEditBase` for one that reaches into a building zone. A player
    /// no longer in game has none.
    #[error("the player lacks {0:?}")]
    NoPermission(PlayerPermissions),
    /// A corner lies outside the world's building area, past the edge the
    /// game lets players build to.
    #[error("outside the world's building area")]
    CursorOutsideBuildingZone,
    /// The box reaches into a strict no-build zone the world places.
    #[error("inside a strict no-build zone")]
    CursorInsideStrictBuildInhibitorZone,
    /// The server could not check, so the edit is refused; enshwick's own
    /// refusal, not the game's.
    #[error("the server could not check: {0}")]
    Unchecked(#[from] WorldError),
}

#[crate::script(build)]
impl World {
    /// Whether the server lets `player` edit the box between the corners
    /// `a` and `b` in metres; a mod that edits the world for a player asks
    /// first. The permissions are the game's terrain tool rule:
    /// `CanEditWorld` always, `CanEditBase` where the box overlaps a
    /// building zone. Enshwick adds two tests the game's building cursor
    /// runs for items: both corners inside the world's building area, and
    /// no strict no-build zone the world places. An item's other rules,
    /// such as one that needs a zone, are not part of it.
    ///
    /// # Errors
    ///
    /// The [`BuildRefusal`]; [`BuildRefusal::Unchecked`] off the server's
    /// game thread, while no world is loaded, for a corner that is not
    /// finite and on a game build without the checks. On the game thread
    /// at a quiet moment only, outside a step or in a game event: refused
    /// inside a step.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::{BuildRefusal, Player, World};
    ///
    /// fn dig(world: World, player: &Player, at: [f64; 3]) {
    ///     let [x, y, z] = at;
    ///     match world.can_build(player, [x - 2.0, y - 2.0, z - 2.0], [x + 2.0, y + 2.0, z + 2.0]) {
    ///         Ok(()) => enshwick::info!("{} may dig here", player.name),
    ///         Err(BuildRefusal::NoPermission(missing)) => {
    ///             world.tell(player, &format!("You need {missing:?} to dig here"));
    ///         }
    ///         Err(why) => enshwick::debug!("no dig: {why}"),
    ///     }
    /// }
    /// ```
    #[script]
    pub fn can_build(self, player: &Player, a: [f64; 3], b: [f64; 3]) -> Result<(), BuildRefusal> {
        if !a.iter().chain(&b).all(|v| v.is_finite()) {
            return Err(BuildRefusal::Unchecked(WorldError::Refused));
        }
        let mut check = BuildRights {
            header: Header::of::<BuildRights>(),
            character: player.entity,
            refusal: BuildRights::ALLOWED,
            min: [0, 1, 2].map(|i| a[i].min(b[i])),
            max: [0, 1, 2].map(|i| a[i].max(b[i])),
        };
        let status = (self.0.host.api().build_rights)(self.0.host.handle(), &raw mut check);
        WorldError::from_status(status)?;
        BuildRefusal::from_code(check.refusal).map_or(Ok(()), Err)
    }
    /// Whether a save keeps changes in the box between corners `a` and `b`,
    /// in metres. The game saves a world only inside its bases, the flame
    /// altars' build zones: a prop placed or terrain changed anywhere else
    /// is gone after a restart, a player's as well as a mod's. The box
    /// must lie wholly inside one base's zone, half a metre in from its
    /// edges.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a corner that is not finite;
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Unavailable`] while no world is loaded or on a game
    /// build without the zone list.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::WorldError;
    /// use enshwick::world::World;
    ///
    /// fn worth_placing(world: World, at: [f64; 3]) -> Result<bool, WorldError> {
    ///     let [x, y, z] = at;
    ///     world.persists([x - 1.0, y, z - 1.0], [x + 1.0, y + 2.0, z + 1.0])
    /// }
    /// ```
    #[script]
    pub fn persists(self, a: [f64; 3], b: [f64; 3]) -> Result<bool, WorldError> {
        if !a.iter().chain(&b).all(|v| v.is_finite()) {
            return Err(WorldError::Refused);
        }
        let min = [0, 1, 2].map(|i| a[i].min(b[i]));
        let max = [0, 1, 2].map(|i| a[i].max(b[i]));
        let mut kept = 0u8;
        let status = (self.0.host.api().world_persists)(
            self.0.host.handle(),
            &raw const min,
            &raw const max,
            &raw mut kept,
        );
        WorldError::from_status(status)?;
        Ok(kept != 0)
    }

    /// Takes the permissions in `deny` from the player in game with account
    /// `key` and gives those in `grant`, on top of their user group's, so
    /// the game's own server checks follow: a taken `CanEditWorld` stops
    /// digging, bombs, building and breaking anywhere, `CanEditBase` inside
    /// bases, `CanAccessInventories` opening chests in bases. Each call
    /// replaces the mod's own change; both lists empty ends it, also for a
    /// player no longer in game. A permission any mod takes is gone even
    /// when another gives it. The change holds from the server's next step
    /// until the mod ends it or turns off, the player leaves or the world
    /// ends; [`Player::permissions`] and the loader's own admin checks then
    /// read the bits in force. The player's client still acts by its
    /// group's bits: it may show an edit the server then refuses, and it
    /// never starts an edit that only a granted bit allows.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for account 0 and a new change to an account
    /// not in game; [`WorldError::WrongThread`] off
    /// the server's game thread; [`WorldError::Unavailable`] outside a
    /// server and on a game build without the permission table, the step
    /// link or the player removal link.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::{Player, PlayerPermissions, World};
    ///
    /// fn read_only(world: World, player: &Player) {
    ///     let taken = [PlayerPermissions::CanEditWorld, PlayerPermissions::CanEditBase];
    ///     if let Err(why) = world.change_permissions(player.key, &taken, &[]) {
    ///         enshwick::warn!("{} keeps their permissions: {why}", player.name);
    ///     }
    /// }
    /// ```
    #[script]
    pub fn change_permissions(
        self,
        key: PlayerKey,
        deny: &[PlayerPermissions],
        grant: &[PlayerPermissions],
    ) -> Result<(), WorldError> {
        let bits = |list: &[PlayerPermissions]| list.iter().fold(0, |bits, p| bits | p.bit());
        WorldError::from_status((self.0.host.api().change_permissions)(
            self.0.host.handle(),
            key,
            bits(deny),
            bits(grant),
        ))
    }
}

impl BuildRefusal {
    /// The refusal a [`BuildRights`] code stands for; `None` for
    /// [`BuildRights::ALLOWED`].
    pub(crate) const fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            BuildRights::ALLOWED => return None,
            BuildRights::NO_CAN_EDIT_WORLD => Self::NoPermission(PlayerPermissions::CanEditWorld),
            BuildRights::NO_CAN_EDIT_BASE => Self::NoPermission(PlayerPermissions::CanEditBase),
            BuildRights::OUTSIDE_BUILDING_ZONE => Self::CursorOutsideBuildingZone,
            BuildRights::INSIDE_STRICT_BUILD_INHIBITOR_ZONE => {
                Self::CursorInsideStrictBuildInhibitorZone
            }
            _ => Self::Unchecked(WorldError::Refused),
        })
    }
}
