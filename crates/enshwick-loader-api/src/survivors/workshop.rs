//! The game's workshop survivors: crafters, assistants and traders.

use super::SurvivorId;
use crate::content::Guid;

/// A workshop survivor of the game, with its summoning staff and the
/// template the staff places; listed in [`WORKSHOP_SURVIVORS`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::survivors::WorkshopSurvivor;
///
/// let smith = WorkshopSurvivor::find("blacksmith").ok_or("no blacksmith")?;
/// assert_eq!(smith.template().id(), smith.id().0);
/// # Ok::<(), &str>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkshopSurvivor {
    key: &'static str,
    id: SurvivorId,
    staff: Guid,
    template: Guid,
}

impl WorkshopSurvivor {
    /// The survivor with the short name `key`, such as `"blacksmith"` or
    /// `"farmer_assistant"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::survivors::WorkshopSurvivor;
    ///
    /// assert!(WorkshopSurvivor::find("fisher").is_some());
    /// assert!(WorkshopSurvivor::find("dragon").is_none());
    /// ```
    #[must_use]
    pub fn find(key: &str) -> Option<&'static Self> {
        WORKSHOP_SURVIVORS.iter().find(|s| s.key == key)
    }

    /// The survivor whose summoning staff has the game id `item`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::survivors::WorkshopSurvivor;
    ///
    /// let smith = WorkshopSurvivor::find("blacksmith").ok_or("no blacksmith")?;
    /// assert_eq!(WorkshopSurvivor::by_staff(smith.staff().id()), Some(smith));
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn by_staff(item: u32) -> Option<&'static Self> {
        WORKSHOP_SURVIVORS.iter().find(|s| s.staff.id() == item)
    }

    /// The short name, lower case with underscores, as the survivor is
    /// called in English.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::survivors::WORKSHOP_SURVIVORS;
    ///
    /// assert_eq!(WORKSHOP_SURVIVORS.first().map(|s| s.key()), Some("blacksmith"));
    /// ```
    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// The survivor's id: the game id of its template.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::survivors::{SurvivorId, WorkshopSurvivor};
    ///
    /// let smith = WorkshopSurvivor::find("blacksmith").ok_or("no blacksmith")?;
    /// assert_eq!(smith.id(), SurvivorId(1_201_801_546));
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub const fn id(&self) -> SurvivorId {
        self.id
    }

    /// The vanilla summoning staff, the item that places or moves the
    /// survivor.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::survivors::WorkshopSurvivor;
    ///
    /// let smith = WorkshopSurvivor::find("blacksmith").ok_or("no blacksmith")?;
    /// assert_eq!(smith.staff().id(), 786_532_879);
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub const fn staff(&self) -> Guid {
        self.staff
    }

    /// The survivor's name as the game shows it in `language`, such as
    /// "Balthazar": the name of its summoning staff, from the game's own
    /// data and texts (game build 1269579). `None` on a server, whose
    /// archive holds no texts; see [`crate::content::items::vanilla_name`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::i18n::Language;
    /// use enshwick_loader_api::survivors::WorkshopSurvivor;
    ///
    /// let alchemist = WorkshopSurvivor::find("alchemist").ok_or("no alchemist")?;
    /// let name = alchemist.name(Language::UK_UA).unwrap_or(alchemist.key());
    /// assert!(!name.is_empty());
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn name(&self, language: crate::i18n::Language) -> Option<&'static str> {
        crate::content::items::vanilla_name(self.staff, language)
    }

    /// The survivor's template, which its staff places.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::survivors::WorkshopSurvivor;
    ///
    /// let smith = WorkshopSurvivor::find("blacksmith").ok_or("no blacksmith")?;
    /// assert_eq!(smith.template().id(), smith.id().0);
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub const fn template(&self) -> Guid {
        self.template
    }
}

macro_rules! survivors {
    ($($key:literal, $id:literal, $staff:literal, $template:literal;)*) => {
        /// Every workshop survivor of the game, from its item and template
        /// data.
        ///
        /// # Examples
        ///
        /// ```
        /// use enshwick_loader_api::survivors::WORKSHOP_SURVIVORS;
        ///
        /// let assistants = WORKSHOP_SURVIVORS.iter().filter(|s| s.key().ends_with("_assistant"));
        /// assert_eq!(assistants.count(), 7);
        /// ```
        pub const WORKSHOP_SURVIVORS: &[WorkshopSurvivor] = &[$(WorkshopSurvivor {
            key: $key,
            id: SurvivorId($id),
            staff: Guid::parse($staff),
            template: Guid::parse($template),
        }),*];
    };
}

survivors! {
    "blacksmith", 1_201_801_546, "505ccfbc-440d-4d63-a3d6-d689d3d55e78", "c313f0a8-ed2e-444e-8384-a540f6bbd98e";
    "blacksmith_assistant", 1_731_045_065, "a514d932-4cf0-4eac-b25c-d8cf32f6d394", "04d42ed2-beb4-4013-8f10-5d63efae2015";
    "alchemist", 4_057_418_535, "b500ea01-9d3f-40c3-a3b1-14127f313755", "7456862b-53fe-4b94-8a75-d3079afc9323";
    "alchemist_assistant", 3_159_593_666, "b8e3be95-1811-43ba-8e25-a6f1b6b1ca04", "27e2fb44-4975-4b61-988d-c7b4e16ed325";
    "carpenter", 2_103_948_589, "3431ac56-3dc6-4230-ba4a-3bf5e0f2d086", "8045836c-0647-4a0a-9a00-7d299b4599f8";
    "carpenter_assistant", 2_031_808_579, "b5ba37e3-3cc5-46dd-9398-89ecbc6a9bd7", "31ff0824-2c28-4934-a236-d3aa5c0306b9";
    "hunter", 2_242_206_866, "3dbdc1d0-98e7-4b25-9327-937444b6ac87", "a6db3870-a426-494e-a203-64789389e7d7";
    "hunter_assistant", 1_633_192_162, "eeae5607-6361-4067-b608-26e93c7cef71", "e3736dff-0ad2-47fc-903c-66c1775836e8";
    "farmer", 974_506_154, "2ba6961c-f842-49bd-9551-654c82cb6de0", "1b533365-f465-42e0-a51f-7d66e9cf3659";
    "farmer_assistant", 1_669_027_340, "79a17ecf-390b-4c1c-baec-5f5ac48a95f1", "a6bab01b-a848-4645-84fb-6ae22b532eb0";
    "bard", 218_342_694, "b3923138-2f51-44db-9a85-c400a342ff7b", "23a61977-ae52-443d-ab6f-23826a2cda5a";
    "bard_assistant", 1_640_210_983, "b55d2fef-1d05-43ea-aa2a-7fe0fcd27bc2", "ff6cd2cf-b566-482c-ba14-075d1a918b3d";
    "crypt_keeper", 757_847_452, "48aee0f0-ba8f-4045-9595-42b0723d67a2", "b61da3e5-b8f2-4933-84f8-48e7ecfcba39";
    "crypt_keeper_assistant", 2_159_156_280, "f0e9d8f1-9af1-4426-8cdb-5bbf1453fb32", "aa51d579-d35f-433e-be91-d4f7133d62c5";
    "barber", 3_689_431_984, "828d1363-f824-4552-8f0a-1303bd369d14", "f63dc05f-88ed-4f5a-9640-1d19d980bb37";
    "researcher", 1_095_951_179, "a343beaa-e10f-4395-981d-ac3e8ca6a785", "35afe333-75a0-45ec-90d4-5abddb08fdc5";
    "trader", 2_797_365_275, "05e5697b-983f-4a99-a1c9-77a39bead07a", "877ab652-90f8-45da-8865-70e4e0fedc57";
    "fisher", 4_192_796_482, "d2561878-5596-4b29-bfa2-0ce0da1f65d5", "e7666cd3-e530-4f9d-9586-519e4ab969dc";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_survivor_id_is_its_template_id() {
        let mut keys = std::collections::BTreeSet::new();
        for survivor in WORKSHOP_SURVIVORS {
            assert_eq!(
                survivor.template().id(),
                survivor.id().0,
                "{}",
                survivor.key()
            );
            assert!(keys.insert(survivor.key()));
        }
        assert_eq!(WORKSHOP_SURVIVORS.len(), 18);
        assert_eq!(WORKSHOP_SURVIVORS[17].staff().id(), 2_684_532_648);
    }
}
