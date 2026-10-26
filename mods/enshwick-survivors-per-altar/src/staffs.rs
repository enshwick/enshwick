//! Copy staffs: one item per workshop survivor, a clone of the survivor's
//! summoning staff with charges of its own.
//!
//! The platform files each copy staff as a staff of its survivor, so the game
//! treats it as the vanilla staff on the server and the client. Each staff
//! keeps its charges in its meter, which the game saves and draws as a bar.
//! The policy spends one per use that works and marks the use of the last
//! charge as consuming, which makes the game take the staff in the same
//! transaction as the spawn or move, and only when that works.

use enshwick::content::icons::{Icon, Pixels, Tint};
use enshwick::content::items::{Meter, NewItem, Stat, StatFormat};
use enshwick::content::{Guid, Id};
use enshwick::i18n::{Language, Texts};
use enshwick::migrations::Migrations;
use enshwick::survivors::WorkshopSurvivor;
use enshwick::{Load, Off};

/// The mod's texts, per language.
pub(crate) const TEXTS: &str = include_str!("texts.toml");

/// The template of every copy staff's own entity, which keeps its charges.
const STAFF_ENTITY: Id = Id::new("survivors-per-altar:copy-staff.entity");
/// The GUID the template had before content ids; saved staffs name it.
const OLD_STAFF_ENTITY: Guid = Guid::parse("79250fcb-6170-4767-bfbb-2745e36e905a");
/// The colour copy staffs' icons take, so they stand apart from the
/// survivors' own staffs.
const ICON_TINT: Tint = Tint {
    colour: [150, 110, 255],
    amount: 55,
};
/// The tag of the tooltip line with a staff's full charges.
const FULL_CHARGES: Id = Id::new("survivors-per-altar:copy-staff.full-charges");

/// A workshop survivor and its copy staff.
#[derive(Debug)]
pub(crate) struct Staff {
    /// The survivor's key in the platform's table.
    pub key: &'static str,
    /// The copy staff.
    pub copy: Id,
    /// The tag of the copy staff's name.
    pub name: Id,
    /// The tag of its description.
    pub description: Id,
    /// Its icon: the vanilla staff's, recoloured.
    pub icon: Id,
    /// The GUID the copy staff had before content ids; saved stacks keep it.
    pub old: Guid,
}

impl Staff {
    /// The survivor the staff copies.
    pub(crate) fn survivor(&self) -> Option<&'static WorkshopSurvivor> {
        WorkshopSurvivor::find(self.key)
    }

    fn survivor_or_off(&self) -> Result<&'static WorkshopSurvivor, Off> {
        self.survivor()
            .ok_or_else(|| Off::new(format!("no workshop survivor {:?}", self.key)))
    }
}

macro_rules! staffs {
    ($($key:literal, $old:literal;)*) => {
        /// A copy staff for every workshop survivor; the ids are the mod's own.
        pub(crate) const STAFFS: &[Staff] = &[$(Staff {
            key: $key,
            copy: Id::new(concat!("survivors-per-altar:copy-staff/", $key)),
            name: Id::new(concat!("survivors-per-altar:copy-staff/", $key, ".name")),
            description: Id::new(concat!("survivors-per-altar:copy-staff/", $key, ".description")),
            icon: Id::new(concat!("survivors-per-altar:copy-staff/", $key, ".icon")),
            old: Guid::parse($old),
        }),*];
    };
}

staffs! {
    "blacksmith", "dac11dad-687c-4397-b528-fdec7cdb6ba1";
    "blacksmith_assistant", "10b19c0f-b86a-4bb2-96c6-5dfef7e996d3";
    "alchemist", "448e4f2a-d620-41ca-bfc6-f311320779e5";
    "alchemist_assistant", "bbecddee-022a-4506-bde1-ffe07d1b6727";
    "carpenter", "177d03e4-ac34-439a-b342-fee6973629f6";
    "carpenter_assistant", "f54ca128-3ed7-4e30-bb93-6f82d8fc0d29";
    "hunter", "8057fdfe-c990-4338-8a8d-a850a12458ab";
    "hunter_assistant", "a3bca904-6d54-4c66-a11b-8fc0f52daba5";
    "farmer", "42f978ea-d349-4f44-8796-b61bebaa7123";
    "farmer_assistant", "d5bf7763-a011-4ad0-af9b-09b67e1f5a91";
    "bard", "afbaccbe-d61d-4997-84d8-9e2c69359ee8";
    "bard_assistant", "17dfc67d-1749-4318-88f7-6aa1c48064f2";
    "crypt_keeper", "4e56d680-2fe3-497b-ad1c-40a8306b2923";
    "crypt_keeper_assistant", "9d99fcc0-7431-41a3-a1e3-9f37cd5ba8a7";
    "barber", "de8e029e-5be3-4ad9-ad7d-817538046f9a";
    "researcher", "5eaf09fd-a724-4d8e-93f4-3bf9e3395e83";
    "trader", "6181d065-0d20-4361-ab4f-2622ceefa542";
    "fisher", "b19d25af-3283-416e-8173-9506ddd8f26e";
}

/// The copy staff with the game id `item`, its own or the one it had
/// before.
pub(crate) fn copy_staff(item: u32) -> Option<&'static Staff> {
    let item = enshwick::migrations::current_id(item);
    STAFFS.iter().find(|s| s.copy.id() == item)
}

/// The game ids of every copy staff.
pub(crate) fn copy_ids() -> Vec<u32> {
    STAFFS.iter().map(|s| s.copy.id()).collect()
}

/// The game ids of every copy staff, with the ids saved stacks may hold.
pub(crate) fn known_ids() -> Vec<u32> {
    STAFFS
        .iter()
        .flat_map(|s| [s.copy.id(), s.old.id()])
        .collect()
}

/// Every migration of the copy staffs, oldest first: the GUIDs they had
/// before content ids.
pub(crate) fn migrations() -> Migrations {
    Migrations::new().add("0.5.0", |mut m| {
        m = m.rename_content(OLD_STAFF_ENTITY, STAFF_ENTITY.guid());
        for staff in STAFFS {
            m = m.rename_content(staff.old, staff.copy.guid());
        }
        m
    })
}

/// `template` with `{survivor}` filled in.
fn fill(template: &str, survivor: &str) -> String {
    template.replace("{survivor}", survivor)
}

/// The survivor's name in `language` as the game shows it; without the
/// game's texts (on a server), its role from its key, such as "Blacksmith
/// assistant".
fn survivor_name(staff: &Staff, language: Language) -> String {
    if let Some(name) = staff.survivor().and_then(|s| s.name(language)) {
        return name.to_owned();
    }
    let role = staff.key.replace('_', " ");
    let mut chars = role.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Adds every copy staff and its texts; a full staff holds `charges`.
pub(crate) fn add(load: &mut Load<'_>, texts: &Texts, charges: u16) -> Result<(), Off> {
    load.add_texts(texts, &[("copy.full_charges", FULL_CHARGES)])?;
    let meter = Meter {
        template: STAFF_ENTITY,
        max: charges,
        start: charges,
    };
    let full = Stat {
        label: FULL_CHARGES.guid(),
        value: i32::from(charges),
        format: StatFormat::Number,
    };
    for staff in STAFFS {
        let vanilla = staff.survivor_or_off()?.staff();
        for (template_key, tag) in [
            ("copy.name", staff.name),
            ("copy.description", staff.description),
        ] {
            let tag = load.declare(tag)?;
            let translations: Vec<_> = texts.translations(template_key).collect();
            if translations.is_empty() {
                return Err(Off::new(format!("texts: no text for {template_key:?}")));
            }
            for (language, template) in translations {
                let survivor = survivor_name(staff, language);
                load.add_text(tag, language, &fill(template, &survivor))?;
            }
        }
        load.add_item(NewItem {
            name: Some(staff.name.guid()),
            description: Some(staff.description.guid()),
            meter: Some(meter),
            stats: vec![full],
            icon: Some(Icon {
                id: staff.icon,
                like: vanilla,
                pixels: Pixels::Tint(ICON_TINT),
            }),
            debug_name: format!("SurvivorsPerAltar_CopyStaff_{}", staff.key),
            ..NewItem::new(staff.copy, vanilla)
        })?;
    }
    Ok(())
}

/// Files every copy staff as a staff of its survivor, beside its vanilla
/// staff, so the cursor, the unlock check and the placement treat both alike.
pub(crate) fn file(load: &mut Load<'_>) -> Result<(), Off> {
    for staff in STAFFS {
        let survivor = staff.survivor_or_off()?.id();
        load.add_summon_item(staff.copy.id(), survivor)?;
        load.add_summon_item(staff.old.id(), survivor)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick::survivors::WORKSHOP_SURVIVORS;

    #[test]
    fn every_staff_has_its_own_ids() {
        let mut ids = std::collections::BTreeSet::new();
        for staff in STAFFS {
            for id in [staff.copy, staff.name, staff.description, staff.icon] {
                assert!(ids.insert(id.id()), "{}", staff.key);
            }
            assert!(ids.insert(staff.old.id()), "{}", staff.key);
            assert_eq!(
                copy_staff(staff.old.id()).map(|s| s.key),
                None,
                "only once the migration is declared"
            );
            let survivor = staff.survivor().unwrap();
            assert_ne!(staff.copy.id(), survivor.staff().id());
            assert_eq!(copy_staff(staff.copy.id()).map(|s| s.key), Some(staff.key));
            assert_eq!(copy_staff(survivor.staff().id()).map(|s| s.key), None);
        }
        assert_eq!(STAFFS.len(), WORKSHOP_SURVIVORS.len());
    }

    #[test]
    fn without_the_game_s_texts_a_survivor_is_named_by_its_role() {
        let smith = STAFFS
            .iter()
            .find(|s| s.key == "blacksmith_assistant")
            .unwrap();
        assert_eq!(
            survivor_name(smith, Language::UK_UA),
            "Blacksmith assistant"
        );
    }

    #[test]
    fn names_fill_in_the_survivor_in_every_language() {
        let texts = Texts::parse(TEXTS).unwrap();
        for language in [
            Language::FALLBACK,
            Language::UK_UA,
            Language::DE_DE,
            Language::FR_FR,
            Language::ES_ES,
            Language::PL_PL,
        ] {
            for key in ["copy.name", "copy.description"] {
                let template = texts.get(key, language).unwrap();
                assert!(template.contains("{survivor}"), "{key} {language:?}");
                assert!(!fill(template, "Balthazar").contains('{'));
            }
            assert!(
                texts
                    .translations("copy.full_charges")
                    .any(|(l, _)| l == language),
                "{language:?}"
            );
        }
    }
}
