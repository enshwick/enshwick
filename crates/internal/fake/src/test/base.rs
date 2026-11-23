//! The base of the fake: build facts, hooks, events, settings, points,
//! calls, pacing and steps, which every domain stands on.

use enshwick_core::__fake::ids;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::test::FakeHost;

pub(crate) mod calls;
pub(crate) mod events;
pub mod facts;
pub(crate) mod hooks;
pub(crate) mod host;
pub(crate) mod pacing;
pub(crate) mod points;
pub(crate) mod settings;
pub(crate) mod ticks;

/// What the fake's base answers, as the test set it.
#[derive(Clone, Debug)]
pub(crate) struct BasePresets {
    pub(crate) side: enshwick_core::abi::Side,
    pub(crate) module_base: u64,
    pub(crate) fingerprint: enshwick_core::abi::Fingerprint,
    pub(crate) build_complete: bool,
    pub(crate) handle: enshwick_core::abi::ModHandle,
    pub(crate) symbols: FxHashMap<String, u64>,
    pub(crate) systems: FxHashMap<String, u64>,
    pub(crate) constants: FxHashMap<String, u64>,
    pub(crate) field_offsets: FxHashMap<(String, String), u64>,
    pub(crate) type_sizes: FxHashMap<String, u32>,
    pub(crate) type_hashes: FxHashMap<String, u32>,
    pub(crate) event_types: Vec<(&'static str, u32, u32)>,
    /// The running world's settings files of the mod, by name.
    pub(crate) settings: std::collections::BTreeMap<String, String>,
    pub(crate) data_dir: String,
    pub(crate) mod_dir: String,
    /// The files of the mod's own folder, by name, written at install.
    pub(crate) mod_files: std::collections::BTreeMap<String, Vec<u8>>,
    pub(crate) log_level: enshwick_core::abi::Level,
    /// The levels of data packs that differ from `log_level`.
    pub(crate) pack_log_levels: Vec<(enshwick_core::abi::ModHandle, enshwick_core::abi::Level)>,
    /// The sides of data packs, by handle.
    pub(crate) pack_sides: Vec<(enshwick_core::abi::ModHandle, enshwick_core::abi::Side)>,
    pub(crate) caller_frame: enshwick_core::abi::CallerFrame,
    pub(crate) components: FxHashMap<(u32, u32), (usize, usize)>,
    pub(crate) players: Vec<enshwick_core::abi::PlayerInfo>,
    pub(crate) mods: Vec<enshwick_core::abi::ModInfo>,
    pub(crate) hosting: bool,
    pub(crate) developer_tools: bool,
    /// The periodic tick subscription, by the order asked, to refuse.
    pub(crate) refused_tick: Option<usize>,
    pub(crate) host_key: Option<enshwick_core::PlayerKey>,
    pub(crate) server_world: u64,
    pub(crate) join_missing: Vec<enshwick_core::abi::ModInfo>,
    /// What other mods export, by id; a mod left out exports every name.
    pub(crate) exports: FxHashMap<String, Vec<String>>,
    pub(crate) language: u32,
    pub(crate) services: Vec<host::FakeService>,
    pub(crate) refused_hooks: FxHashSet<u64>,
    pub(crate) code: (u64, u64),
    pub(crate) workshop_recipes: u32,
    pub(crate) world_key: Option<String>,
    pub(crate) net_peers: FxHashSet<u64>,
    /// Players whose client has not said hello, so their windows wait.
    #[cfg(feature = "unstable")]
    pub(crate) without_hello: FxHashSet<u64>,
    /// Content ids other loaded mods declared.
    pub(crate) declared_content: Vec<String>,
    pub(crate) player_languages: FxHashMap<u64, u32>,
}

impl Default for BasePresets {
    fn default() -> Self {
        Self {
            side: enshwick_core::abi::Side::SERVER,
            module_base: 0x1_4000_0000,
            fingerprint: enshwick_core::abi::Fingerprint {
                guid: enshwick_core::abi::Guid::NIL,
                age: 0,
            },
            build_complete: true,
            handle: enshwick_core::abi::ModHandle(1),
            symbols: FxHashMap::default(),
            systems: FxHashMap::default(),
            constants: FxHashMap::default(),
            field_offsets: FxHashMap::default(),
            type_sizes: FxHashMap::default(),
            type_hashes: FxHashMap::default(),
            event_types: Vec::new(),
            settings: std::collections::BTreeMap::new(),
            data_dir: "world0-mod-data/test".to_owned(),
            mod_dir: "mods/test".to_owned(),
            mod_files: std::collections::BTreeMap::new(),
            log_level: enshwick_core::abi::Level::INFO,
            pack_log_levels: Vec::new(),
            pack_sides: Vec::new(),
            caller_frame: enshwick_core::abi::CallerFrame::default(),
            components: FxHashMap::default(),
            players: Vec::new(),
            mods: Vec::new(),
            hosting: false,
            developer_tools: false,
            refused_tick: None,
            host_key: None,
            server_world: 0,
            join_missing: Vec::new(),
            exports: FxHashMap::default(),
            language: 0,
            services: Vec::new(),
            refused_hooks: FxHashSet::default(),
            code: (0, 0),
            workshop_recipes: 512,
            world_key: None,
            net_peers: FxHashSet::default(),
            #[cfg(feature = "unstable")]
            without_hello: FxHashSet::default(),
            declared_content: Vec::new(),
            player_languages: FxHashMap::default(),
        }
    }
}

impl BasePresets {
    /// Whether the fake runs the server side of a game: a dedicated server or
    /// a client that hosts.
    pub(crate) fn serving(&self) -> bool {
        self.side == enshwick_core::abi::Side::SERVER || self.hosting
    }

    /// The account of the hosting client's own player, as the loader's
    /// carrier knows it; `None` unless the fake hosts.
    pub(crate) fn own_key(&self) -> Option<enshwick_core::PlayerKey> {
        self.host_key.filter(|_| self.hosting)
    }

    /// The language the player of account `key` reads, as the ABI numbers
    /// it: the game's own for the hosting client's own player, else the one
    /// its game named when it joined, else 0, the fallback.
    pub(crate) fn language_of(&self, key: enshwick_core::PlayerKey) -> u32 {
        if self.own_key() == Some(key) {
            return self.language;
        }
        self.player_languages.get(&key.get()).copied().unwrap_or(0)
    }
}

impl FakeHost {
    /// The hit, death, equipment change and respawn point change game
    /// events, with the hit history and experience credit a death reads its
    /// killer from, and damage bits, an attack category and an equipment
    /// slot numbered unlike the API's own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().game_events();
    /// let target = host.offset(facts::HIT_EVENT_TARGET_ID.id())?;
    /// assert!(target + 4 <= host.size(facts::HIT_EVENT.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn game_events(self) -> Self {
        const HIT: u64 = 64;
        self.preset(
            &[
                ids::HIT_EVENT,
                ids::ENTITY_DIED_EVENT,
                ids::HIT_HISTORY,
                ids::EXPERIENCE_CREDIT,
                ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT,
                ids::PLAYER_SPAWN_INFO_CHANGE_EVENT,
            ],
            &[
                (ids::HIT_EVENT, 64),
                (ids::ENTITY_DIED_EVENT, 16),
                (ids::HIT_HISTORY, 152),
                (ids::EXPERIENCE_CREDIT, 8),
                (ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT, 16),
                (ids::PLAYER_SPAWN_INFO_CHANGE_EVENT, 48),
            ],
            &[
                (ids::HIT_EVENT_ROOT_SOURCE_ID, 8),
                (ids::HIT_EVENT_TARGET_ID, 12),
                (ids::HIT_EVENT_SOURCE_ID, 16),
                (ids::HIT_EVENT_ROOT_TARGET_ID, 20),
                (ids::HIT_EVENT_WEAPON_PIDE_ID, 24),
                (ids::HIT_HEALTH_CHANGE, 28),
                (ids::HIT_STAMINA_CHANGE, 32),
                (ids::HIT_DAMAGE_TYPES, 36),
                (ids::HIT_EVENT_ATTACK_CATEGORY, 38),
                (ids::HIT_TARGET_FACTION, 39),
                (ids::ENTITY_DIED_EVENT_DEAD_ENTITY_ID, 8),
                (ids::HIT_HISTORY_LAST_INDEX, 0),
                (ids::HIT_HISTORY_ENTRIES, 8),
                (ids::HIT_HISTORY_END, 8 + 2 * HIT),
                (ids::EXPERIENCE_CREDIT_LAST_PLAYER, 4),
                (ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT, 12),
                (ids::DAMAGE_TYPE_MAGICAL_FIRE, 0),
                (ids::DAMAGE_TYPE_PHYSICAL_CUT, 5),
                (ids::ATTACK_CATEGORY_MELEE, 2),
                (ids::EQUIPMENT_SLOT_SHIELD, 40),
                (ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_CHARACTER, 8),
                (ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_BASE, 24),
                (ids::PLAYER_SPAWN_INFO_CHANGE_EVENT_SCENE, 40),
            ],
        )
    }

    /// Plays a build that lacks `fact`, whichever preset set it, so a test
    /// of what turns off without it starts from the preset.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new()
    ///     .world()
    ///     .without_fact(facts::WORLD_TRANSFORM_ORIENTATION);
    /// assert!(host.offset(facts::WORLD_TRANSFORM_ORIENTATION.id()).is_err());
    /// ```
    #[must_use]
    pub fn without_fact(mut self, fact: facts::Fact) -> Self {
        self.base.type_hashes.remove(fact.id());
        self.base.type_sizes.remove(fact.id());
        self.base.constants.remove(fact.id());
        self
    }
}
