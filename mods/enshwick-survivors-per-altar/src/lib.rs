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

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_loader_api::abi::Side;
use enshwick_loader_api::loot::Drop;
use enshwick_loader_api::{Load, Mod, Off};

/// Whether copy staffs work and drop, from the settings in force: the
/// server's on a joined client.
pub(crate) static COPY_STAFFS: AtomicBool = AtomicBool::new(true);

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
    #[config(startup)]
    copy_staff_boss_chance: f64,
    /// The chance a chest that needs a key or a lockpick holds one.
    #[serde(default = "locked_chest_chance")]
    #[config(startup)]
    copy_staff_locked_chest_chance: f64,
    /// The charges a copy staff holds when full.
    #[serde(default = "full_charges")]
    #[config(startup, range = 1..=100)]
    copy_staff_charges: u16,
    /// The fewest charges a dropped staff holds, at random up to
    /// the full count; 0 drops every staff full.
    #[serde(default)]
    #[config(startup, range = 0..=100)]
    copy_staff_charges_min: u16,
    /// Whether the survivor's own staff places a copy at an altar without
    /// one and moves the copy an altar has, as before copy staffs; off, it
    /// moves the survivor itself, as in vanilla.
    #[serde(default)]
    #[config(startup)]
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
        if !settings.per_altar {
            return Err(Off::new("per_altar is off in the settings"));
        }
        // The staffs stay in the game when they are off, so a value that
        // changes while it runs, or the server's on a joined client, finds
        // them.
        let drops = copy_staffs(load, &settings)?;
        apply_copy_staffs(settings.copy_staffs, &drops);
        load.on_config_change(move |settings: Settings| {
            apply_copy_staffs(settings.copy_staffs, &drops);
        })?;
        let staffs = policy::Staffs {
            charges: settings.charges(),
            vanilla_places: settings.vanilla_staff_places_copies,
        };
        if load.host().build().side == Side::SERVER {
            policy::link(load, staffs)?;
            if let Err(off) = renew::link(load) {
                enshwick_loader_api::warn!(
                    "copy staffs from before charges stay as they are: {off}"
                );
            }
        } else {
            // The policy acts in the games this client hosts, and for its own
            // cursor.
            if let Err(off) = policy::link(load, staffs) {
                enshwick_loader_api::warn!("copies stay off in hosted games: {off}");
            }
            // The menu keeps one record per survivor, which every copy writes
            // its status into in turn: show the bits every copy has, the
            // worst status, so a copy without a bed shows "no bed".
            load.on_survivor_status(|_, statuses| {
                (statuses.len() >= 2).then(|| statuses.iter().fold(u8::MAX, |all, s| all & s))
            })?;
        }
        Ok(Self)
    }
}

/// Turns the copy staffs on or off: whether they work, and their drops.
fn apply_copy_staffs(on: bool, drops: &[Drop]) {
    if COPY_STAFFS.swap(on, Ordering::Relaxed) == on {
        return;
    }
    let kept = if on { drops.to_vec() } else { Vec::new() };
    match enshwick_loader_api::loot::replace_drops(kept) {
        Ok(()) => enshwick_loader_api::info!("copy staffs {}", if on { "on" } else { "off" }),
        Err(off) => enshwick_loader_api::warn!("copy staffs {on}, their drops stay: {off}"),
    }
}

/// Adds the copy staffs, files them with their survivors and puts them in
/// loot; the drops it added.
fn copy_staffs(load: &mut Load<'_>, settings: &Settings) -> Result<Vec<Drop>, Off> {
    use enshwick_loader_api::loot::Source;

    let texts = load.texts(staffs::TEXTS)?;
    let charges = settings.charges();
    staffs::add(load, &texts, charges.full)?;
    staffs::file(load)?;
    let items = staffs::copy_ids();
    let boss = chance("copy_staff_boss_chance", settings.copy_staff_boss_chance);
    let locked = chance(
        "copy_staff_locked_chest_chance",
        settings.copy_staff_locked_chest_chance,
    );
    let mut drops = Vec::new();
    for (chance, source) in [(boss, Source::bosses()), (locked, Source::locked_chests())] {
        if chance > 0.0 {
            let drop =
                Drop::one_of(&items, chance, source).meter(charges.min, charges.full, charges.full);
            load.add_loot(drop.clone())?;
            drops.push(drop);
        }
    }
    enshwick_loader_api::info!(
        "{} copy staffs of {} to {} charges; in loot from bosses at {}, locked chests at {}",
        items.len(),
        charges.min,
        charges.full,
        boss,
        locked
    );
    Ok(drops)
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
