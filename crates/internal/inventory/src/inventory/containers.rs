//! Containers: a chest, crate or bin with a slot count of its own, and which
//! items each kind of container takes.
//!
//! [`Load::add_container`](../../struct.Load.html#method.add_container) adds
//! a prop copied from a vanilla container, as
//! [`Load::add_prop`](../../struct.Load.html#method.add_prop) does, and sets
//! how many slots its template holds.
//! [`Load::container_rule`](../../struct.Load.html#method.container_rule)
//! decides, stack by stack, what enters the containers placed from an item,
//! the mod's own or a vanilla one, whatever moves it.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//!
//! ```
//! use enshwick::content::{Guid, id};
//! use enshwick::content::items::NewItem;
//! use enshwick::content::props::NewProp;
//! use enshwick::inventory::containers::NewContainer;
//!
//! const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
//! const IRON_ORE: enshwick::ItemId = enshwick::ItemId::new(0x0e17);
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let bin = load.add_container(NewContainer::new(NewProp::new(NewItem::new(id!("ore-bin"), CHEST)), 16))?;
//!     // The ore bin takes iron ore and nothing else.
//!     load.container_rule(bin.item, |put| put.item == IRON_ORE)
//! }
//! ```

use core::ffi::c_void;

use crate::__private::{INVENTORY_LINKS, INVENTORY_SLOTS};
use crate::Off;
use crate::abi::ContainerPut;
use crate::content::containers::{ContainerLayout, resize_container};
use crate::content::props::{NewProp, PropId};
use crate::content::types::TEMPLATE_RESOURCE;
use crate::content::vanilla::Vanilla;
use crate::inventory::SlotRef;
use crate::{Entity, ItemId};
use enshwick_base::vetoes::Verdict;
use enshwick_content::__load::*;

/// The most slots a container holds: every link of its inventory setup
/// full.
#[expect(
    clippy::cast_possible_truncation,
    reason = "16 links of 8 slots, 128, fit the game's u8 count"
)]
pub const MAX_SLOTS: u8 = INVENTORY_LINKS as u8 * INVENTORY_SLOTS;

/// A new container type for
/// [`Load::add_container`](../../struct.Load.html#method.add_container): a
/// prop copied from a vanilla container, holding [`slots`](Self::slots)
/// slots.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, id};
/// use enshwick::content::items::NewItem;
/// use enshwick::content::props::NewProp;
/// use enshwick::inventory::containers::NewContainer;
///
/// let prop = NewProp::new(NewItem::new(id!("ore-bin"), Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d")));
/// assert_eq!(NewContainer::new(prop, 16).slots, 16);
/// ```
#[derive(Clone, Debug, crate::Script)]
#[non_exhaustive]
pub struct NewContainer {
    /// The prop, whose item copies a vanilla container's.
    pub prop: NewProp,
    /// How many slots it holds: a multiple of 8, the game's row, from 8 to
    /// [`MAX_SLOTS`].
    pub slots: u8,
}

impl NewContainer {
    /// A container of `prop` holding `slots` slots.
    ///
    /// # Examples
    ///
    /// See [`NewContainer`].
    #[must_use]
    pub const fn new(prop: NewProp, slots: u8) -> Self {
        Self { prop, slots }
    }
}

/// Why the game cannot hold `slots` slots in one container; `None` when it
/// can.
fn slots_refusal(slots: u8) -> Option<String> {
    (slots == 0 || !slots.is_multiple_of(INVENTORY_SLOTS) || slots > MAX_SLOTS).then(|| {
        format!(
            "{slots} slots: a container holds a multiple of {INVENTORY_SLOTS} slots, \
             from {INVENTORY_SLOTS} to {MAX_SLOTS}"
        )
    })
}

/// A stack about to enter one slot of a container, from
/// [`Load::container_rule`](../../struct.Load.html#method.container_rule).
///
/// The name is enshwick's: the loader hooks the game's put of one stack into
/// one slot.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[script(name = "ContainerPut")]
#[non_exhaustive]
pub struct Put {
    /// The container: the placed entity whose inventory holds the slot.
    pub container: Entity,
    /// The item the container was placed from, the one the rule names.
    pub container_item: ItemId,
    /// The item of the stack.
    pub item: ItemId,
    /// How many items enter.
    pub count: u32,
    /// The slot they enter.
    pub slot: SlotRef,
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_container`.
    #[doc(hidden)]
    fn add_container(&mut self, new: NewContainer) -> Result<PropId, Off>;

    /// As `Load::container_rule`.
    #[doc(hidden)]
    fn container_rule<F, V>(&mut self, item: ItemId, rule: F) -> Result<(), Off>
    where
        F: Fn(&Put) -> V + Send + Sync + 'static,
        V: Into<Verdict>;

    /// As `Load::put_filter`.
    #[doc(hidden)]
    fn put_filter<F>(&mut self, item: ItemId, rule: F) -> Result<(), Off>
    where
        F: Fn(&Put) -> Verdict + Send + Sync + 'static;
}

unsafe extern "system" fn run_rule<F>(put: *const ContainerPut, user: *mut c_void) -> u8
where
    F: Fn(&Put) -> Verdict,
{
    // SAFETY: the loader passes a valid put for the duration of this call.
    let Some(put) = (unsafe { put.as_ref() }) else {
        return crate::abi::FILTER_PASS;
    };
    let put = Put {
        container: put.container,
        container_item: put.container_item,
        item: put.item,
        count: put.count,
        slot: SlotRef(put.slot),
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let rule = unsafe { &*user.cast::<F>() };
    crate::host::verdict(
        crate::host::guard("panic in a container rule", || Verdict::Pass, || rule(&put)).passes(),
    )
}

impl LoadMethods for crate::BaseLoad {
    fn add_container(&mut self, new: NewContainer) -> Result<PropId, Off> {
        let (item, slots) = (new.prop.item.id, new.slots);
        if let Some(why) = slots_refusal(slots) {
            return Err(Off::new(format!("container {item}: {why}")));
        }
        let like = new.prop.item.like;
        if let Some(vanilla) = Vanilla::game()
            && matches!(vanilla.container_slots(like), Ok(None))
        {
            return Err(Off::new(format!(
                "container {item}: the vanilla item {like} places no container"
            )));
        }
        let layout = ContainerLayout::resolve(self)?;
        let prop = self.add_prop(new.prop)?;
        let key = TEMPLATE_RESOURCE.key(prop.template.guid())?;
        // A second link on the template, after `add_prop`'s copy of it.
        self.serve(key, key, move |template, out| {
            resize_container(&layout, template, slots, out)
        })?;
        Ok(prop)
    }

    fn container_rule<F, V>(&mut self, item: ItemId, rule: F) -> Result<(), Off>
    where
        F: Fn(&Put) -> V + Send + Sync + 'static,
        V: Into<Verdict>,
    {
        self.put_filter(item, move |put| rule(put).into())
    }

    fn put_filter<F>(&mut self, item: ItemId, rule: F) -> Result<(), Off>
    where
        F: Fn(&Put) -> Verdict + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            rule,
            |user| (host.api().container_rule)(host.handle(), item, Some(run_rule::<F>), user),
            "container rule",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_holds_whole_rows_up_to_every_link() {
        for slots in [8, 16, 64, MAX_SLOTS] {
            assert_eq!(slots_refusal(slots), None, "{slots}");
        }
        for slots in [0, 7, 12, MAX_SLOTS + 8, u8::MAX] {
            assert!(slots_refusal(slots).is_some(), "{slots}");
        }
        assert_eq!(MAX_SLOTS, 128);
    }
}
