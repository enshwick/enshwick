//! The game's item set categories, for a [`NewItemSet`](super::NewItemSet).
//!
//! Each is a group of the journal's collection screen. The journal holds a
//! category as the key of its title text; the names here are enshwick's,
//! from those titles' English texts on client 426, and the keys are the 20
//! categories of server 387 data. A key under which the running game's
//! journal lists no set is refused like any other.
//!
//! ```
//! use enshwick::content::journal::item_set_category;
//!
//! assert!(item_set_category::ALL.contains(&item_set_category::COLLECTIONS));
//! ```

use super::ItemSetCategory;

/// "Building Blocks".
pub const BUILDING_BLOCKS: ItemSetCategory = ItemSetCategory::new(3_126_661_636);
/// "Terrain Materials".
pub const TERRAIN_MATERIALS: ItemSetCategory = ItemSetCategory::new(4_213_158_582);
/// "Player Customization".
pub const PLAYER_CUSTOMIZATION: ItemSetCategory = ItemSetCategory::new(3_040_502_422);
/// "Color Palettes".
pub const COLOR_PALETTES: ItemSetCategory = ItemSetCategory::new(1_282_469_619);
/// "Clothes - Springlands".
pub const CLOTHES_SPRINGLANDS: ItemSetCategory = ItemSetCategory::new(1_538_056_829);
/// "Clothes - Revelwood".
pub const CLOTHES_REVELWOOD: ItemSetCategory = ItemSetCategory::new(2_758_270_008);
/// "Clothes - Nomad Highlands".
pub const CLOTHES_NOMAD_HIGHLANDS: ItemSetCategory = ItemSetCategory::new(2_894_516_005);
/// "Clothes - Kindlewastes".
pub const CLOTHES_KINDLEWASTES: ItemSetCategory = ItemSetCategory::new(318_311_267);
/// "Clothes - Albaneve Summits".
pub const CLOTHES_ALBANEVE_SUMMITS: ItemSetCategory = ItemSetCategory::new(1_475_714_913);
/// "Clothes - Veilwater Basin".
pub const CLOTHES_VEILWATER_BASIN: ItemSetCategory = ItemSetCategory::new(1_477_726_916);
/// "Clothes - Cinder's Cradle".
pub const CLOTHES_CINDERS_CRADLE: ItemSetCategory = ItemSetCategory::new(1_313_062_115);
/// "Clothes - Miscellaneous".
pub const CLOTHES_MISCELLANEOUS: ItemSetCategory = ItemSetCategory::new(623_543_224);
/// "Unique Equipment".
pub const UNIQUE_EQUIPMENT: ItemSetCategory = ItemSetCategory::new(3_036_299_198);
/// "Music".
pub const MUSIC: ItemSetCategory = ItemSetCategory::new(2_689_204_112);
/// "Animals And Pets".
pub const ANIMALS_AND_PETS: ItemSetCategory = ItemSetCategory::new(983_643_856);
/// "Uncooked Food And Herbs".
pub const UNCOOKED_FOOD_AND_HERBS: ItemSetCategory = ItemSetCategory::new(615_556_608);
/// "Cooked Foods And Dishes".
pub const COOKED_FOODS_AND_DISHES: ItemSetCategory = ItemSetCategory::new(2_854_672_060);
/// "Collections".
pub const COLLECTIONS: ItemSetCategory = ItemSetCategory::new(3_331_860_567);
/// "Weapon Gems".
pub const WEAPON_GEMS: ItemSetCategory = ItemSetCategory::new(4_277_734_449);
/// "Enemy Trophies".
pub const ENEMY_TROPHIES: ItemSetCategory = ItemSetCategory::new(4_206_935_582);

/// Every category, in the journal's order.
pub const ALL: [ItemSetCategory; 20] = [
    BUILDING_BLOCKS,
    TERRAIN_MATERIALS,
    PLAYER_CUSTOMIZATION,
    COLOR_PALETTES,
    CLOTHES_SPRINGLANDS,
    CLOTHES_REVELWOOD,
    CLOTHES_NOMAD_HIGHLANDS,
    CLOTHES_KINDLEWASTES,
    CLOTHES_ALBANEVE_SUMMITS,
    CLOTHES_VEILWATER_BASIN,
    CLOTHES_CINDERS_CRADLE,
    CLOTHES_MISCELLANEOUS,
    UNIQUE_EQUIPMENT,
    MUSIC,
    ANIMALS_AND_PETS,
    UNCOOKED_FOOD_AND_HERBS,
    COOKED_FOODS_AND_DISHES,
    COLLECTIONS,
    WEAPON_GEMS,
    ENEMY_TROPHIES,
];

/// The English names of [`ALL`], in its order.
pub(super) const NAMES: [&str; 20] = [
    "Building Blocks",
    "Terrain Materials",
    "Player Customization",
    "Color Palettes",
    "Clothes - Springlands",
    "Clothes - Revelwood",
    "Clothes - Nomad Highlands",
    "Clothes - Kindlewastes",
    "Clothes - Albaneve Summits",
    "Clothes - Veilwater Basin",
    "Clothes - Cinder's Cradle",
    "Clothes - Miscellaneous",
    "Unique Equipment",
    "Music",
    "Animals And Pets",
    "Uncooked Food And Herbs",
    "Cooked Foods And Dishes",
    "Collections",
    "Weapon Gems",
    "Enemy Trophies",
];
