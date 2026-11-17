//! The fake's containers: a vanilla chest whose template sets up its slots.

use enshwick_core::__fake::ids;
use enshwick_core::content::Guid;
use enshwick_core::resources::{Field, Kind, Registry, Type, Value, Variant, fnv1a32};

use crate::test::content::templates::TEMPLATE;
use crate::test::{FakeHost, PROP_ITEM};

/// A vanilla chest the fake holds after [`FakeHost::containers`]: its item
/// places a `Prop` template whose inventory setup holds
/// [`CHEST_SLOTS`] slots.
pub const CHEST_ITEM: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000c4e5700");

/// How many slots [`CHEST_ITEM`]'s template sets up.
pub const CHEST_SLOTS: u8 = 24;

const CHEST_TEMPLATE: Guid = Guid::parse("c0ffee00-0000-4000-8000-00000c4e5701");
const SETUP: &str = "demo::ecs::InventorySetup";
/// Where the fake's inventory setup keeps its generic and loot slot counts,
/// as the game's does.
const GENERIC: u32 = 0x51;
const AVAILABLE: u32 = 0x52;

impl FakeHost {
    /// Containers: [`FakeHost::props`] and a vanilla [`CHEST_ITEM`] whose
    /// template sets up [`CHEST_SLOTS`] slots, with the layout
    /// [`ContainerLayout::resolve`](enshwick_core::content::containers::ContainerLayout::resolve)
    /// reads. What `Load::add_container`, with the `unstable` feature,
    /// resolves.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::FakeHost;
    ///
    /// let host = FakeHost::new().containers();
    /// assert!(host.hash("inventory_setup").is_ok());
    /// ```
    #[must_use]
    pub fn containers(self) -> Self {
        let setup = Type::new(SETUP, Kind::Struct, 0x54, 4)
            .with_inner("demo::ecs::Component")
            .with_fields(vec![
                Field::new("genericSlotCount", GENERIC, "demo::uint8"),
                Field::new("availableSlotCount", AVAILABLE, "demo::uint8"),
            ]);
        let host = self.props().types([setup]);
        let registry = Registry::new(host.content.type_list.clone());
        let template_hash = fnv1a32(TEMPLATE.as_bytes());
        let placed_at = host
            .base
            .constants
            .get(ids::ITEM_INFO_PLACED_ENTITY)
            .and_then(|at| usize::try_from(*at).ok());
        let prop_info = host
            .content
            .resources
            .iter()
            .find(|((guid, _, _), _)| *guid == PROP_ITEM)
            .map(|((_, item_hash, _), info)| (*item_hash, info.clone()));
        let (Some(at), Some((item_hash, mut info))) = (placed_at, prop_info) else {
            return host;
        };
        let Some(prop_template) = info
            .get(at..at + 16)
            .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
            .map(Guid)
        else {
            return host;
        };
        let chest = host
            .content
            .resources
            .get(&(prop_template, template_hash, 0))
            .and_then(|bytes| registry.decode(TEMPLATE, bytes).ok())
            .and_then(|mut template| {
                let slots = Value::Struct(vec![
                    ("genericSlotCount".into(), Value::Uint(CHEST_SLOTS.into())),
                    ("availableSlotCount".into(), Value::Uint(CHEST_SLOTS.into())),
                ]);
                match template.field_mut("components") {
                    Some(Value::List(components)) => {
                        components.push(Value::Variant(Box::new(Variant::new(SETUP, slots))));
                    }
                    _ => return None,
                }
                registry.encode(TEMPLATE, &template, None).ok()
            });
        let Some(chest) = chest else {
            return host;
        };
        info[..4].copy_from_slice(&CHEST_ITEM.fnv1a().to_le_bytes());
        info[4..20].copy_from_slice(&CHEST_ITEM.0);
        info[at..at + 16].copy_from_slice(&CHEST_TEMPLATE.0);
        host.resource(CHEST_TEMPLATE.key_by_hash(template_hash), chest)
            .resource(CHEST_ITEM.key_by_hash(item_hash), info)
            .type_hash(ids::INVENTORY_SETUP, fnv1a32(SETUP.as_bytes()))
            .constant(ids::TEMPLATE_COMPONENTS, 0x10)
            .constant(ids::INVENTORY_SETUP_GENERIC_SLOTS, GENERIC.into())
            .constant(ids::INVENTORY_SETUP_AVAILABLE_SLOTS, AVAILABLE.into())
    }
}
