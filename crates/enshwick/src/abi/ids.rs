//! The ids the ABI carries, each a transparent newtype over the game's
//! integer so a struct field or a call says which id it takes.

/// An entity id, as the engine numbers entities: a character, a prop, an item
/// with its own entity, anything the world holds.
///
/// The engine's id 0 stands for no entity; the safe API hands it over as
/// `None` in an `Option<Entity>`, never as an `Entity`.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
///
/// let entity = Entity(1210);
/// assert_eq!(entity.0, 1210);
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
pub struct ItemId(pub(super) u32);

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
pub struct PlayerKey(pub(super) u64);

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
pub struct RecipeId(pub(super) u32);

/// `new`, `get` and the `From` conversions both ways of an id newtype.
macro_rules! id_conversions {
    ($($id:ident($int:ty)),* $(,)?) => {$(
        impl $id {
            /// The id of the game's number `id`, in a `const` too.
            ///
            /// # Examples
            ///
            #[doc = concat!("See [`", stringify!($id), "`].")]
            #[must_use]
            pub const fn new(id: $int) -> Self {
                Self(id)
            }

            /// The game's number.
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
    )*};
}

id_conversions!(ItemId(u32), PlayerKey(u64), RecipeId(u32));

const _: () = {
    assert!(size_of::<Entity>() == size_of::<u32>() && align_of::<Entity>() == align_of::<u32>());
    assert!(size_of::<ItemId>() == size_of::<u32>() && align_of::<ItemId>() == align_of::<u32>());
    assert!(
        size_of::<PlayerKey>() == size_of::<u64>() && align_of::<PlayerKey>() == align_of::<u64>()
    );
    assert!(
        size_of::<RecipeId>() == size_of::<u32>() && align_of::<RecipeId>() == align_of::<u32>()
    );
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
    fn every_id_serializes_as_its_bare_number() {
        assert_eq!(bytes(&Entity(1210)), bytes(&1210_u32));
        assert_eq!(bytes(&ItemId::new(1_042)), bytes(&1_042_u32));
        assert_eq!(bytes(&PlayerKey::new(u64::MAX)), bytes(&u64::MAX));
        assert_eq!(bytes(&RecipeId::new(77)), bytes(&77_u32));
    }

    #[test]
    fn every_id_deserializes_from_its_bare_number() {
        assert_eq!(read::<Entity, _>(1210_u32), Entity(1210));
        assert_eq!(read::<ItemId, _>(9_u32), ItemId::new(9));
        assert_eq!(read::<PlayerKey, _>(u64::MAX), PlayerKey::new(u64::MAX));
        assert_eq!(read::<RecipeId, _>(77_u32), RecipeId::new(77));
    }
}
