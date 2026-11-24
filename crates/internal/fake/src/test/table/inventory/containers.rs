//! The fake's container rules and pickup messages: each mod's rule by the
//! container item it names, and the messages mods show without a give.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use enshwick_core::abi::{ContainerPut, ContainerRuleFn, FILTER_REFUSE, Status};
use enshwick_core::{Entity, ItemId};

use crate::test::table::{Registered, with, with_open};

/// The most items one pickup message shows: the game's op record keeps a
/// `u16`.
const MOST_SHOWN: u32 = u16::MAX as u32;

/// The mods' container rules and the pickup messages they showed.
#[derive(Default)]
pub(crate) struct Containers {
    /// Each rule with the container item it rules, in load order.
    rules: Vec<(ItemId, Registered<ContainerRuleFn>)>,
    /// `(character, item, count)` of each message shown, in order.
    shown: Vec<(Entity, ItemId, u32)>,
}

entries! {
    /// As the loader: a null callback or item 0 is a bad argument, then a call
    /// after init is too late.
    pub(crate) fn container_rule(
        me as Mod,
        item: ItemId,
        callback: Option<ContainerRuleFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        if item.get() == 0 {
            return Status::BAD_ARGS;
        }
        with_open(|s| {
            s.inventory
                .containers
                .rules
                .push((item, Registered::new(callback, user, me)));
            Status::OK
        })
    }

    /// As the loader, in its order: a bad argument for item 0 or count 0, then
    /// unavailable outside a server, the wrong thread off the game thread, and
    /// a bad argument for an entity that is no player in game; the count held
    /// to the game's most.
    pub(crate) fn show_pickup(
        me as Caller,
        character: Entity,
        item: ItemId,
        count: u32,
    ) -> Status {
        if item.get() == 0 || count == 0 {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            if !s.host.base.serving() {
                return Status::UNAVAILABLE;
            }
            if !crate::test::table::on_game_thread() {
                return Status::WRONG_THREAD;
            }
            if !s.host.base.players.iter().any(|p| p.entity == character) {
                return Status::BAD_ARGS;
            }
            s.inventory
                .containers
                .shown
                .push((character, item, count.min(MOST_SHOWN)));
            Status::OK
        })
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl crate::test::Installed {
    /// Asks the rules of `put.container_item` about `put` as the loader
    /// does before a stack enters a container's slot; whether every one lets
    /// it in.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ContainerPut, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let put = ContainerPut {
    ///     header: Header::of::<ContainerPut>(),
    ///     container: enshwick::Entity(5000),
    ///     container_item: enshwick::ItemId::new(0x5e01),
    ///     item: enshwick::ItemId::new(0x0e17),
    ///     count: 1,
    ///     slot: 5001,
    /// };
    /// assert!(loader.container_put(&put));
    /// ```
    #[must_use]
    pub fn container_put(&self, put: &ContainerPut) -> bool {
        let rules = with(Vec::new(), |s| {
            s.inventory
                .containers
                .rules
                .iter()
                .filter(|(item, _)| *item == put.container_item)
                .map(|(_, rule)| *rule)
                .collect::<Vec<_>>()
        });
        rules.iter().all(|rule| {
            // SAFETY: a valid put; `user` is the mod's pointer.
            rule.run(|callback, user| unsafe { callback(put, user) != FILTER_REFUSE })
        })
    }

    /// The pickup messages the mod showed without a give, as
    /// `(character, item, count)` in order, each count held to the game's
    /// most.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.pickups_shown().is_empty());
    /// ```
    #[must_use]
    pub fn pickups_shown(&self) -> Vec<(Entity, ItemId, u32)> {
        with(Vec::new(), |s| s.inventory.containers.shown.clone())
    }
}
