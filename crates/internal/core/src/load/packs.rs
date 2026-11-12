//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::packs::Pack;
use crate::{Load, Off};
use enshwick_packs::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Adds what `pack` describes, on the server and the client alike:
    /// its workshops, items, recipes, texts and loot, each through the call
    /// code uses, beside vanilla content, and declares its changes of
    /// vanilla values with [`Load::change`].
    ///
    /// # Errors
    ///
    /// [`Off`] as the calls it makes return it, such as for a content id
    /// the loader refuses, among them another mod's content id that mod
    /// does not export.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// const ITEMS: &str = r#"
    /// [[item]]
    /// id = "lantern"
    /// like = "6943053a-32c1-4718-ae22-e61427ddf54d"
    /// "#;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let files = PackFiles::new().with("items.toml", ITEMS);
    ///     load.add_pack(&Pack::parse("you.my-mod", &files)?)
    /// }
    /// ```
    pub fn add_pack(&mut self, pack: &Pack) -> Result<(), Off>;
}
