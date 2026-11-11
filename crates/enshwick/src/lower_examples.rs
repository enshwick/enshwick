//! The examples of the internal crates that name items above their crate.
//! Each stays in its item's docs there, outside that crate's own doctests,
//! and is compiled here, as written: below, and in a file per crate and
//! feature in the folder of this module. Only `cargo test --doc` reads this
//! module.

mod base;
#[cfg(feature = "unstable")]
mod base_unstable;

/// `content::Guid`:
///
/// ```
/// use enshwick::content::Guid;
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// assert_eq!(enshwick::ItemId::from_guid(CHEST).get(), 1_302_084_846);
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
mod values {}

/// `abi::BuffTypeInfo`:
///
/// ```
/// use enshwick::abi::{BuffTypeInfo, Header};
///
/// let rested = BuffTypeInfo {
///     header: Header::of::<BuffTypeInfo>(),
///     id: enshwick::buffs::BuffTypeId::new(0xcdbe_3e80),
///     slot: BuffTypeInfo::RESTED,
///     flags: BuffTypeInfo::SAVED,
///     name: 1_237_250_642,
///     duration: 5_000_000_000,
/// };
/// assert_eq!(rested.slot, BuffTypeInfo::RESTED);
/// ```
///
/// `abi::ActiveBuff`:
///
/// ```
/// use enshwick::abi::{ActiveBuff, Header};
///
/// let fed = ActiveBuff {
///     header: Header::of::<ActiveBuff>(),
///     id: enshwick::buffs::BuffTypeId::new(7),
///     entity: enshwick::Entity(4100),
///     slot: 0,
///     level: 0,
///     duration: 600_000_000_000,
///     remaining: 120_000_000_000,
/// };
/// assert!(fed.remaining < fed.duration);
/// ```
///
/// `abi::BuffRequest`:
///
/// ```
/// use enshwick::abi::{BuffRequest, Header};
///
/// let request = BuffRequest {
///     header: Header::of::<BuffRequest>(),
///     entity: enshwick::Entity(2),
///     id: enshwick::buffs::BuffTypeId::new(7),
///     level: 0,
///     reserved: 0,
///     duration: 0,
/// };
/// assert_eq!(request.duration, 0, "the type's own duration");
/// ```
///
/// `abi::KnowledgeData`:
///
/// ```
/// use enshwick::abi::{Header, KnowledgeData};
///
/// let learnt = KnowledgeData { header: Header::of::<KnowledgeData>(), entity: enshwick::Entity(2), id: enshwick::knowledge::KnowledgeId::new(0x1d), value: 3 };
/// assert_eq!(learnt.value, 3);
/// ```
///
/// `abi::QuestData`:
///
/// ```
/// use enshwick::abi::{Header, QuestData};
///
/// let done = QuestData { header: Header::of::<QuestData>(), entity: enshwick::Entity(2), quest: enshwick::knowledge::KnowledgeId::new(0x51) };
/// assert_eq!(done.entity, enshwick::Entity(2));
/// ```
///
/// `abi::KnowledgeInput`:
///
/// ```
/// use enshwick::abi::{Header, KnowledgeInfo, KnowledgeInput};
///
/// let seen = KnowledgeInput {
///     header: Header::of::<KnowledgeInput>(),
///     player: enshwick::Entity(2),
///     id: enshwick::knowledge::KnowledgeId::new(0x1d),
///     kind: KnowledgeInfo::FLAGS,
///     scope: KnowledgeInfo::PLAYER,
///     reserved: 0,
///     value: 0,
///     refusal: KnowledgeInput::ALLOWED,
///     allowed: 1,
/// };
/// assert_eq!(seen.allowed, 1);
/// ```
///
/// `abi::KnowledgeId`:
///
/// ```
/// use enshwick::RecipeId;
/// use enshwick::knowledge::KnowledgeId;
///
/// let quest = KnowledgeId::new(0x51);
/// assert_eq!(quest.get(), 0x51);
/// assert_eq!(KnowledgeId::from(RecipeId::new(0x1d)), KnowledgeId::new(0x1d));
/// ```
///
/// `abi::BuffTypeId`:
///
/// ```
/// use enshwick::{buffs::BuffTypeId, content::id};
///
/// const WARMTH: BuffTypeId = BuffTypeId::from_guid(id!("warmth").guid());
/// assert_eq!(WARMTH.to_string(), WARMTH.get().to_string());
/// ```
///
/// `abi::NpcId`:
///
/// ```
/// use enshwick::survivors::NpcId;
///
/// let blacksmith = NpcId::new(1_201_801_546);
/// assert_eq!(blacksmith.get(), 1_201_801_546);
/// ```
///
/// `abi::SurvivorLookup`:
///
/// ```
/// use enshwick::abi::{Header, SurvivorLookup};
///
/// let lookup = SurvivorLookup {
///     header: Header::of::<SurvivorLookup>(),
///     survivor: enshwick::survivors::NpcId::new(0x47a1_5e4a),
///     vanilla: enshwick::Entity(4096),
///     purpose: SurvivorLookup::SPAWN,
///     entity: enshwick::Entity(0),
///     cell: [10, 0, 10],
///     bed_of: None,
///     token: core::ptr::null_mut(),
/// };
/// assert_eq!(lookup.purpose, SurvivorLookup::SPAWN);
/// ```
///
/// `abi::BedCheck`:
///
/// ```
/// use enshwick::abi::{BedCheck, Header};
///
/// let check = BedCheck { header: Header::of::<BedCheck>(), npc_id: enshwick::survivors::NpcId::new(7), entity: enshwick::Entity(4096), bed_id: enshwick::Entity(4100), home: [0; 3], bed_at: [4, 0, 4] };
/// assert_eq!(check.bed_id, enshwick::Entity(4100));
/// ```
///
/// `abi::KnowledgeInfo`:
///
/// ```
/// use enshwick::abi::{Header, KnowledgeInfo};
///
/// let recipe = KnowledgeInfo {
///     header: Header::of::<KnowledgeInfo>(),
///     id: enshwick::knowledge::KnowledgeId::new(0x1d),
///     kind: KnowledgeInfo::FLAGS,
///     scope: KnowledgeInfo::PLAYER,
///     reserved: 0,
///     value: 1,
///     flags: 3,
/// };
/// assert_eq!(recipe.value & 1, 1);
/// ```
mod abi {}

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
#[cfg(feature = "unstable")]
mod unstable_values {}

/// `content::items::FuelSetup::change`:
///
/// ```
/// use enshwick::content::id;
/// use enshwick::content::items::FuelSetup;
///
/// let host = enshwick::test::FakeHost::new().meters();
/// let meter = FuelSetup { template: id!("charges"), max_fuel_charges: 4, start: 1 };
/// assert_eq!(meter.change(&host)?.component(), "fuel_setup");
/// # Ok::<(), enshwick::Off>(())
/// ```
///
/// `content::workshops::at_workshop`:
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::workshops::{WorkshopId, at_workshop};
///
/// let host = enshwick::test::FakeHost::new().workshops();
/// let changes = at_workshop(&host, WorkshopId::new(Guid::named("you.mod:bench.workshop")))?;
/// assert_eq!(changes[0].component(), "crafting");
/// # Ok::<(), enshwick::Off>(())
/// ```
///
/// `stats::Stats::info`:
///
/// ```rust,standalone_crate
/// use std::sync::OnceLock;
///
/// use enshwick::content::id;
/// use enshwick::stats::{Stat, StatDef, Stats};
/// use enshwick::{Load, Mod, Off};
///
/// static LUCK: OnceLock<(Stats, Stat)> = OnceLock::new();
///
/// struct Lucky;
///
/// impl Mod for Lucky {
///     fn init(load: &mut Load<'_>) -> Result<Self, Off> {
///         let luck = load.define_stat(id!("luck"), &StatDef::additive())?;
///         let _first = LUCK.set((load.stats()?, luck));
///         Ok(Self)
///     }
/// }
///
/// enshwick::test::declare_mod!(Lucky, "id = \"you.hello\"\nside = \"server\"");
///
/// let loader = enshwick::test::FakeHost::new().install();
/// // SAFETY: both functions are the exports of `declare_mod!(Lucky)`.
/// let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
/// assert!(status.is_ok());
/// let (stats, luck) = LUCK.get().ok_or("init ran")?;
/// let info = stats.info(*luck).ok_or("luck is defined")?;
/// assert_eq!(info.id, "you.hello:luck");
/// assert_eq!(info.text, "luck.name");
/// # Ok::<(), &str>(())
/// ```
#[cfg(feature = "test")]
mod fake {}
