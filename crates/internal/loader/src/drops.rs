//! Loose item stacks mods drop on the ground, on the server's game thread
//! between steps.
//!
//! A drop is the game's generic pickup template, spawned with the item's
//! `PickupItem` as its first component, as the game spills loot (server
//! 26430253, `sys.random_pickup_loot_spawner`): the template carries the
//! model setup, the interaction offer any player takes it by, gravity and
//! `DoNotSave`, and `sys.pickup_item_setup` fills the rest from the item.

use std::sync::OnceLock;

use enshwick::abi::{DropRequest, Header, SpawnRequest, Status};
use enshwick::affixes::ItemRarity;

use crate::host::Loader;

/// The template the game spills loose loot with.
const TEMPLATE: &str = "Base_LootPickup_Generic_Gravity";
/// The component a pickup holds its item in.
const PICKUP: &str = "keen::ecs::PickupItem";

/// The pickup template and where `PickupItem` keeps its fields.
#[derive(Debug, Clone, Copy)]
struct Layout {
    template: [u8; 16],
    hash: u32,
    size: usize,
    item: usize,
    level: usize,
    rarity: usize,
    amount: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

/// The layout, read once: the template by its name in the game's data,
/// which reads every template's name the first time.
fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let reflection = &loader.reflection;
            let field = |name: &str| {
                reflection
                    .field_offset(PICKUP, name)
                    .ok()
                    .and_then(|at| usize::try_from(at).ok())
            };
            let info = reflection.type_info(PICKUP).ok()?;
            let vanilla = enshwick::content::vanilla::Vanilla::new(
                &enshwick_gamedata::catalogue::Reflected(reflection),
                |key| crate::archive::game()?.resource(&key),
            );
            let template = match vanilla.map(|vanilla| vanilla.template(TEMPLATE)) {
                Ok(Ok(guid)) => guid.0,
                why => {
                    crate::log!(WARN, "item drops off: {TEMPLATE}: {why:?}");
                    return None;
                }
            };
            Some(Layout {
                template,
                hash: crate::world::type_hash(info.va())?,
                size: usize::try_from(info.size()).ok()?,
                item: field("itemId")?,
                level: field("level")?,
                rarity: field("rarity")?,
                amount: field("pickupAmount")?,
            })
        })
        .as_ref()
}

/// `PickupItem`'s bytes for `request` on `layout`, `None` for a count or a
/// rarity the game does not hold.
fn pickup(layout: &Layout, request: &DropRequest) -> Option<Vec<u8>> {
    let amount = u16::try_from(request.count).ok().filter(|&n| n != 0)?;
    let rarity = u8::try_from(request.rarity)
        .ok()
        .filter(|&r| ItemRarity::from_raw(r).is_some())?;
    let mut bytes = vec![0u8; layout.size];
    let mut put = |at: usize, part: &[u8]| {
        bytes
            .get_mut(at..at + part.len())
            .map(|slot| slot.copy_from_slice(part))
    };
    put(layout.item, &request.item.get().to_le_bytes())?;
    put(layout.level, &request.level.to_le_bytes())?;
    put(layout.rarity, &[rarity])?;
    put(layout.amount, &amount.to_le_bytes())?;
    Some(bytes)
}

/// Drops the stack `request` asks for; the pickup's entity.
pub(crate) fn drop_items(loader: &Loader, request: &DropRequest) -> Result<u32, Status> {
    crate::world::server()?;
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let bytes = pickup(layout, request).ok_or(Status::BAD_ARGS)?;
    crate::inventory::item_info(loader, request.item.get()).map_err(|_| Status::REFUSED)?;
    let spawn = SpawnRequest {
        header: Header::of::<SpawnRequest>(),
        template: layout.template,
        position: request.position,
        orientation: [0.0, 0.0, 0.0, 1.0],
        level: 0,
        reserved: 0,
        lifetime_ns: 0,
        flags: 0,
    };
    crate::spawns::spawn_with(loader, &spawn, vec![(layout.hash, bytes)], 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
        template: [1; 16],
        hash: 7,
        size: 28,
        item: 0,
        level: 20,
        rarity: 24,
        amount: 26,
    };

    fn request(count: u32, rarity: u32) -> DropRequest {
        DropRequest {
            header: Header::of::<DropRequest>(),
            item: enshwick::ItemId::new(0x2a),
            count,
            position: [0; 3],
            level: 9,
            rarity,
        }
    }

    #[test]
    fn a_pickup_holds_the_item_level_rarity_and_count_where_the_game_reads_them() {
        let bytes = pickup(&LAYOUT, &request(300, 2)).unwrap();
        assert_eq!(bytes.len(), 28);
        assert_eq!(bytes[0..4], 0x2a_u32.to_le_bytes());
        assert_eq!(bytes[4..20], [0; 16], "no item reference");
        assert_eq!(bytes[20..24], 9_u32.to_le_bytes());
        assert_eq!(bytes[24], 2);
        assert_eq!(bytes[26..28], 300_u16.to_le_bytes());
    }

    #[test]
    fn a_count_of_none_or_past_a_pickup_and_an_unknown_rarity_are_refused() {
        assert!(pickup(&LAYOUT, &request(0, 0)).is_none());
        assert!(pickup(&LAYOUT, &request(65_536, 0)).is_none());
        assert!(pickup(&LAYOUT, &request(65_535, 0)).is_some());
        assert!(pickup(&LAYOUT, &request(1, 200)).is_none());
    }
}
