//! A copy of each survivor at every Flame Altar: a cluster of N altars holds at
//! most N copies of a survivor, each copy stands inside a zone, works there and
//! sleeps in a bed of its own altar.
//!
//! A copy staff places a copy at an altar without one and moves the copy an
//! altar has; the survivor's own staff moves the original, as in vanilla, and
//! a copy where it arrives gives way.

mod copies;
mod cursor;
mod policy;
mod renew;
mod staffs;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::loot::Drop;
use enshwick_loader_api::{Load, Mod, Off};

/// Whether copy staffs work and drop, from the settings in force: the
/// server's on a joined client.
pub(crate) static COPY_STAFFS: AtomicBool = AtomicBool::new(true);

/// Whether the survivor's own staff places and moves copies, from the
/// settings in force.
pub(crate) static VANILLA_PLACES: AtomicBool = AtomicBool::new(false);

/// Whether the world in force keeps a copy of each survivor per altar; off,
/// every answer is vanilla.
pub(crate) static PER_ALTAR: AtomicBool = AtomicBool::new(true);

/// The copy staff drops in force: the ones the settings ask for while copy
/// staffs are on, none while they are off.
static DROPS: Mutex<Vec<Drop>> = Mutex::new(Vec::new());

const fn yes() -> bool {
    true
}

const fn boss_chance() -> f64 {
    0.10
}

const fn locked_chest_chance() -> f64 {
    0.02
}

const fn full_charges() -> u16 {
    3
}

/// Survivors per altar settings, in `mod-configs/survivors-per-altar.toml`.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// A copy of each survivor at every altar, at most one per altar zone.
    /// Off, survivors and staffs behave as in vanilla in that world.
    #[config(startup)]
    per_altar: bool,
    /// Copy staffs: one per workshop survivor. Each new copy it places
    /// spends a charge and the last charge uses the staff up; moving the
    /// copy an altar has is free. Off, they neither drop nor work, and
    /// staffs already held stay. A joined player runs with the server's
    /// value.
    #[serde(default = "yes")]
    #[config(sync)]
    copy_staffs: bool,
    /// The chance a boss's corpse holds a copy staff, from 0 to 1.
    #[serde(default = "boss_chance")]
    #[config(runtime)]
    copy_staff_boss_chance: f64,
    /// The chance a chest that needs a key or a lockpick holds one.
    #[serde(default = "locked_chest_chance")]
    #[config(runtime)]
    copy_staff_locked_chest_chance: f64,
    /// The charges a copy staff holds when full.
    #[serde(default = "full_charges")]
    #[config(restart, range = 1..=100)]
    copy_staff_charges: u16,
    /// The fewest charges a dropped staff holds, at random up to
    /// the full count; 0 drops every staff full.
    #[serde(default)]
    #[config(runtime, range = 0..=100)]
    copy_staff_charges_min: u16,
    /// Whether the survivor's own staff places a copy at an altar without
    /// one and moves the copy an altar has, as before copy staffs; off, it
    /// moves the survivor itself, as in vanilla.
    #[serde(default)]
    #[config(runtime)]
    vanilla_staff_places_copies: bool,
}

/// How many charges a staff holds when full, and the fewest a dropped one
/// holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Charges {
    pub(crate) full: u16,
    pub(crate) min: u16,
}

impl Settings {
    /// Refuses a fewest count above the full one.
    fn check(&self) -> Result<(), String> {
        if self.copy_staff_charges_min > self.copy_staff_charges {
            return Err(format!(
                "copy_staff_charges_min ({}) is above copy_staff_charges ({})",
                self.copy_staff_charges_min, self.copy_staff_charges
            ));
        }
        Ok(())
    }

    /// The staffs' charges.
    fn charges(&self) -> Charges {
        let full = self.copy_staff_charges.max(1);
        let min = match self.copy_staff_charges_min {
            0 => full,
            min => min.min(full),
        };
        Charges { full, min }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            per_altar: true,
            copy_staffs: yes(),
            copy_staff_boss_chance: boss_chance(),
            copy_staff_locked_chest_chance: locked_chest_chance(),
            copy_staff_charges: full_charges(),
            copy_staff_charges_min: 0,
            vanilla_staff_places_copies: false,
        }
    }
}

/// `chance` within 0 to 1, warning about a value outside, such as `10` typed
/// for 10 %: a setting out of range never turns the copies off.
fn chance(name: &str, chance: f64) -> f64 {
    if (0.0..=1.0).contains(&chance) {
        return chance;
    }
    let kept = if chance > 1.0 { 1.0 } else { 0.0 };
    enshwick_loader_api::warn!("{name} must be from 0 to 1, not {chance}: {kept} is used");
    kept
}

/// The mod's state.
#[derive(Debug)]
pub struct SurvivorsPerAltar;

impl Mod for SurvivorsPerAltar {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "survivors-per-altar",
            side: BOTH,
            name: "Survivors per altar",
            join = true,
            updates: github("themaiby/enshwick", "enshwick-survivors-per-altar"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.migrate(staffs::migrations())?;
        let settings: Settings = load.config()?;
        // The staffs stay in the game when they are off, so a value that
        // changes while it runs, or the server's on a joined client, finds
        // them.
        let loot = copy_staffs(load, &settings)?;
        apply(&settings, &loot);
        load.on_config_change(move |settings: Settings| apply(&settings, &loot))?;
        let staffs = policy::Staffs {
            charges: settings.charges(),
        };
        // Renewals act in the dedicated server and in the games a client
        // hosts, solo play included: the scheduler and the world's events
        // run only there.
        let server = load.host().build().side == Side::SERVER;
        if server {
            policy::link(load, staffs)?;
        }
        if let Err(off) = renew::link(load) {
            enshwick_loader_api::warn!("copy staffs from before charges stay as they are: {off}");
        }
        if !server {
            // The policy acts in the games this client hosts, and for its own
            // cursor.
            if let Err(off) = policy::link(load, staffs) {
                enshwick_loader_api::warn!("copies stay off in hosted games: {off}");
            }
            // The menu keeps one record per survivor, which every copy writes
            // its status into in turn: show the bits every copy has, the
            // worst status, so a copy without a bed shows "no bed".
            load.on_survivor_status(|_, statuses| {
                (PER_ALTAR.load(Ordering::Relaxed) && statuses.len() >= 2)
                    .then(|| statuses.iter().fold(u8::MAX, |all, s| all & s))
            })?;
        }
        Ok(Self)
    }
}

/// The copy staffs' items and full charges, set as the game starts.
#[derive(Clone, Debug)]
struct Loot {
    items: Vec<u32>,
    full: u16,
}

impl Loot {
    /// The drops `settings` ask for: one per source with a chance above 0.
    fn drops(&self, settings: &Settings) -> Vec<Drop> {
        use enshwick_loader_api::loot::Source;

        let min = settings.charges().min.min(self.full);
        let boss = chance("copy_staff_boss_chance", settings.copy_staff_boss_chance);
        let locked = chance(
            "copy_staff_locked_chest_chance",
            settings.copy_staff_locked_chest_chance,
        );
        [(boss, Source::bosses()), (locked, Source::locked_chests())]
            .into_iter()
            .filter(|(chance, _)| *chance > 0.0)
            .map(|(chance, source)| {
                Drop::one_of(&self.items, chance, source).meter(min, self.full, self.full)
            })
            .collect()
    }
}

/// Puts the copy staff settings in force: whether they work, their drops
/// and whether the survivor's own staff places copies.
fn apply(settings: &Settings, loot: &Loot) {
    let per_altar = settings.per_altar;
    if PER_ALTAR.swap(per_altar, Ordering::Relaxed) != per_altar {
        enshwick_loader_api::info!("a copy of each survivor per altar: {per_altar}");
    }
    let on = settings.copy_staffs;
    if COPY_STAFFS.swap(on, Ordering::Relaxed) != on {
        enshwick_loader_api::info!("copy staffs {}", if on { "on" } else { "off" });
    }
    let vanilla = settings.vanilla_staff_places_copies;
    if VANILLA_PLACES.swap(vanilla, Ordering::Relaxed) != vanilla {
        enshwick_loader_api::info!("the survivor's own staff places copies: {vanilla}");
    }
    let drops = if on && per_altar {
        loot.drops(settings)
    } else {
        Vec::new()
    };
    let mut kept = DROPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *kept == drops {
        return;
    }
    match enshwick_loader_api::loot::replace_drops(drops.clone()) {
        Ok(()) => {
            let chances: Vec<f64> = drops.iter().map(|d| d.chance).collect();
            enshwick_loader_api::info!("copy staff drops at {chances:?}");
            *kept = drops;
        }
        Err(off) => enshwick_loader_api::warn!("copy staff drops stay: {off}"),
    }
}

/// Adds the copy staffs, files them with their survivors and links a drop
/// for each source, so a world whose settings turn one on finds it linked;
/// [`apply`] then puts the drops the settings ask for in force.
fn copy_staffs(load: &mut Load<'_>, settings: &Settings) -> Result<Loot, Off> {
    use enshwick_loader_api::loot::Source;

    let texts = load.texts(staffs::TEXTS)?;
    let charges = settings.charges();
    staffs::add(load, &texts, charges.full)?;
    staffs::file(load)?;
    let loot = Loot {
        items: staffs::copy_ids(),
        full: charges.full,
    };
    let mut kept = DROPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for source in [Source::bosses(), Source::locked_chests()] {
        let drop =
            Drop::one_of(&loot.items, 0.0, source).meter(charges.full, charges.full, charges.full);
        if let Err(off) = load.add_loot(drop.clone()) {
            enshwick_loader_api::warn!("copy staffs drop from no loot: {off}");
            break;
        }
        kept.push(drop);
    }
    drop(kept);
    enshwick_loader_api::info!(
        "{} copy staffs of {} to {} charges",
        loot.items.len(),
        charges.min,
        charges.full,
    );
    Ok(loot)
}

enshwick_loader_api::declare_mod!(SurvivorsPerAltar);

#[cfg(test)]
mod tests {
    use super::*;

    /// A loader that knows the item layout the copy staffs need, with
    /// settings that turn copy staffs off and keep them out of loot.
    fn host() -> enshwick_loader_api::test::FakeHost {
        let mut host =
            enshwick_loader_api::test::FakeHost::new().type_size("keen::ImpactUiValue", 0x28);
        for (field, at) in [
            ("itemId", 0),
            ("objectId", 4),
            ("name", 0x48),
            ("iconImage", 0x58),
            ("description", 0x68),
            ("flags", 0x78),
            ("pidEntity", 0x80),
            ("uiValues", 0x90),
        ] {
            host = host.field_offset("keen::ItemInfo", field, at);
        }
        for (field, at) in [
            ("locaId", 0),
            ("valueFormat", 4),
            ("value", 8),
            ("isSigned", 0x24),
        ] {
            host = host.field_offset("keen::ImpactUiValue", field, at);
        }
        host.field_offset("keen::ItemRegistryResource", "itemRefs", 0)
            .field_offset("keen::ItemRegistryResource", "dbgNames", 0x38)
            .field_offset("keen::ecs::TemplateResource", "components", 0x10)
            .type_size("keen::ecs::FuelSetup", 8)
            .field_offset("keen::ecs::FuelSetup", "maxFuelCharges", 0)
            .field_offset("keen::ecs::FuelSetup", "initialFuelPercentage", 4)
            .field_offset("keen::ecs::TemplateCollectionResource", "templates", 0)
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .settings(
                "per_altar = true
copy_staffs = false
copy_staff_boss_chance = 0.0
                 copy_staff_locked_chest_chance = 0.0
",
            )
    }

    #[test]
    fn the_default_settings_parse() {
        use enshwick_loader_api::config::Config as _;

        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.per_altar && settings.copy_staffs);
        assert!((settings.copy_staff_boss_chance - 0.10).abs() < f64::EPSILON);
        let old: Settings = toml::from_str("per_altar = true\n").unwrap();
        assert!(old.copy_staffs);
        assert!((old.copy_staff_locked_chest_chance - 0.02).abs() < f64::EPSILON);
        assert_eq!(settings.charges(), Charges { full: 3, min: 3 });
        assert_eq!(old.charges(), Charges { full: 3, min: 3 });
    }

    #[test]
    fn a_world_s_chances_and_fewest_charges_make_its_drops() {
        let loot = Loot {
            items: vec![7, 8],
            full: 3,
        };
        let settings = Settings {
            copy_staff_boss_chance: 0.0,
            copy_staff_locked_chest_chance: 0.5,
            copy_staff_charges_min: 1,
            ..Settings::default()
        };
        let drops = loot.drops(&settings);
        assert_eq!(drops.len(), 1, "a chance of 0 drops nothing");
        assert!((drops[0].chance - 0.5).abs() < f64::EPSILON);
        let full = loot.drops(&Settings::default());
        assert_eq!(full.len(), 2);
        assert_ne!(drops[0].meter, full[1].meter, "the fewest charges differ");
    }

    #[test]
    fn a_joined_player_takes_the_server_s_copy_staffs() {
        use enshwick_loader_api::config::Config as _;

        let server = Settings {
            copy_staffs: false,
            ..Settings::default()
        };
        let synced = server.synced().unwrap();
        assert_eq!(synced, "{ copy_staffs = false }");
        let mut joined = Settings::default();
        joined.merge_synced(&synced).unwrap();
        assert!(!joined.copy_staffs);
    }

    #[test]
    fn a_dropped_staff_holds_from_the_fewest_to_full_charges() {
        let with = |full, min| Settings {
            copy_staff_charges: full,
            copy_staff_charges_min: min,
            ..Settings::default()
        };
        assert_eq!(with(5, 0).charges(), Charges { full: 5, min: 5 });
        assert_eq!(with(5, 2).charges(), Charges { full: 5, min: 2 });
        assert!(with(5, 2).check().is_ok());
        assert!(with(5, 6).check().is_err());
    }

    #[test]
    fn chances_stay_within_zero_to_one() {
        assert!((chance("c", 0.25) - 0.25).abs() < f64::EPSILON);
        assert!((chance("c", 10.0) - 1.0).abs() < f64::EPSILON);
        assert!((chance("c", f64::INFINITY) - 1.0).abs() < f64::EPSILON);
        assert!(chance("c", -0.5).abs() < f64::EPSILON);
        assert!(chance("c", f64::NAN).abs() < f64::EPSILON);
    }

    #[test]
    fn copies_answer_the_game_per_altar() {
        use enshwick_loader_api::abi::{
            AltarInfo, BedCheck, Bounds, Header, Status, SurvivorLookup,
        };

        let loader = host().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let altar = |entity, x| {
            let mut info = AltarInfo::EMPTY;
            info.entity = entity;
            info.zone = Bounds {
                min: [x, 0, 0],
                max: [x + 80, 80, 80],
            };
            info
        };
        loader.set_altars(&[altar(1, 0), altar(2, 100)]);
        policy::settle();
        let ask = |vanilla, purpose, entity, cell, beds: &[(u32, u32)]| {
            loader.survivor_lookup(7, vanilla, purpose, entity, cell, beds)
        };
        let unique = SurvivorLookup::UNIQUENESS;
        assert_eq!(
            ask(10, unique, 10, [10, 0, 10], &[]),
            Some(10),
            "the original stays"
        );
        assert_eq!(
            ask(10, unique, 11, [120, 0, 10], &[]),
            Some(11),
            "a copy at the other altar"
        );
        assert_eq!(
            ask(10, unique, 12, [130, 0, 10], &[]),
            Some(11),
            "a second copy goes"
        );
        assert_eq!(
            ask(10, SurvivorLookup::SPAWN, 0, [20, 0, 20], &[]),
            Some(10)
        );
        assert_eq!(
            ask(10, SurvivorLookup::BED_OWNER, 0, [150, 0, 10], &[]),
            Some(11)
        );
        assert_eq!(
            ask(
                10,
                SurvivorLookup::PREVIOUS_OCCUPANT,
                50,
                [150, 0, 10],
                &[(11, 50)]
            ),
            Some(11)
        );
        assert_eq!(ask(10, SurvivorLookup::CURSOR, 0, [0; 3], &[]), Some(0));
        let check = BedCheck {
            header: Header::of::<BedCheck>(),
            survivor: 7,
            entity: 10,
            bed: 50,
            home: [10, 0, 10],
            bed_at: [150, 0, 10],
        };
        assert!(
            !loader.survivor_bed_check(&check),
            "a bed at another altar is cleared"
        );
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_world_with_per_altar_off_answers_as_vanilla() {
        use enshwick_loader_api::abi::{
            AltarInfo, BedCheck, Bounds, Header, Status, SurvivorLookup,
        };
        use enshwick_loader_api::events::WorldSettings;

        let loader = host().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let altar = |entity, x| {
            let mut info = AltarInfo::EMPTY;
            info.entity = entity;
            info.zone = Bounds {
                min: [x, 0, 0],
                max: [x + 80, 80, 80],
            };
            info
        };
        loader.set_altars(&[altar(1, 0), altar(2, 100)]);
        policy::settle();
        let unique = SurvivorLookup::UNIQUENESS;
        assert_eq!(
            loader.survivor_lookup(7, 10, unique, 11, [120, 0, 10], &[]),
            Some(11),
            "per altar: a copy at the other altar"
        );
        loader.set_settings(
            "per_altar = false
copy_staffs = false
copy_staff_boss_chance = 0.0
copy_staff_locked_chest_chance = 0.0
",
        );
        assert!(loader.raise::<WorldSettings>(&()) >= 1);
        assert!(!PER_ALTAR.load(Ordering::Relaxed));
        assert_eq!(
            loader.survivor_lookup(7, 10, unique, 11, [120, 0, 10], &[]),
            None,
            "the world's per_altar is off: the game's own answer"
        );
        let check = BedCheck {
            header: Header::of::<BedCheck>(),
            survivor: 7,
            entity: 10,
            bed: 50,
            home: [10, 0, 10],
            bed_at: [150, 0, 10],
        };
        assert!(loader.survivor_bed_check(&check), "vanilla keeps the bed");
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn old_staffs_are_renewed_in_a_hosted_game_too() {
        use enshwick_loader_api::abi::Status;
        use enshwick_loader_api::events::WorldLoaded;

        // What the inventories need; no player and no container is
        // read.
        let loader = host()
            .side(Side::CLIENT)
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.skip_entity", 0x1_4000_0000)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .type_hash("keen::ecs::InventorySetup", 3)
            .type_hash("keen::ecs::Inventory", 4)
            .type_hash("keen::ecs::NetworkDurability", 5)
            .type_hash("keen::ecs::NetworkFuel", 6)
            .type_hash("keen::ecs::OwnerRelationship", 7)
            .field_offset("keen::ecs::InventorySetup", "linksEntities", 0)
            .field_offset("keen::ecs::InventorySetup", "linksCategories", 0x40)
            .field_offset("keen::ecs::InventorySetup", "isInitialized", 0x53)
            .field_offset("keen::ItemStack", "data", 4)
            .field_offset("keen::ecs::NetworkDurability", "durabilityMax", 2)
            .field_offset("keen::ecs::NetworkFuel", "fuelMax", 2)
            .type_size("keen::ItemStack", 12)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1, "{:?}", loader.logs());
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn copy_staffs_take_the_survivor_s_name_in_the_game_s_language() {
        use enshwick_loader_api::abi::Status;
        use enshwick_loader_api::content::{Guid, types};
        use enshwick_loader_api::i18n::Language;
        use enshwick_loader_api::survivors::WorkshopSurvivor;

        let tag = Guid::parse("ddda8053-f1f5-49fe-aab9-2a6cd460dc47");
        let mut info = vec![0u8; 0x60];
        info[0x48..0x58].copy_from_slice(&tag.0);
        let alchemist = WorkshopSurvivor::find("alchemist").unwrap();
        let loader = host()
            .side(Side::CLIENT)
            .settings(
                "per_altar = true
copy_staff_boss_chance = 0.0
copy_staff_locked_chest_chance = 0.0
",
            )
            .resource(alchemist.staff().key(types::ITEM_INFO), info)
            .game_text(tag.id(), Language::EN_US, "Balthazar")
            .game_text(tag.id(), Language::UK_UA, "Бальтазар")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let name = staffs::STAFFS
            .iter()
            .find(|s| s.key == "alchemist")
            .unwrap()
            .name
            .id();
        let texts = loader.texts();
        let text = |language: Language| {
            texts
                .iter()
                .find(|(id, l, _)| *id == name && *l == language.0)
                .map(|(_, _, text)| text.as_str())
        };
        assert_eq!(text(Language::UK_UA), Some("Посох копії: Бальтазар"));
        assert_eq!(text(Language::FALLBACK), Some("Copy staff: Balthazar"));
        assert_eq!(text(Language::DE_DE), Some("Kopiestab: Balthazar"));
    }

    #[test]
    fn the_survivor_s_own_staff_moves_the_original_and_its_copy_gives_way() {
        use enshwick_loader_api::abi::{AltarInfo, Bounds, Header, Status, Summon, SurvivorLookup};
        use enshwick_loader_api::survivors::WORKSHOP_SURVIVORS;

        let loader = host().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let altar = |entity, x| {
            let mut info = AltarInfo::EMPTY;
            info.entity = entity;
            info.zone = Bounds {
                min: [x, 0, 0],
                max: [x + 80, 80, 80],
            };
            info
        };
        loader.set_altars(&[altar(1, 0), altar(2, 100), altar(3, 200)]);
        policy::settle();
        let smith = &WORKSHOP_SURVIVORS[0];
        let survivor = smith.id().0;
        let ask = |purpose, entity, cell| {
            loader.survivor_lookup(survivor, 10, purpose, entity, cell, &[])
        };
        let unique = SurvivorLookup::UNIQUENESS;
        // The original at altar 1, copies at altars 2 and 3.
        assert_eq!(ask(unique, 10, [10, 0, 10]), Some(10));
        assert_eq!(ask(unique, 11, [120, 0, 10]), Some(11));
        assert_eq!(ask(unique, 12, [220, 0, 10]), Some(12));
        // The survivor's own staff aimed at altar 2.
        let summon = Summon {
            header: Header::of::<Summon>(),
            item: smith.staff().id(),
            ammo: 0,
            template: smith.template().0,
            target: [130, 0, 10],
            slot: 0,
            pide: 0,
            meter: [0; 2],
            consumed: 0,
            reserved: [0; 3],
        };
        assert_eq!(loader.summon(&summon), 0, "allowed, nothing taken");
        assert_eq!(
            ask(SurvivorLookup::SPAWN, 0, [130, 0, 10]),
            Some(10),
            "the original moves, not the copy there"
        );
        // The original now stands at altar 2: the copy there gives way, the
        // one at altar 3 stays.
        assert_eq!(ask(unique, 10, [130, 0, 10]), Some(10));
        assert_eq!(
            ask(unique, 11, [120, 0, 10]),
            Some(10),
            "copy 11 is removed"
        );
        assert_eq!(ask(unique, 12, [220, 0, 10]), Some(12));
        assert!(loader.faults().is_empty());
    }
}
