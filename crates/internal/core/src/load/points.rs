//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::points::{
    DescribedOwnedPoint, DescribedPoint, DescribedVerdict, Order, OwnedPoint, Point, Verdict,
};
use crate::{Load, Off};

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Defines the extension point `P` for other mods to extend, and returns
    /// the handle that asks it.
    ///
    /// `P` comes from [`#[point]`](macro@crate::point), named after this
    /// mod: `<id>:<point>`, see
    /// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
    /// Links run once every mod is initialised.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice, or
    /// when called after `init`.
    ///
    /// # Examples
    ///
    /// See [`OwnedPoint`].
    pub fn define_point<P: Point>(&mut self) -> Result<OwnedPoint<P>, Off>;

    /// Attaches `link` to another mod's extension point `P`, at `order` in
    /// its chain. A panic in `link` turns this link off, not the mod.
    ///
    /// The loader matches the link once every mod is initialised: with no
    /// running mod defining `P`, the link stays inert, which is no error;
    /// with a definition whose input lacks or changed a field of `P`, or
    /// whose answer or mode differs, it logs why and the link stays inert.
    /// Declare the owner in the manifest with `after`, or `requires`
    /// when this mod is pointless without it: a required owner initialises
    /// first, so the loader matches the link at once.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`, when this mod links `P` already,
    /// or when this mod requires the owner and it does not define `P` as
    /// declared here; the log says why.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::points::{Order, Verdict};
    ///
    /// #[enshwick::point(of = trading_post, name = "price", mode = Override, output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Price {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.extend_point(&Order::new(), |price: &Price, coins: &u32| {
    ///         if price.item == 7 { Verdict::Set(coins / 2) } else { Verdict::Pass }
    ///     })
    /// }
    /// ```
    pub fn extend_point<P: Point>(
        &mut self,
        order: &Order,
        link: impl Fn(&P, &P::Output) -> Verdict<P::Output> + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Attaches `link` to another mod's extension point `P` to see each
    /// answer as the links before left it, changing nothing; otherwise as
    /// [`Load::extend_point`].
    ///
    /// # Errors
    ///
    /// As [`Load::extend_point`].
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::points::Order;
    ///
    /// #[enshwick::point(of = trading_post, name = "price", mode = Observe, output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Price {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.observe_point(&Order::new(), |price: &Price, coins: &u32| {
    ///         enshwick::debug!("item {} costs {coins}", price.item);
    ///     })
    /// }
    /// ```
    pub fn observe_point<P: Point>(
        &mut self,
        order: &Order,
        link: impl Fn(&P, &P::Output) + Send + Sync + 'static,
    ) -> Result<(), Off>;
}

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Links `link` into another mod's extension point described by
    /// `point`, as [`extend_point`](Self::extend_point) does for a type:
    /// the loader checks the layout hashes against the owner's and orders
    /// the link by `order`. `link` gets the input's bytes, `input_size` of
    /// them, and the answer so far.
    ///
    /// At the seal the loader leaves the link idle, with the reason in the
    /// log, when a hash differs, the description reads past the owner's
    /// input or its answer has another size.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the link, as after `init` or for a
    /// second link of this mod on the point.
    ///
    /// # Safety
    ///
    /// - The bytes of every [`DescribedVerdict::Set`] the link answers are a
    ///   valid value of the owner's answer type: a `bool` is 0 or 1, an enum
    ///   one of its variants, every field of a struct likewise.
    ///
    /// # Examples
    ///
    /// See [`DescribedPoint`].
    pub unsafe fn extend_described_point<F>(
        &mut self,
        point: &DescribedPoint,
        order: &Order,
        link: F,
    ) -> Result<(), Off>
    where
        F: Fn(&[u8], &[u8]) -> DescribedVerdict + Send + Sync + 'static;

    /// Defines the extension point `point` describes for other mods to
    /// extend, as [`define_point`](Self::define_point) does for a type, and
    /// returns the handle that asks it with bytes.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice, or
    /// when called after `init`.
    ///
    /// # Safety
    ///
    /// - The input and the answer of every [`DescribedOwnedPoint::decide`]
    ///   are valid values of the types the hashes name, since links read them
    ///   as those types: a `bool` is 0 or 1, an enum one of its variants, and
    ///   no byte is padding.
    ///
    /// # Examples
    ///
    /// See [`DescribedOwnedPoint`].
    pub unsafe fn define_described_point(
        &mut self,
        point: &DescribedPoint,
    ) -> Result<DescribedOwnedPoint, Off>;
}
