//! Copy staffs: one item per workshop survivor, a clone of the survivor's
//! summoning staff with charges of its own.
//!
//! The platform files each copy staff as a staff of its survivor, so the game
//! treats it as the vanilla staff on the server and the client. Each staff
//! keeps its charges in its meter, which the game saves and draws as a bar.
//! The policy spends one per use that works and marks the use of the last
//! charge as consuming, which makes the game take the staff in the same
//! transaction as the spawn or move, and only when that works.

use enshwick_loader_api::content::Guid;
use enshwick_loader_api::content::icons::{Icon, Pixels, Tint};
use enshwick_loader_api::content::items::{Meter, NewItem, Stat, StatFormat};
use enshwick_loader_api::i18n::Texts;
use enshwick_loader_api::survivors::WorkshopSurvivor;
use enshwick_loader_api::{Load, Off};

/// The mod's texts, per language.
pub(crate) const TEXTS: &str = include_str!("texts.toml");

/// The template of every copy staff's own entity, which keeps its charges.
const STAFF_ENTITY: Guid = Guid::parse("79250fcb-6170-4767-bfbb-2745e36e905a");
/// The colour copy staffs' icons take, so they stand apart from the
/// survivors' own staffs.
const ICON_TINT: Tint = Tint {
    colour: [150, 110, 255],
    amount: 55,
};
/// The tag of the tooltip line with a staff's full charges.
const FULL_CHARGES: Guid = Guid::parse("f8146c9e-74b6-4b65-8a02-2975f56d5882");

/// A workshop survivor and its copy staff.
#[derive(Debug)]
pub(crate) struct Staff {
    /// The text key of the survivor's name, after `survivor.`, and its key in
    /// the platform's table.
    pub key: &'static str,
    /// The copy staff.
    pub copy: Guid,
    /// The tag of the copy staff's name.
    pub name: Guid,
    /// The tag of its description.
    pub description: Guid,
    /// Its icon: the vanilla staff's, recoloured.
    pub icon: Guid,
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
    ($($key:literal, $copy:literal, $name:literal, $description:literal, $icon:literal;)*) => {
        /// A copy staff for every workshop survivor; the ids are the mod's own.
        pub(crate) const STAFFS: &[Staff] = &[$(Staff {
            key: $key,
            copy: Guid::parse($copy),
            name: Guid::parse($name),
            description: Guid::parse($description),
            icon: Guid::parse($icon),
        }),*];
    };
}

staffs! {
    "blacksmith", "dac11dad-687c-4397-b528-fdec7cdb6ba1", "438a229b-d221-4d69-8a76-acc252628fa3", "5f0fed42-fbff-4811-8516-ed774b5a7232", "7170314f-3f43-4530-b0c4-0a4885e4de6a";
    "blacksmith_assistant", "10b19c0f-b86a-4bb2-96c6-5dfef7e996d3", "1b6a3f21-d2df-4649-a536-766485c89d28", "33316d27-428d-460c-9e42-0192aadc20bf", "c42ad406-8df5-40d5-9d32-e106ac72f1f0";
    "alchemist", "448e4f2a-d620-41ca-bfc6-f311320779e5", "b12bb3d7-5ce4-4f7b-89fe-3e38747abbce", "e7f7f180-0aae-45b0-9452-3b4266b78d74", "b7c71732-b4a4-47d8-91a2-9f56b2db414d";
    "alchemist_assistant", "bbecddee-022a-4506-bde1-ffe07d1b6727", "f6968340-15f8-4ac7-b521-e31f713d2a26", "13bd532e-f26f-4aa8-a2d4-e4201453a98a", "88656837-0380-4e15-8d4b-421d962f0055";
    "carpenter", "177d03e4-ac34-439a-b342-fee6973629f6", "dfb2ca2c-4e27-451b-8787-7728a71997c2", "973390fd-a203-4480-9a73-57941a778503", "08e21cba-7c53-41f0-bbdb-29ef17898c25";
    "carpenter_assistant", "f54ca128-3ed7-4e30-bb93-6f82d8fc0d29", "bc6902e1-3221-4620-a4bc-afcb03364bc3", "5a1efc91-aee9-4bd4-a047-6d4a9c45f364", "af76e72f-c6c4-4548-8584-45f68ee31a71";
    "hunter", "8057fdfe-c990-4338-8a8d-a850a12458ab", "44f1e095-af4b-4565-bf85-c43e79aca035", "fa2519ec-7ce6-4d76-9668-70fd34e35297", "ac417309-09da-41c1-bb2d-18ebd7a81838";
    "hunter_assistant", "a3bca904-6d54-4c66-a11b-8fc0f52daba5", "a8922651-5118-4d88-bb12-f9b1f3655abd", "65700ddb-d554-4ea8-b08b-ede01576c538", "7a440797-71b0-4e4b-b5cf-b7b97414f775";
    "farmer", "42f978ea-d349-4f44-8796-b61bebaa7123", "08ced2ca-8a7d-4b73-a71e-286a1da25195", "547ee06c-e76b-4a1d-9835-f2b9af08b4e6", "f0ba517b-ac88-47c8-9c4f-35d2def738fa";
    "farmer_assistant", "d5bf7763-a011-4ad0-af9b-09b67e1f5a91", "f7e2a0b6-ebf2-4d64-8c80-9c6860fd78b1", "805a5d12-bbcd-477c-b3b5-567c7b8e6b33", "54c7aa21-fb89-4b87-b951-a5fbc95c4e32";
    "bard", "afbaccbe-d61d-4997-84d8-9e2c69359ee8", "30cc777b-774e-48ce-9561-a4b8ee8e2b96", "9b658bd2-3fdf-4be6-876d-6c54f3e367bd", "77cb5cf4-4ab4-47da-870e-7eb6f0c6d3fd";
    "bard_assistant", "17dfc67d-1749-4318-88f7-6aa1c48064f2", "92d74808-2abf-4229-a062-15d3330d1292", "7c0da0c8-93b3-422e-8534-476fcf36b200", "dc73067d-724d-4ad6-9573-3076b6f96ffa";
    "crypt_keeper", "4e56d680-2fe3-497b-ad1c-40a8306b2923", "02110812-263a-4fd3-bc25-0881cc974dc2", "71c5ca98-24c7-4b7c-a228-33dfa6b2749d", "dce834cc-6cdf-4e0c-8967-df0a40425ee0";
    "crypt_keeper_assistant", "9d99fcc0-7431-41a3-a1e3-9f37cd5ba8a7", "6b9b4ba8-bfdb-42f8-b104-944757391f06", "3b0fa550-2a2a-46f3-9b17-7405be5ccdf9", "915499ae-3c19-4252-a403-30b7b3f815e1";
    "barber", "de8e029e-5be3-4ad9-ad7d-817538046f9a", "b0b3b692-6f34-4adf-b6c7-0367ccea316b", "d2f69354-9a0c-49ce-a85c-c54610ac16b0", "7a124a5f-d5bd-476b-b6bd-409ba426dcc1";
    "researcher", "5eaf09fd-a724-4d8e-93f4-3bf9e3395e83", "1020dcce-c6ac-4ba3-b9e1-1e43d415fc77", "265ac8fd-a4a3-42ec-9aaf-f5d1f315e56a", "483d63f8-df1b-44d3-948d-8fdbb226b851";
    "trader", "6181d065-0d20-4361-ab4f-2622ceefa542", "a3cb0d1a-8b95-4acf-bda0-bcd2d4df33cc", "1dbd5a86-94b0-4d05-9d35-4c6e9a09cf03", "5463f9c1-8b22-41f1-9e50-25edd25d4b47";
    "fisher", "b19d25af-3283-416e-8173-9506ddd8f26e", "22276c36-ffe3-49ac-94b1-abe34f341fd6", "6426c3a1-481b-4574-954d-9a5d23c2c28d", "966d63f1-754f-4a0c-ab7c-dd18087dd94b";
}

/// The copy staff with the game id `item`.
pub(crate) fn copy_staff(item: u32) -> Option<&'static Staff> {
    STAFFS.iter().find(|s| s.copy.id() == item)
}

/// The game ids of every copy staff.
pub(crate) fn copy_ids() -> Vec<u32> {
    STAFFS.iter().map(|s| s.copy.id()).collect()
}

/// `template` with `{survivor}` filled in.
fn fill(template: &str, survivor: &str) -> String {
    template.replace("{survivor}", survivor)
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
        label: FULL_CHARGES,
        value: i32::from(charges),
        format: StatFormat::Number,
    };
    for staff in STAFFS {
        let vanilla = staff.survivor_or_off()?.staff();
        let survivor_key = format!("survivor.{}", staff.key);
        for (template_key, tag) in [
            ("copy.name", staff.name),
            ("copy.description", staff.description),
        ] {
            let translations: Vec<_> = texts.translations(template_key).collect();
            if translations.is_empty() {
                return Err(Off::new(format!("texts: no text for {template_key:?}")));
            }
            for (language, template) in translations {
                let survivor = texts.get(&survivor_key, language).unwrap_or(staff.key);
                load.add_text(tag, language.0, &fill(template, survivor))?;
            }
        }
        load.add_item(NewItem {
            name: Some(staff.name),
            description: Some(staff.description),
            meter: Some(meter),
            stats: vec![full],
            icon: Some(Icon {
                guid: staff.icon,
                like: vanilla,
                pixels: Pixels::Tint(ICON_TINT),
            }),
            ..NewItem::new(
                staff.copy,
                vanilla,
                format!("SurvivorsPerAltar_CopyStaff_{}", staff.key),
            )
        })?;
    }
    Ok(())
}

/// Files every copy staff as a staff of its survivor, beside its vanilla
/// staff, so the cursor, the unlock check and the placement treat both alike.
pub(crate) fn file(load: &mut Load<'_>) -> Result<(), Off> {
    for staff in STAFFS {
        load.add_summon_item(staff.copy.id(), staff.survivor_or_off()?.id())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_loader_api::i18n::Language;
    use enshwick_loader_api::survivors::WORKSHOP_SURVIVORS;

    #[test]
    fn every_staff_has_its_own_ids_and_a_name() {
        let texts = Texts::parse(TEXTS).unwrap();
        let mut ids = std::collections::BTreeSet::new();
        for staff in STAFFS {
            for guid in [staff.copy, staff.name, staff.description, staff.icon] {
                assert!(ids.insert(guid.id()), "{}", staff.key);
            }
            let survivor = staff.survivor().unwrap();
            assert_ne!(staff.copy.id(), survivor.staff().id());
            assert!(
                texts
                    .get(&format!("survivor.{}", staff.key), Language::FALLBACK)
                    .is_some()
            );
            assert_eq!(copy_staff(staff.copy.id()).map(|s| s.key), Some(staff.key));
            assert_eq!(copy_staff(survivor.staff().id()).map(|s| s.key), None);
        }
        assert_eq!(STAFFS.len(), WORKSHOP_SURVIVORS.len());
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
