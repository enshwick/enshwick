//! Content renames, so content saved under an old GUID reads as the new one.
//!
//! A renamed item's old stacks and placed objects stay; a mod declares its
//! renames once with [`Load::rename_content`](../../struct.Load.html#method.rename_content).

use crate::abi::ContentRename;
use crate::content::Guid;
use crate::{ItemId, Off};

/// Every content rename a mod ever made, oldest first.
///
/// # Examples
///
/// ```
/// use enshwick::content::renames::Renames;
/// use enshwick::content::{Guid, Id, id};
///
/// const OLD_LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
/// const LOCKER: Id = id!("locker");
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.rename_content(Renames::new().rename(OLD_LOCKER, LOCKER.guid()))
/// }
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Renames {
    list: Vec<(Guid, Guid)>,
}

impl Renames {
    /// No renames yet.
    ///
    /// # Examples
    ///
    /// See [`Renames`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads content saved under `from`, such as a placed object of an item
    /// whose GUID changed, as the content now under `to`. An item the mod
    /// adds under `to` is also served under `from`, so stacks saved with the
    /// old item stay.
    ///
    /// # Examples
    ///
    /// See [`Renames`].
    #[must_use]
    pub fn rename(mut self, from: Guid, to: Guid) -> Self {
        self.list.push((from, to));
        self
    }
}

/// The mod's renames, kept for the process.
static KEPT: crate::fresh::Fresh<Renames> = crate::fresh::Fresh::new();

/// The GUIDs content the mod adds under `guid` was saved under before, by
/// its renames.
pub(crate) fn old_guids(guid: Guid) -> Vec<Guid> {
    KEPT.get()
        .map_or_else(Vec::new, |kept| renamed_to(&kept.list, guid))
}

/// Every GUID a chain of renames in `list` leads from to `guid`.
fn renamed_to(list: &[(Guid, Guid)], guid: Guid) -> Vec<Guid> {
    let mut found: Vec<Guid> = Vec::new();
    let mut at = 0;
    let mut wanted = guid;
    loop {
        for &(from, to) in list {
            if to == wanted && from != guid && !found.contains(&from) {
                found.push(from);
            }
        }
        let Some(&next) = found.get(at) else {
            return found;
        };
        wanted = next;
        at += 1;
    }
}

/// The game id content saved as `id` has now: the id of the content the
/// mod's renames lead `id`'s GUID to, else `id` itself.
///
/// An item renamed with [`Renames::rename`] stays in old stacks under its
/// old id, so compare item ids through this.
///
/// # Examples
///
/// ```
/// use enshwick::ItemId;
/// use enshwick::content::renames::{Renames, current_id};
/// use enshwick::content::{Guid, Id, id};
///
/// const OLD_LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
/// const LOCKER: Id = id!("locker");
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.rename_content(Renames::new().rename(OLD_LOCKER, LOCKER.guid()))?;
///     let locker = ItemId::from_guid(LOCKER.guid());
///     assert_eq!(current_id(ItemId::from_guid(OLD_LOCKER)), locker);
///     assert_eq!(current_id(locker), locker);
///     Ok(())
/// }
/// ```
#[must_use]
pub fn current_id(id: ItemId) -> ItemId {
    KEPT.get().map_or(id, |kept| renamed_from(&kept.list, id))
}

/// The id at the end of the chain of renames in `list` that starts at
/// `id`: as many hops as there are renames, so a cycle ends.
fn renamed_from(list: &[(Guid, Guid)], id: ItemId) -> ItemId {
    let mut current = id;
    for _ in list {
        match list
            .iter()
            .find(|(from, _)| ItemId::from_guid(*from) == current)
        {
            Some(&(_, to)) if ItemId::from_guid(to) != current && ItemId::from_guid(to) != id => {
                current = ItemId::from_guid(to);
            }
            _ => break,
        }
    }
    current
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::rename_content`.
    #[doc(hidden)]
    fn rename_content(&mut self, renames: Renames) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn rename_content(&mut self, renames: Renames) -> Result<(), Off> {
        if let Some((from, _)) = renames.list.iter().find(|(from, to)| from == to) {
            return Err(Off::new(format!(
                "content renames: {from} renamed to itself"
            )));
        }
        let list: Vec<ContentRename> = renames
            .list
            .iter()
            .map(|(from, to)| ContentRename {
                from: from.0,
                to: to.0,
            })
            .collect();
        if KEPT.set(renames).is_err() {
            return Err(Off::new("content renames: declared twice"));
        }
        let host = self.host();
        let count = u32::try_from(list.len()).unwrap_or(u32::MAX);
        let status = (host.api().rename_content)(host.handle(), list.as_ptr(), count);
        crate::load::ok(status, (), || {
            "content renames: refused; the log says why".to_owned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{renamed_from, renamed_to};
    use crate::ItemId;
    use crate::content::Guid;

    #[test]
    fn renames_follow_their_chain_both_ways() {
        let (a, b, c) = (Guid::named("t:a"), Guid::named("t:b"), Guid::named("t:c"));
        let list = [(a, b), (b, c)];
        assert_eq!(
            renamed_from(&list, ItemId::from_guid(a)),
            ItemId::from_guid(c)
        );
        assert_eq!(
            renamed_from(&list, ItemId::from_guid(b)),
            ItemId::from_guid(c)
        );
        assert_eq!(
            renamed_from(&list, ItemId::from_guid(c)),
            ItemId::from_guid(c)
        );
        assert_eq!(renamed_to(&list, c), [b, a]);
        let cycle = [(a, b), (b, a)];
        assert_eq!(
            renamed_from(&cycle, ItemId::from_guid(a)),
            ItemId::from_guid(b)
        );
        assert_eq!(renamed_to(&cycle, a), [b]);
    }
}
