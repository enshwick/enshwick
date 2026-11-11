//! The examples of `enshwick-base` that name items above its crate,
//! compiled here as written.
//!
//! `script::events::GameEventKind::subscribe`:
//!
//! ```rust,standalone_crate
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let died = enshwick::script::game_events()
//!         .find(|e| e.name() == "EntityDiedEvent")
//!         .ok_or(enshwick::Off::new("no such event"))?;
//!     died.subscribe(load, |data| enshwick::debug!("{:?}", data()))
//! }
//! ```
//!
//! `points::described::DescribedPoint`:
//!
//! ```
//! use enshwick::points::{DescribedField, DescribedPoint, DescribedVerdict, Mode, Order};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let price = DescribedPoint::new("you.trading-post:price", Mode::Override)
//!         .with_input(vec![DescribedField::new("item", 5_938_679_357_011_966_802)], 4)
//!         .with_output(14_918_346_923_295_848_190, 4);
//!     // SAFETY: every answer set is a `u32`, the point's answer type, for
//!     // which any bytes are a valid value.
//!     unsafe {
//!         load.extend_described_point(&price, &Order::new(), |input, _answer| {
//!             let item = u32::from_le_bytes(input[..4].try_into().unwrap_or_default());
//!             if item == 7 { DescribedVerdict::Set(10u32.to_le_bytes().to_vec()) } else { DescribedVerdict::Pass }
//!         })
//!     }
//! }
//! ```
//!
//! `points::described::DescribedOwnedPoint`:
//!
//! ```
//! use enshwick::points::{DescribedField, DescribedPoint, Mode};
//! use enshwick::services::layout_hash;
//! use enshwick::Shape as _;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let price = DescribedPoint::new("you.hello:price", Mode::Override)
//!         .with_input(vec![DescribedField::new("item", layout_hash("item", u32::SHAPE, 4, 4, 0))], 4)
//!         .with_output(layout_hash("output", u32::SHAPE, 4, 4, 0), 4);
//!     // SAFETY: every input and answer asked is a `u32`, valid for any bytes.
//!     let price = unsafe { load.define_described_point(&price) }?;
//!     let mut coins = 10u32.to_le_bytes();
//!     let decision = price.decide(&7u32.to_le_bytes(), &mut coins);
//!     enshwick::info!("item 7: {decision:?}, {} coins", u32::from_le_bytes(coins));
//!     Ok(())
//! }
//! ```
//!
//! `services::calls::Caller`:
//!
//! ```
//! #[enshwick::call(of = trading_post, name = "price-of", output = u32)]
//! #[derive(Clone, Copy, Debug)]
//! pub struct PriceOf {
//!     pub item: u32,
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let price_of = load.caller::<PriceOf>()?;
//!     load.on::<enshwick::events::WorldLoaded>(move |_| {
//!         if let Ok(coins) = price_of.call(&PriceOf { item: 7 }) {
//!             enshwick::info!("item 7 costs {coins}");
//!         }
//!     })
//! }
//! ```
//!
//! `services::calls::DescribedCaller`:
//!
//! ```
//! use enshwick::points::DescribedField;
//! use enshwick::services::DescribedCall;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let price_of = DescribedCall::new(
//!         "you.trading-post:price-of",
//!         1,
//!         (vec![DescribedField::new("item", 1)], 4),
//!         (vec![DescribedField::new("output", 2)], 4),
//!     );
//!     // SAFETY: the input is one `u32`, for which any bytes are valid.
//!     let price_of = unsafe { load.described_caller(&price_of) }?;
//!     let mut coins = [0u8; 4];
//!     let _answer = price_of.call(&7u32.to_le_bytes(), &mut coins);
//!     Ok(())
//! }
//! ```
//!
//! `services::described::DescribedEvent`:
//!
//! ```
//! use enshwick::points::DescribedField;
//! use enshwick::services::DescribedEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let sold = DescribedEvent::new("you.trading-post:sold", vec![DescribedField::new("item", 1)], 4);
//!     load.on_described_mod_event(&sold, |data| enshwick::debug!("{} bytes", data.len()))
//! }
//! ```
//!
//! `services::described::DescribedEmitter`:
//!
//! ```
//! use enshwick::points::DescribedField;
//! use enshwick::services::DescribedEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let greeted = DescribedEvent::new("you.greeter:greeted", vec![DescribedField::new("player", 1)], 8);
//!     // SAFETY: the one field is a `u64`, for which any bytes are valid.
//!     let send = unsafe { load.define_described_event(&greeted) }?;
//!     send.send(&7u64.to_le_bytes());
//!     Ok(())
//! }
//! ```
