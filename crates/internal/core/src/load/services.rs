//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::services::Service;
#[cfg(feature = "unstable")]
use crate::services::{
    Call, Caller, DescribedCall, DescribedCaller, DescribedEmitter, DescribedEvent,
};
use crate::{Load, Off};

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Publishes `table` as the service its type declares, for other mods.
    ///
    /// The type comes from [`#[service]`](macro@crate::service), and the
    /// loader keeps its layout to check every user against.
    ///
    /// # Errors
    ///
    /// [`Off`] when another mod publishes the same name and major.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::service(name = "prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// extern "system" fn price(_item: u32) -> u32 {
    ///     0
    /// }
    ///
    /// static PRICES: Prices = Prices { price };
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.publish(&PRICES)
    /// }
    /// ```
    pub fn publish<T: Service>(&mut self, table: &'static T) -> Result<(), Off>;

    /// The service `T` declares, published by another mod, once the loader
    /// has checked that the published table starts with every field of
    /// `T`.
    ///
    /// A publisher built against a longer declaration serves this mod; one
    /// built against a shorter declaration does not, and the error names
    /// the first field it lacks.
    ///
    /// # Errors
    ///
    /// [`Off`] with the reason when no running mod publishes the service,
    /// or the published table lacks or changed a field of `T`. Declare the
    /// publisher in `requires` so it loads first.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::service(of = trading_post, name = "prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let prices: &'static Prices = load.service()?;
    ///     let _ = (prices.price)(0);
    ///     Ok(())
    /// }
    /// ```
    pub fn service<T: Service>(&self) -> Result<&'static T, Off>;

    /// The service `T` declares when a running mod publishes a table that
    /// serves it, `None` otherwise: for a service the mod can do without,
    /// with the publisher in `after`. A table that does not serve `T` is
    /// logged as a warning.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::service(of = trading_post, name = "prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     if let Some(prices) = load.optional_service::<Prices>() {
    ///         let _ = (prices.price)(0);
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub fn optional_service<T: Service>(&self) -> Option<&'static T>;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Serves the call `C` with `handler` for other mods to call, from the
    /// caller's thread. A panic in `handler` fails that call alone.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// `C` comes from [`#[call]`](macro@crate::call), named after this mod:
    /// `<id>:<call>`, as [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md)
    /// sets out. Mark it `export` so other mods may call it and its schema
    /// reaches their tools.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id, the call and major
    /// are served already, or when called after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::call(name = "price-of", output = u32)]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct PriceOf {
    ///     pub item: u32,
    /// }
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.serve_call(|input: &PriceOf| input.item * 10)
    /// }
    /// ```
    pub fn serve_call<C, F>(&mut self, handler: F) -> Result<(), Off>
    where
        C: Call,
        F: Fn(&C) -> C::Output + Send + Sync + 'static;

    /// Asks for another mod's call `C`, and returns the handle that runs it.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// The loader matches it once every mod is initialised: with no running
    /// mod serving `C` as declared here, it logs why, and every call answers
    /// [`CallError::Unmatched`](crate::services::CallError::Unmatched). Declare the server in the manifest with
    /// `after`, or `requires` when this mod is pointless without it: a
    /// required server initialises first, so the loader matches the call
    /// at once.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`, when this mod asked for `C`
    /// already with another declaration, or when this mod requires the
    /// server and it does not serve `C` as declared here; the log says why.
    ///
    /// # Examples
    ///
    /// See [`Caller`].
    pub fn caller<C: Call>(&mut self) -> Result<Caller<C>, Off>;

    /// Serves the described call `call` with `handler`, which gets the
    /// input's bytes and writes the answer's, as [`Load::serve_call`] does for a
    /// type; it returns `false` to fail the call.
    ///
    /// # Errors
    ///
    /// As [`Load::serve_call`].
    ///
    /// # Safety
    ///
    /// - Every answer `handler` writes and returns `true` for is a valid
    ///   value of the type the answer's hash names, since a caller reads it
    ///   as that type: a `bool` is 0 or 1, an enum one of its variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::points::DescribedField;
    /// use enshwick::services::DescribedCall;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let price_of = DescribedCall::new(
    ///         "you.trading-post:price-of",
    ///         1,
    ///         (vec![DescribedField::new("item", 1)], 4),
    ///         (vec![DescribedField::new("output", 2)], 4),
    ///     );
    ///     // SAFETY: the answer is one `u32`, for which any bytes are valid.
    ///     unsafe {
    ///         load.serve_described_call(&price_of, |input, output| {
    ///             output.copy_from_slice(input);
    ///             true
    ///         })
    ///     }
    /// }
    /// ```
    pub unsafe fn serve_described_call<F>(
        &mut self,
        call: &DescribedCall,
        handler: F,
    ) -> Result<(), Off>
    where
        F: Fn(&[u8], &mut [u8]) -> bool + Send + Sync + 'static;

    /// Asks for another mod's described call `call`, as [`Load::caller`]
    /// does for a type.
    ///
    /// # Errors
    ///
    /// As [`Load::caller`].
    ///
    /// # Safety
    ///
    /// - Every input passed to the returned caller is a valid value of the
    ///   type each input field's hash names, since the server reads it as
    ///   that type: a `bool` is 0 or 1, an enum one of its variants.
    ///
    /// # Examples
    ///
    /// See [`DescribedCaller`].
    pub unsafe fn described_caller(&mut self, call: &DescribedCall)
    -> Result<DescribedCaller, Off>;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Calls `handler` with the data of every described mod event `event`
    /// another mod sends, as [`on_mod_event`](Self::on_mod_event) does for a
    /// type: the loader checks the layout hashes against the definition.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`.
    ///
    /// # Examples
    ///
    /// See [`DescribedEvent`].
    pub fn on_described_mod_event<F>(
        &mut self,
        event: &DescribedEvent,
        handler: F,
    ) -> Result<(), Off>
    where
        F: Fn(&[u8]) + Send + Sync + 'static;

    /// Defines the described mod event `event` for other mods to subscribe
    /// to, as [`define_event`](Self::define_event) does for a type.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice.
    ///
    /// # Safety
    ///
    /// - Every byte pattern sent through the returned emitter is a valid
    ///   value of the type each field's hash names, since a subscriber reads
    ///   the data as that type: a `bool` is 0 or 1, an enum one of its
    ///   variants.
    ///
    /// # Examples
    ///
    /// See [`DescribedEmitter`].
    pub unsafe fn define_described_event(
        &mut self,
        event: &DescribedEvent,
    ) -> Result<DescribedEmitter, Off>;
}
