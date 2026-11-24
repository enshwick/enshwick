//! The fake's changes to players' permissions: each mod's change by account,
//! applied to the fake's player at once, where the loader applies it from
//! the server's next step; and a respawn that moves a player to a new
//! character, which a mod that changes permissions must follow.

use crate::test::table::entries;
use enshwick_core::abi::{ModHandle, PlayerInfo, Status};
use enshwick_core::{Entity, PlayerKey};

use crate::test::table::{on_game_thread, with};

/// The mods' changes, and the group bits of each changed player.
#[derive(Default)]
pub(crate) struct Changes {
    /// `(mod, account, deny, grant)`, in the order made.
    by_mod: Vec<(ModHandle, PlayerKey, u32, u32)>,
    /// The bits each changed player joined with: their user group's.
    groups: Vec<(PlayerKey, u32)>,
}

impl Changes {
    /// The bits `account`, whose group gives `group`, holds under every
    /// mod's change.
    fn held(&self, account: PlayerKey, group: u32) -> u32 {
        let (deny, grant) = self.of(account);
        (group | grant) & !deny
    }

    /// Every mod's change to `account`, joined, as `(deny, grant)`.
    fn of(&self, account: PlayerKey) -> (u32, u32) {
        self.by_mod
            .iter()
            .filter(|c| c.1 == account)
            .fold((0, 0), |(deny, grant), c| (deny | c.2, grant | c.3))
    }

    /// Ends every mod's change to `account`, whose player left, as the
    /// loader does.
    pub(crate) fn left(&mut self, account: PlayerKey) {
        self.by_mod.retain(|c| c.1 != account);
        self.groups.retain(|g| g.0 != account);
    }

    /// Ends the change of mod `me` to `account`, whose player is not in
    /// game.
    fn left_by(&mut self, me: ModHandle, account: PlayerKey) {
        self.by_mod.retain(|c| (c.0, c.1) != (me, account));
        if !self.by_mod.iter().any(|c| c.1 == account) {
            self.groups.retain(|g| g.0 != account);
        }
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl crate::test::Installed {
    /// The permission bits every mod takes from and gives to the account
    /// `key` now, as `(deny, grant)`; `(0, 0)` while none changes them.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.permission_change(enshwick::PlayerKey::new(7)), (0, 0));
    /// ```
    #[must_use]
    pub fn permission_change(&self, key: PlayerKey) -> (u32, u32) {
        with((0, 0), |s| s.world.permission_changes.of(key))
    }

    /// Gives the player in game with account `key` the character `entity`,
    /// as a respawn would if the game made a new character entity, so a test
    /// checks that a mod follows the player and not a stale entity; false
    /// for an account not in game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::PlayerInfo;
    ///
    /// let ann = PlayerInfo::with_name(enshwick::Entity(2), enshwick::PlayerKey::new(7), 0, "Ann");
    /// let loader = enshwick::test::FakeHost::new().player(ann).install();
    /// assert!(loader.respawn(enshwick::PlayerKey::new(7), enshwick::Entity(3)));
    /// assert!(!loader.respawn(enshwick::PlayerKey::new(8), enshwick::Entity(4)));
    /// ```
    #[must_use]
    pub fn respawn(&self, key: PlayerKey, entity: Entity) -> bool {
        with(false, |s| {
            s.host
                .base
                .players
                .iter_mut()
                .find(|p| p.key == key)
                .map(|p| p.entity = entity)
                .is_some()
        })
    }
}

entries! {
    pub(crate) fn change_permissions(
        me as Mod,
        account: PlayerKey,
        deny: u32,
        grant: u32,
    ) -> Status {
        if account == PlayerKey::new(0) || (deny | grant) & !PlayerInfo::ALL_PERMISSIONS != 0 {
            return Status::BAD_ARGS;
        }
        let ends = deny == 0 && grant == 0;
        if !ends && !on_game_thread() {
            return Status::WRONG_THREAD;
        }
        with(Status::MISSING, |s| {
            if !ends && (!s.host.base.serving() || s.host.base.world_key.is_none()) {
                return Status::UNAVAILABLE;
            }
            let Some(player) = s.host.base.players.iter_mut().find(|p| p.key == account) else {
                // As the loader: ending a change needs no player in game.
                if ends {
                    s.world.permission_changes.left_by(me, account);
                    return Status::OK;
                }
                return Status::REFUSED;
            };
            let changes = &mut s.world.permission_changes;
            if !changes.groups.iter().any(|g| g.0 == account) {
                changes.groups.push((account, player.permissions));
            }
            let group = changes
                .groups
                .iter()
                .find_map(|g| (g.0 == account).then_some(g.1))
                .unwrap_or(player.permissions);
            changes.by_mod.retain(|c| (c.0, c.1) != (me, account));
            if deny != 0 || grant != 0 {
                changes.by_mod.push((me, account, deny, grant));
            }
            player.permissions = changes.held(account, group);
            if !changes.by_mod.iter().any(|c| c.1 == account) {
                changes.groups.retain(|g| g.0 != account);
            }
            Status::OK
        })
    }
}
