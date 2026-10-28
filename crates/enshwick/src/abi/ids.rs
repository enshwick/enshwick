//! The ids the ABI carries, each a transparent newtype over the game's
//! integer so a struct field or a call says which id it takes.

/// An entity id, as the engine numbers entities: a character, a prop, an item
/// with its own entity, anything the world holds.
///
/// The id lasts one session: every load of a world numbers its entities
/// anew, in the order of the save, so an id kept across a restart names
/// another entity. It therefore has no serde form. Data that must outlast a
/// restart goes in [entity data](crate::entity_data) or
/// [item data](crate::item_data), which find their entities again after a
/// load. The engine's id 0 stands for no entity; the safe API hands it over
/// as `None` in an `Option<Entity>`, never as an `Entity`.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
///
/// let entity = Entity(1210);
/// assert_eq!(entity.0, 1210);
/// ```
///
/// A kept file cannot hold one:
///
/// ```compile_fail
/// #[derive(serde::Serialize)]
/// struct Owners {
///     chest: enshwick::Entity,
/// }
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[shape(api = "entity")]
pub struct Entity(pub u32);

impl Entity {
    /// The engine's id for no entity.
    pub(crate) const NONE: Self = Self(0);

    /// Whether this is the engine's id for no entity.
    #[must_use]
    pub(crate) const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// `Some(entity)` for the engine's id `raw`, `None` for 0.
    #[must_use]
    pub(crate) const fn from_raw(raw: u32) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }
}

/// An item of the game by the number its item data gives it, as a prop's
/// `UsedItem` names the item the prop was placed from.
///
/// # Examples
///
/// ```
/// use enshwick::ItemId;
///
/// let torch = ItemId::from(1_042);
/// assert_eq!(torch.get(), 1_042);
/// assert_eq!(torch, ItemId::new(1_042));
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[serde(transparent)]
#[shape(api = "item_id")]
pub struct ItemId(pub u32);

/// A player's account by the number the game keeps for it, the same in every
/// session, to remember a player across reconnects and restarts.
///
/// # Examples
///
/// ```
/// use enshwick::PlayerKey;
///
/// let account = PlayerKey::from(0x5eed_u64);
/// assert_eq!(account.get(), 0x5eed);
/// assert_eq!(account, PlayerKey::new(0x5eed));
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[serde(transparent)]
#[shape(api = "player_key")]
pub struct PlayerKey(pub u64);

/// A crafting recipe of the game by its recipe id, to grant, refuse or watch
/// the crafting of one recipe.
///
/// # Examples
///
/// ```
/// use enshwick::RecipeId;
///
/// let recipe = RecipeId::from(77);
/// assert_eq!(recipe.get(), 77);
/// assert_eq!(recipe, RecipeId::new(77));
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[serde(transparent)]
#[shape(api = "recipe_id")]
pub struct RecipeId(pub u32);

/// A node of the game's skill tree by its id, as
/// [`Skill::node`](crate::world::Skill::node) names the nodes a character
/// unlocked and a skill check names the node it asks about.
///
/// # Examples
///
/// ```
/// use enshwick::world::SkillNode;
///
/// let node = SkillNode::from(0x2a_u32);
/// assert_eq!(node.get(), 0x2a);
/// assert_eq!(u32::from(node), 0x2a);
/// assert_eq!(node, SkillNode::new(0x2a));
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[serde(transparent)]
#[shape(api = "skill_node")]
pub struct SkillNode(pub u32);

/// A knowledge value of the game by its id: a recipe's, a quest's, or any
/// other bool, counter or flags the world or a player keeps.
///
/// A recipe's knowledge has the recipe's id, so a [`RecipeId`] converts.
///
/// # Examples
///
/// ```
/// use enshwick::RecipeId;
/// use enshwick::knowledge::KnowledgeId;
///
/// let quest = KnowledgeId::new(0x51);
/// assert_eq!(quest.get(), 0x51);
/// assert_eq!(KnowledgeId::from(RecipeId::new(0x1d)), KnowledgeId::new(0x1d));
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[serde(transparent)]
#[shape(api = "knowledge_id")]
pub struct KnowledgeId(pub u32);

/// A buff type of the game by its id, the
/// [`Guid::fnv1a`](crate::content::Guid::fnv1a) of its GUID, to apply,
/// find or remove one buff.
///
/// # Examples
///
/// ```
/// use enshwick::buffs::BuffTypeId;
///
/// const WARMTH: BuffTypeId = BuffTypeId::named("my-mod:warmth");
/// assert_eq!(WARMTH.to_string(), WARMTH.get().to_string());
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[serde(transparent)]
#[shape(api = "buff_type_id")]
pub struct BuffTypeId(pub u32);

/// An NPC of the game, such as a survivor, by its id, the
/// [`Guid::fnv1a`](crate::content::Guid::fnv1a) of its template's GUID, to
/// tell survivors apart in lookups, bed checks and summons.
///
/// # Examples
///
/// ```
/// use enshwick::survivors::NpcId;
///
/// let blacksmith = NpcId::new(1_201_801_546);
/// assert_eq!(blacksmith.get(), 1_201_801_546);
/// ```
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    crate::Script,
)]
#[serde(transparent)]
#[shape(api = "npc_id")]
pub struct NpcId(pub u32);

impl From<RecipeId> for KnowledgeId {
    fn from(recipe: RecipeId) -> Self {
        Self(recipe.0)
    }
}

/// `new`, `get`, the `From` conversions both ways and the formatting traits
/// of an id newtype over an integer, which print the bare number.
macro_rules! id_conversions {
    ($($id:ident($int:ty)),* $(,)?) => {$(
        impl $id {
            /// The id numbered `id`, in a `const` too.
            ///
            /// # Examples
            ///
            #[doc = concat!("See [`", stringify!($id), "`].")]
            #[must_use]
            pub const fn new(id: $int) -> Self {
                Self(id)
            }

            /// Its number.
            ///
            /// # Examples
            ///
            #[doc = concat!("See [`", stringify!($id), "`].")]
            #[must_use]
            pub const fn get(self) -> $int {
                self.0
            }
        }

        impl From<$int> for $id {
            fn from(id: $int) -> Self {
                Self(id)
            }
        }

        impl From<$id> for $int {
            fn from(id: $id) -> Self {
                id.0
            }
        }

        impl core::fmt::Display for $id {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(&self.0, f)
            }
        }

        impl core::fmt::LowerHex for $id {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::LowerHex::fmt(&self.0, f)
            }
        }

        impl core::fmt::UpperHex for $id {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::UpperHex::fmt(&self.0, f)
            }
        }
    )*};
}

pub(crate) use id_conversions;

/// `from_guid` of an id the game numbers by the [`Guid::fnv1a`] of a
/// resource's GUID.
///
/// [`Guid::fnv1a`]: crate::content::Guid::fnv1a
macro_rules! guid_ids {
    ($($id:ident),* $(,)?) => {$(
        impl $id {
            /// The id of the resource with GUID `guid`, as the game numbers
            /// it, in a `const` too: for a mod's own content, its
            /// [`Id::guid`](crate::content::Id::guid).
            ///
            /// # Examples
            ///
            /// ```
            /// use enshwick::content::Id;
            ///
            /// const LOCKER: Id = Id::new("you.lockers:locker");
            #[doc = concat!("let id = enshwick::abi::", stringify!($id), "::from_guid(LOCKER.guid());")]
            /// assert_eq!(id.get(), LOCKER.guid().fnv1a());
            /// ```
            #[must_use]
            pub const fn from_guid(guid: crate::content::Guid) -> Self {
                Self(guid.fnv1a())
            }

            /// The id of the resource a mod serves under the content id `id`
            /// (`<mod>:<name>`); a malformed literal fails the build when
            /// used in a constant.
            ///
            /// # Panics
            ///
            /// On a malformed id.
            ///
            /// # Examples
            ///
            /// ```
            /// use enshwick::content::Id;
            ///
            #[doc = concat!("const WARMTH: enshwick::abi::", stringify!($id), " = enshwick::abi::", stringify!($id), "::named(\"my-mod:warmth\");")]
            #[doc = concat!("assert_eq!(WARMTH, enshwick::abi::", stringify!($id), "::from_guid(Id::new(\"my-mod:warmth\").guid()));")]
            /// ```
            #[must_use]
            pub const fn named(id: &str) -> Self {
                Self::from_guid(crate::content::Guid::named(id))
            }
        }
    )*};
}

id_conversions!(
    Entity(u32),
    ItemId(u32),
    PlayerKey(u64),
    RecipeId(u32),
    SkillNode(u32),
    KnowledgeId(u32),
    BuffTypeId(u32),
    NpcId(u32),
);

guid_ids!(ItemId, RecipeId, KnowledgeId, BuffTypeId, NpcId);

const _: () = {
    assert!(size_of::<Entity>() == size_of::<u32>() && align_of::<Entity>() == align_of::<u32>());
    assert!(size_of::<ItemId>() == size_of::<u32>() && align_of::<ItemId>() == align_of::<u32>());
    assert!(
        size_of::<PlayerKey>() == size_of::<u64>() && align_of::<PlayerKey>() == align_of::<u64>()
    );
    assert!(
        size_of::<RecipeId>() == size_of::<u32>() && align_of::<RecipeId>() == align_of::<u32>()
    );
    assert!(
        size_of::<SkillNode>() == size_of::<u32>() && align_of::<SkillNode>() == align_of::<u32>()
    );
    assert!(
        size_of::<KnowledgeId>() == size_of::<u32>()
            && align_of::<KnowledgeId>() == align_of::<u32>()
    );
    assert!(
        size_of::<BuffTypeId>() == size_of::<u32>()
            && align_of::<BuffTypeId>() == align_of::<u32>()
    );
    assert!(size_of::<NpcId>() == size_of::<u32>() && align_of::<NpcId>() == align_of::<u32>());
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_id_converts_to_and_from_its_number() {
        let item = ItemId::from(1_042_u32);
        assert_eq!(u32::from(item), 1_042);
        assert_eq!(item, ItemId::new(1_042));
        assert_eq!(item.get(), 1_042);
    }

    #[test]
    fn a_player_key_converts_to_and_from_its_number() {
        let key = PlayerKey::from(u64::MAX - 1);
        assert_eq!(u64::from(key), u64::MAX - 1);
        assert_eq!(key, PlayerKey::new(u64::MAX - 1));
        assert_eq!(key.get(), u64::MAX - 1);
    }

    #[test]
    fn a_recipe_id_converts_to_and_from_its_number() {
        let recipe = RecipeId::from(77_u32);
        assert_eq!(u32::from(recipe), 77);
        assert_eq!(recipe, RecipeId::new(77));
        assert_eq!(recipe.get(), 77);
    }

    #[test]
    fn an_id_prints_in_hex_as_its_number_does() {
        assert_eq!(format!("{:#x}", ItemId::new(0x5e01)), "0x5e01");
        assert_eq!(format!("{:016X}", PlayerKey::new(0xab)), "00000000000000AB");
        assert_eq!(format!("{:x}", RecipeId::new(255)), "ff");
    }

    #[test]
    fn every_id_displays_as_its_number() {
        assert_eq!(Entity(1210).to_string(), "1210");
        assert_eq!(ItemId::new(1_042).to_string(), "1042");
        assert_eq!(PlayerKey::new(u64::MAX).to_string(), u64::MAX.to_string());
        assert_eq!(RecipeId::new(77).to_string(), "77");
        assert_eq!(SkillNode::new(0x2a).to_string(), "42");
        assert_eq!(KnowledgeId::new(29).to_string(), "29");
        assert_eq!(format!("{:>6}", ItemId::new(7)), "     7");
    }

    #[test]
    fn a_named_id_is_the_id_of_its_content_guid() {
        let id = crate::content::Id::new("my-mod:warmth");
        assert_eq!(
            BuffTypeId::named("my-mod:warmth"),
            BuffTypeId::from_guid(id.guid())
        );
        assert_eq!(
            NpcId::named("my-mod:smith").get(),
            crate::content::Guid::named("my-mod:smith").fnv1a()
        );
        assert_eq!(
            ItemId::named("my-mod:warmth").get(),
            BuffTypeId::named("my-mod:warmth").get()
        );
        assert_eq!(NpcId::new(7).to_string(), "7");
    }

    #[test]
    fn a_recipe_s_knowledge_is_its_recipe_id() {
        assert_eq!(KnowledgeId::from(RecipeId::new(77)), KnowledgeId::new(77));
    }

    #[test]
    fn an_id_from_a_guid_is_the_fnv_hash_of_its_bytes() {
        let chest = crate::content::Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
        assert_eq!(ItemId::from_guid(chest).get(), 1_302_084_846);
        assert_eq!(RecipeId::from_guid(chest).get(), 1_302_084_846);
        assert_eq!(KnowledgeId::from_guid(chest).get(), 1_302_084_846);
    }

    #[test]
    fn an_entity_of_zero_is_no_entity() {
        assert_eq!(Entity::from_raw(0), None);
        assert_eq!(Entity::from_raw(9), Some(Entity(9)));
        assert!(Entity::NONE.is_none());
        assert!(!Entity(9).is_none());
    }

    /// The postcard bytes of `value`.
    fn bytes<T: serde::Serialize>(value: &T) -> Vec<u8> {
        postcard::to_allocvec(value).unwrap()
    }

    /// `T` read back from the bytes of `number`.
    fn read<T: serde::de::DeserializeOwned, N: serde::Serialize>(number: N) -> T {
        postcard::from_bytes(&bytes(&number)).unwrap()
    }

    #[test]
    fn every_lasting_id_serializes_as_its_bare_number() {
        assert_eq!(bytes(&ItemId::new(1_042)), bytes(&1_042_u32));
        assert_eq!(bytes(&PlayerKey::new(u64::MAX)), bytes(&u64::MAX));
        assert_eq!(bytes(&RecipeId::new(77)), bytes(&77_u32));
    }

    #[test]
    fn every_lasting_id_deserializes_from_its_bare_number() {
        assert_eq!(read::<ItemId, _>(9_u32), ItemId::new(9));
        assert_eq!(read::<PlayerKey, _>(u64::MAX), PlayerKey::new(u64::MAX));
        assert_eq!(read::<RecipeId, _>(77_u32), RecipeId::new(77));
    }
}
