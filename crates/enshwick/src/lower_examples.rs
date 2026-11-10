//! The examples of the internal crates that name items above their crate.
//! Each stays in its item's docs there, outside that crate's own doctests,
//! and is compiled here, as written. Only `cargo test --doc` reads this
//! module.

/// `content::Guid`:
///
/// ```
/// use enshwick::content::Guid;
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// assert_eq!(enshwick::ItemId::from_guid(CHEST).get(), 1_302_084_846);
/// ```
///
/// `content::Guid::key`:
///
/// ```
/// use enshwick::content::{Guid, types};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
///     assert_eq!(chest.key(types::ITEM_INFO)?.part, 0);
///     Ok(())
/// }
/// ```
///
/// `content::Id::key`:
///
/// ```
/// use enshwick::content::{Id, types};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let key = Id::new("you.lockers:locker").key(types::ITEM_INFO)?;
///     assert_eq!(key.guid, Id::new("you.lockers:locker").guid().0);
///     Ok(())
/// }
/// ```
///
/// `content::ResourceType`:
///
/// ```
/// use enshwick::content::types;
///
/// assert_eq!(types::ITEM_INFO.id(), "item_info");
/// assert!(types::ITEM_INFO.hash().is_err(), "outside the game");
/// ```
///
/// `Off`:
///
/// ```
/// let off = enshwick::Off::new("this game build is not supported");
/// assert_eq!(off.to_string(), "this game build is not supported");
///
/// fn calls(load: &enshwick::Load<'_>) -> Result<usize, enshwick::Off> {
///     let pattern = enshwick::code::Pattern::parse("E8 ?? ?? ?? ??")?;
///     Ok(pattern.find_all(load.code()?).count())
/// }
/// ```
///
/// `Off::text`:
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let _texts = load.texts_from("[en]\n\"glow.off\" = \"No glow on this game version.\"\n")?;
///     let glow = load.feature("glow", |load| {
///         load.symbol("lantern.glow").map_err(|_| enshwick::Off::text("glow.off", &[]))
///     });
///     let _ = glow;
///     Ok(())
/// }
///
/// let off = enshwick::Off::text("stalls.min", &[("min", "2")]);
/// assert_eq!(off.to_string(), "stalls.min (min = 2)");
/// ```
///
/// `script::Function`:
///
/// ```rust,standalone_crate
/// use enshwick::script::{Bound, Value};
///
/// #[derive(Clone, Copy, Debug)]
/// pub struct Dice;
///
/// #[enshwick::script(rolls)]
/// impl Dice {
///     /// The sum of `count` dice of six sides, all showing four.
///     #[script]
///     pub fn roll(self, count: u32) -> u32 {
///         count * 4
///     }
/// }
///
/// impl Bound for Dice {
///     const NAME: &'static str = "dice";
///     const DOC: &'static str = "Dice.";
///     const PARTS: &'static [&'static [enshwick::script::Function<Self>]] = &[Self::SCRIPT_ROLLS];
///
///     fn resolve(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
///         Ok(Self)
///     }
/// }
///
/// let roll = &Dice::SCRIPT_ROLLS[0];
/// assert_eq!(roll.name(), "roll");
/// assert_eq!(roll.signature().params()[0].name(), "count");
/// let Ok(Value::Number(sum)) = roll.call(&Dice, &mut [Value::Number(2.0)]) else { unreachable!() };
/// assert_eq!(sum, 8.0);
/// ```
mod values {}
