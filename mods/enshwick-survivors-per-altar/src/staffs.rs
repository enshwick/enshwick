//! Copy staffs: one item per workshop survivor, a clone of the survivor's
//! summoning staff that the game takes when it is used.
//!
//! The game knows which survivor a staff summons from its item registry's map
//! of staff items to survivors, built once per game from `NpcCollection`, one
//! item per survivor. A link after that build files each copy staff beside its
//! vanilla staff, so the cursor, the unlock check and the placement treat both
//! alike, on the server and the client. The spawn link in `server` then marks
//! a copy staff's use as consuming, which makes the game take one staff in
//! the same transaction as the spawn or move, and only when that works.

use enshwick_loader_api::content::Guid;
use enshwick_loader_api::content::items::NewItem;
use enshwick_loader_api::i18n::Texts;
use enshwick_loader_api::{Load, Next, Off};

/// The mod's texts, per language.
pub(crate) const TEXTS: &str = include_str!("texts.toml");

/// A workshop survivor and its copy staff.
#[derive(Debug)]
pub(crate) struct Staff {
    /// The text key of the survivor's name, after `survivor.`.
    pub key: &'static str,
    /// The survivor's id.
    pub npc: u32,
    /// The vanilla summoning staff.
    pub vanilla: Guid,
    /// The survivor's template, which the staff places.
    pub template: Guid,
    /// The copy staff.
    pub copy: Guid,
    /// The tag of the copy staff's name.
    pub name: Guid,
    /// The tag of its description.
    pub description: Guid,
}

macro_rules! staffs {
    ($($key:literal, $npc:literal, $vanilla:literal, $template:literal, $copy:literal, $name:literal, $description:literal;)*) => {
        /// Every workshop survivor: crafters, assistants and traders.
        pub(crate) const STAFFS: &[Staff] = &[$(Staff {
            key: $key,
            npc: $npc,
            vanilla: Guid::parse($vanilla),
            template: Guid::parse($template),
            copy: Guid::parse($copy),
            name: Guid::parse($name),
            description: Guid::parse($description),
        }),*];
    };
}

staffs! {
    "blacksmith", 1_201_801_546, "505ccfbc-440d-4d63-a3d6-d689d3d55e78", "c313f0a8-ed2e-444e-8384-a540f6bbd98e",
        "dac11dad-687c-4397-b528-fdec7cdb6ba1", "438a229b-d221-4d69-8a76-acc252628fa3", "5f0fed42-fbff-4811-8516-ed774b5a7232";
    "blacksmith_assistant", 1_731_045_065, "a514d932-4cf0-4eac-b25c-d8cf32f6d394", "04d42ed2-beb4-4013-8f10-5d63efae2015",
        "10b19c0f-b86a-4bb2-96c6-5dfef7e996d3", "1b6a3f21-d2df-4649-a536-766485c89d28", "33316d27-428d-460c-9e42-0192aadc20bf";
    "alchemist", 4_057_418_535, "b500ea01-9d3f-40c3-a3b1-14127f313755", "7456862b-53fe-4b94-8a75-d3079afc9323",
        "448e4f2a-d620-41ca-bfc6-f311320779e5", "b12bb3d7-5ce4-4f7b-89fe-3e38747abbce", "e7f7f180-0aae-45b0-9452-3b4266b78d74";
    "alchemist_assistant", 3_159_593_666, "b8e3be95-1811-43ba-8e25-a6f1b6b1ca04", "27e2fb44-4975-4b61-988d-c7b4e16ed325",
        "bbecddee-022a-4506-bde1-ffe07d1b6727", "f6968340-15f8-4ac7-b521-e31f713d2a26", "13bd532e-f26f-4aa8-a2d4-e4201453a98a";
    "carpenter", 2_103_948_589, "3431ac56-3dc6-4230-ba4a-3bf5e0f2d086", "8045836c-0647-4a0a-9a00-7d299b4599f8",
        "177d03e4-ac34-439a-b342-fee6973629f6", "dfb2ca2c-4e27-451b-8787-7728a71997c2", "973390fd-a203-4480-9a73-57941a778503";
    "carpenter_assistant", 2_031_808_579, "b5ba37e3-3cc5-46dd-9398-89ecbc6a9bd7", "31ff0824-2c28-4934-a236-d3aa5c0306b9",
        "f54ca128-3ed7-4e30-bb93-6f82d8fc0d29", "bc6902e1-3221-4620-a4bc-afcb03364bc3", "5a1efc91-aee9-4bd4-a047-6d4a9c45f364";
    "hunter", 2_242_206_866, "3dbdc1d0-98e7-4b25-9327-937444b6ac87", "a6db3870-a426-494e-a203-64789389e7d7",
        "8057fdfe-c990-4338-8a8d-a850a12458ab", "44f1e095-af4b-4565-bf85-c43e79aca035", "fa2519ec-7ce6-4d76-9668-70fd34e35297";
    "hunter_assistant", 1_633_192_162, "eeae5607-6361-4067-b608-26e93c7cef71", "e3736dff-0ad2-47fc-903c-66c1775836e8",
        "a3bca904-6d54-4c66-a11b-8fc0f52daba5", "a8922651-5118-4d88-bb12-f9b1f3655abd", "65700ddb-d554-4ea8-b08b-ede01576c538";
    "farmer", 974_506_154, "2ba6961c-f842-49bd-9551-654c82cb6de0", "1b533365-f465-42e0-a51f-7d66e9cf3659",
        "42f978ea-d349-4f44-8796-b61bebaa7123", "08ced2ca-8a7d-4b73-a71e-286a1da25195", "547ee06c-e76b-4a1d-9835-f2b9af08b4e6";
    "farmer_assistant", 1_669_027_340, "79a17ecf-390b-4c1c-baec-5f5ac48a95f1", "a6bab01b-a848-4645-84fb-6ae22b532eb0",
        "d5bf7763-a011-4ad0-af9b-09b67e1f5a91", "f7e2a0b6-ebf2-4d64-8c80-9c6860fd78b1", "805a5d12-bbcd-477c-b3b5-567c7b8e6b33";
    "bard", 218_342_694, "b3923138-2f51-44db-9a85-c400a342ff7b", "23a61977-ae52-443d-ab6f-23826a2cda5a",
        "afbaccbe-d61d-4997-84d8-9e2c69359ee8", "30cc777b-774e-48ce-9561-a4b8ee8e2b96", "9b658bd2-3fdf-4be6-876d-6c54f3e367bd";
    "bard_assistant", 1_640_210_983, "b55d2fef-1d05-43ea-aa2a-7fe0fcd27bc2", "ff6cd2cf-b566-482c-ba14-075d1a918b3d",
        "17dfc67d-1749-4318-88f7-6aa1c48064f2", "92d74808-2abf-4229-a062-15d3330d1292", "7c0da0c8-93b3-422e-8534-476fcf36b200";
    "crypt_keeper", 757_847_452, "48aee0f0-ba8f-4045-9595-42b0723d67a2", "b61da3e5-b8f2-4933-84f8-48e7ecfcba39",
        "4e56d680-2fe3-497b-ad1c-40a8306b2923", "02110812-263a-4fd3-bc25-0881cc974dc2", "71c5ca98-24c7-4b7c-a228-33dfa6b2749d";
    "crypt_keeper_assistant", 2_159_156_280, "f0e9d8f1-9af1-4426-8cdb-5bbf1453fb32", "aa51d579-d35f-433e-be91-d4f7133d62c5",
        "9d99fcc0-7431-41a3-a1e3-9f37cd5ba8a7", "6b9b4ba8-bfdb-42f8-b104-944757391f06", "3b0fa550-2a2a-46f3-9b17-7405be5ccdf9";
    "barber", 3_689_431_984, "828d1363-f824-4552-8f0a-1303bd369d14", "f63dc05f-88ed-4f5a-9640-1d19d980bb37",
        "de8e029e-5be3-4ad9-ad7d-817538046f9a", "b0b3b692-6f34-4adf-b6c7-0367ccea316b", "d2f69354-9a0c-49ce-a85c-c54610ac16b0";
    "researcher", 1_095_951_179, "a343beaa-e10f-4395-981d-ac3e8ca6a785", "35afe333-75a0-45ec-90d4-5abddb08fdc5",
        "5eaf09fd-a724-4d8e-93f4-3bf9e3395e83", "1020dcce-c6ac-4ba3-b9e1-1e43d415fc77", "265ac8fd-a4a3-42ec-9aaf-f5d1f315e56a";
    "trader", 2_797_365_275, "05e5697b-983f-4a99-a1c9-77a39bead07a", "877ab652-90f8-45da-8865-70e4e0fedc57",
        "6181d065-0d20-4361-ab4f-2622ceefa542", "a3cb0d1a-8b95-4acf-bda0-bcd2d4df33cc", "1dbd5a86-94b0-4d05-9d35-4c6e9a09cf03";
    "fisher", 4_192_796_482, "d2561878-5596-4b29-bfa2-0ce0da1f65d5", "e7666cd3-e530-4f9d-9586-519e4ab969dc",
        "b19d25af-3283-416e-8173-9506ddd8f26e", "22276c36-ffe3-49ac-94b1-abe34f341fd6", "6426c3a1-481b-4574-954d-9a5d23c2c28d";
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

/// Adds every copy staff and its texts.
pub(crate) fn add(load: &mut Load<'_>, texts: &Texts) -> Result<(), Off> {
    for staff in STAFFS {
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
            guid: staff.copy,
            like: staff.vanilla,
            name: Some(staff.name),
            description: Some(staff.description),
            debug_name: format!("SurvivorsPerAltar_CopyStaff_{}", staff.key),
        })?;
    }
    Ok(())
}

type RegistryLoadFn = unsafe extern "system" fn(registry: *mut u8);
type FindSlotFn = unsafe extern "system" fn(map: *mut u8, key: *const u32) -> u64;

enshwick_loader_api::detour! {
    static REGISTRY_LOAD: unsafe extern "system" fn(registry: *mut u8) = registry_load;
}

/// The staff item map inside the survivor registry.
#[derive(Debug)]
struct ItemMap {
    find_slot: FindSlotFn,
    loaded: usize,
    at: usize,
    bits: usize,
    capacity: usize,
    keys: usize,
    values: usize,
    count: usize,
}

static ITEM_MAP: std::sync::OnceLock<ItemMap> = std::sync::OnceLock::new();

/// Files the copy staffs in the survivor registry each time the game builds
/// it.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let size = |name| {
        load.constant(name)
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
    };
    let find_slot = load.symbol("survivors.map_find_slot")?;
    let map = ItemMap {
        // SAFETY: the mod pins this id to the map's find-or-free-slot.
        find_slot: unsafe { crate::server::function(find_slot) },
        loaded: size("survivors.registry_loaded")?,
        at: size("survivors.registry_item_map")?,
        bits: size("survivors.map_bits")?,
        capacity: size("survivors.map_capacity")?,
        keys: size("survivors.map_keys")?,
        values: size("survivors.map_values")?,
        count: size("survivors.map_count")?,
    };
    let _first = ITEM_MAP.set(map);
    load.hook(&REGISTRY_LOAD, load.symbol("survivors.registry_load")?)
}

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// A readable `T` lies there.
unsafe fn read_at<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

fn registry_load(next: Next<RegistryLoadFn>, registry: *mut u8) {
    let map = ITEM_MAP.get().filter(|_| !registry.is_null());
    // The game calls this often and builds only once: a registry that holds
    // its collection already is left as it is.
    // SAFETY: the registry's collection pointer, null until it is built.
    let built = map.is_some_and(|m| unsafe { read_at::<usize>(registry, m.loaded) } != 0);
    // SAFETY: the rest of the chain takes the registry the game passed.
    unsafe { next.get()(registry) };
    let Some(map) = map.filter(|_| !built) else {
        return;
    };
    let mut filed = 0;
    for staff in STAFFS {
        // SAFETY: the registry is built; its item map lies at `map.at`.
        if unsafe {
            file(
                map,
                registry.wrapping_add(map.at),
                staff.copy.id(),
                staff.npc,
            )
        } {
            filed += 1;
        }
    }
    enshwick_loader_api::info!("filed {filed} copy staffs in the survivor registry");
}

/// Maps `item` to `npc` in the game's map at `table`, as its own build does:
/// the game's find-or-free-slot, which grows the table as needed, then the
/// bit, key and value.
///
/// # Safety
///
/// `table` is the live staff item map, not used by another thread.
unsafe fn file(map: &ItemMap, table: *mut u8, item: u32, npc: u32) -> bool {
    // SAFETY: the game's own slot search on its map.
    let slot = unsafe { (map.find_slot)(table, &raw const item) };
    // SAFETY: the map's header fields, read again after a possible growth.
    let capacity: u64 = unsafe { read_at(table, map.capacity) };
    if slot >= capacity {
        return false;
    }
    let Ok(slot) = usize::try_from(slot) else {
        return false;
    };
    // SAFETY: as above.
    let bits: *mut u64 = unsafe { read_at(table, map.bits) };
    // SAFETY: as above.
    let keys: *mut u32 = unsafe { read_at(table, map.keys) };
    // SAFETY: as above.
    let values: *mut u32 = unsafe { read_at(table, map.values) };
    let word = bits.wrapping_add(slot >> 6);
    let bit = 1u64 << (slot & 63);
    // SAFETY: the slot is below the capacity, so its bit word exists.
    let used = unsafe { word.read() } & bit != 0;
    if !used {
        // SAFETY: as above.
        let marked = unsafe { word.read() } | bit;
        // SAFETY: as above.
        unsafe { word.write(marked) };
        // SAFETY: the map's entry count.
        let count: u32 = unsafe { read_at(table, map.count) };
        let at = table.wrapping_add(map.count);
        // SAFETY: as above.
        unsafe { at.cast::<[u8; 4]>().write((count + 1).to_le_bytes()) };
        // SAFETY: the slot's key.
        unsafe { keys.wrapping_add(slot).write(item) };
    }
    // SAFETY: the slot's value.
    unsafe { values.wrapping_add(slot).write(npc) };
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_loader_api::i18n::Language;

    #[test]
    fn every_staff_has_its_own_ids_and_a_name() {
        let texts = Texts::parse(TEXTS).unwrap();
        let mut ids = std::collections::BTreeSet::new();
        for staff in STAFFS {
            for guid in [staff.copy, staff.name, staff.description] {
                assert!(ids.insert(guid.id()), "{}", staff.key);
            }
            assert_ne!(staff.copy.id(), staff.vanilla.id());
            assert!(
                texts
                    .get(&format!("survivor.{}", staff.key), Language::FALLBACK)
                    .is_some()
            );
            assert_eq!(copy_staff(staff.copy.id()).map(|s| s.npc), Some(staff.npc));
        }
        assert_eq!(STAFFS.len(), 18);
        assert_eq!(copy_staff(STAFFS[0].vanilla.id()).map(|s| s.key), None);
    }

    #[test]
    fn the_vanilla_staff_ids_match_their_guids() {
        assert_eq!(STAFFS[0].vanilla.id(), 786_532_879);
        assert_eq!(STAFFS[17].vanilla.id(), 2_684_532_648);
        // A survivor's id is the game id of its template.
        for staff in STAFFS {
            assert_eq!(staff.template.id(), staff.npc, "{}", staff.key);
        }
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
        }
    }

    #[test]
    fn filing_marks_a_free_slot_and_overwrites_a_used_one() {
        unsafe extern "system" fn find(_map: *mut u8, key: *const u32) -> u64 {
            // SAFETY: the test passes a live key.
            u64::from(unsafe { key.read() } & 127)
        }
        let mut bits = [0u64; 2];
        let mut keys = [0u32; 128];
        let mut values = [0u32; 128];
        let mut table = [0u8; 0x60];
        table[0x08..0x10].copy_from_slice(&(bits.as_mut_ptr() as u64).to_le_bytes());
        table[0x10..0x18].copy_from_slice(&128u64.to_le_bytes());
        table[0x28..0x30].copy_from_slice(&(keys.as_mut_ptr() as u64).to_le_bytes());
        table[0x40..0x48].copy_from_slice(&(values.as_mut_ptr() as u64).to_le_bytes());
        let map = ItemMap {
            find_slot: find,
            loaded: 0x20,
            at: 0,
            bits: 0x08,
            capacity: 0x10,
            keys: 0x28,
            values: 0x40,
            count: 0x54,
        };
        // SAFETY: a table laid out as the game's, owned by the test.
        assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 5) });
        // SAFETY: as above.
        assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 6) });
        assert_eq!(bits[1], 1 << 6);
        assert_eq!((keys[70], values[70]), (70, 6));
        assert_eq!(u32::from_le_bytes(table[0x54..0x58].try_into().unwrap()), 1);
    }
}
