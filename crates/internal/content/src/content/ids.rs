use crate::abi::ResourceKey;

/// Names a piece of the mod's own content by its local name: `id!("locker")`
/// is the constant [`Id`] `<mod>:locker`, with the mod id read at build time
/// from the `[package.metadata.enshwick]` table of the crate's `Cargo.toml`.
///
/// The name is 1 to 128 of `[a-z0-9_./-]`; a malformed name, or a crate
/// without the table, fails the build with the reason. [`Id::part`] derives
/// further ids from one name.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id, id};
///
/// // This crate's mod id is `you.hello`.
/// const LOCKER: Id = id!("locker");
/// assert_eq!(LOCKER.to_string(), "you.hello:locker");
/// assert_eq!(LOCKER.guid(), Guid::named("you.hello:locker"));
/// ```
pub use enshwick_macros::id;
pub(crate) use enshwick_values::content::NAME_PART;
#[doc(inline)]
pub use enshwick_values::content::{DebugName, Guid, Id, NAMESPACE, is_id};

/// A vanilla resource's bytes from the game's own archive, as the game reads
/// them before it resolves their references.
///
/// Game data a mod builds on, such as an item's name tag or the template a
/// station item places, read the same at init and later, on the server and the
/// client.
///
/// `None` for a key the archive lacks. Each call reads the archive's files; keep what a mod reads
/// more than once.
///
/// # Examples
///
/// ```
/// use enshwick::content::{game_resource, types};
/// use enshwick::content::workshops::stations;
///
/// let key = types::ITEM_INFO.key(stations::WORKBENCH.item);
/// if let Some(item) = key.ok().and_then(game_resource) {
///     enshwick::info!("the Workbench item is {} bytes", item.len());
/// }
/// ```
#[must_use]
pub fn game_resource(key: ResourceKey) -> Option<Vec<u8>> {
    crate::host()?.game_resource(key)
}

/// The content id, `<mod>:<name>`, that a loaded mod declared for exactly
/// `guid`, such as a mod's item a player picked, which
/// [`Vanilla`](crate::content::vanilla::Vanilla) does not know. A cheap
/// lookup in the loader's ids, from any thread.
///
/// `None` for a GUID no loaded mod declared, a vanilla item's included, and
/// one that only shares a declared id's game id.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, content_id};
///
/// fn picked(item: Guid) -> String {
///     content_id(item).unwrap_or_else(|| "a vanilla item".to_owned())
/// }
/// ```
#[must_use]
pub fn content_id(guid: Guid) -> Option<String> {
    crate::__internal::content_id(&crate::host()?, guid)
}
