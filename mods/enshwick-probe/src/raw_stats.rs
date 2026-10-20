//! The raw stat data of a character beside what the game shows: every value
//! of its health, stamina and mana sets, the replicated copies and the API's
//! reading, for telling which slot holds which value.

use core::fmt::Write as _;

use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::world::{ComponentType, World};
use enshwick_loader_api::{Load, Off};

/// Bytes read of a set whose size reflection does not give.
const FALLBACK_SIZE: u32 = 0x40;

/// One attribute set and what the API expects of it.
#[derive(Debug, Clone, Copy)]
struct Set {
    name: &'static str,
    ty: ComponentType,
    size: u32,
    /// The id of its first attribute, which the set keeps.
    root: u64,
    /// The slot the API reads the maximum from.
    max_slot: u64,
}

/// What the raw stat dump needs.
#[derive(Debug)]
pub(crate) struct RawStats {
    world: World,
    sets: Vec<Set>,
    network_health: Option<ComponentType>,
    network_stamina: Option<ComponentType>,
    data_offset: usize,
    flags: usize,
    count: usize,
}

impl RawStats {
    pub(crate) fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        let world = load.world()?;
        let slot = |name: &str| {
            load.constant(name)
                .and_then(|v| usize::try_from(v).map_err(|_| Off::new(format!("{name}: too big"))))
        };
        let mut sets = Vec::new();
        for (name, root, max_slot) in [
            ("Health", "attribute.health", "attribute.health_max_slot"),
            ("Stamina", "attribute.stamina", "attribute.stamina_max_slot"),
            ("Mana", "attribute.mana", "attribute.mana_max_slot"),
        ] {
            let ty_name = format!("keen::ecs::{name}");
            sets.push(Set {
                name,
                ty: world.component_type(load, &ty_name)?,
                size: load.type_size(&ty_name).unwrap_or(FALLBACK_SIZE),
                root: load.constant(root)?,
                max_slot: load.constant(max_slot)?,
            });
        }
        Ok(Self {
            network_health: world.component_type(load, "keen::ecs::NetworkHealth").ok(),
            network_stamina: world.component_type(load, "keen::ecs::NetworkStamina").ok(),
            data_offset: slot("ecs.level_data_offset")?,
            flags: slot("ecs.level_flags")?,
            count: slot("ecs.attribute_count")?,
            world,
            sets,
        })
    }

    /// Logs `entity`'s raw stats with the values the player read off the
    /// screen, `shown`; returns a short answer for the chat.
    pub(crate) fn dump(&self, entity: Entity, shown: &[&str]) -> String {
        let mut text = format!(
            "R6 raw stats of entity {}, the game shows {}:",
            entity.0,
            if shown.is_empty() {
                "(nothing given)".to_owned()
            } else {
                shown.join(" ")
            }
        );
        for set in &self.sets {
            self.set_lines(&mut text, entity, set);
        }
        if let Some(ty) = self.network_health {
            let replicated = self.world.component(entity, ty).map(|data| {
                // SAFETY: NetworkHealth is health then its maximum, two u32,
                // read at once on the game thread.
                unsafe { data.cast::<[u32; 2]>().read_unaligned() }
            });
            let _infallible = write!(text, "\n  NetworkHealth: {replicated:?}");
        }
        if let Some(ty) = self.network_stamina {
            let replicated = self.world.component(entity, ty).map(|data| {
                // SAFETY: NetworkStamina is stamina then its maximum, two
                // u16 (reflection, build 1269579), read on the game thread.
                unsafe { data.cast::<[u16; 2]>().read_unaligned() }
            });
            let _infallible = write!(text, "\n  NetworkStamina: {replicated:?}");
        }
        let vitals = self.world.vitals(entity);
        let _infallible = write!(text, "\n  the API reads {vitals:?}");
        enshwick_loader_api::info!("{text}");
        format!("{vitals:?}; the raw sets are in probe.log")
    }

    fn set_lines(&self, text: &mut String, entity: Entity, set: &Set) {
        let Some(data) = self.world.component(entity, set.ty) else {
            let _infallible = write!(text, "\n  {}: none", set.name);
            return;
        };
        let size = set.size as usize;
        // SAFETY: the entity's live component of this type, `size` bytes
        // long by reflection, read at once on the game thread.
        let bytes = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) };
        let byte = |at: usize| bytes.get(at).copied().unwrap_or(0);
        let root = bytes
            .get(0..4)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let offset = usize::from(u16::from_le_bytes([
            byte(self.data_offset),
            byte(self.data_offset + 1),
        ]));
        let flags = byte(self.flags);
        let count = usize::from(byte(self.count));
        let values: Vec<i32> = (0..count)
            .filter_map(|slot| {
                let at = offset + slot * 4;
                let b = bytes.get(at..at + 4)?;
                Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            })
            .collect();
        let _infallible = write!(
            text,
            "\n  {}: root {root:#010x} (the API expects {:#010x}), values at +{offset:#x}, flags {flags:#x}, \
             {count} slots: {values:?}; the API reads the current value from slot 0 and the maximum from slot {}",
            set.name, set.root, set.max_slot
        );
        let _infallible = write!(text, "\n  {} bytes:", set.name);
        for word in bytes.chunks(4) {
            let mut padded = [0u8; 4];
            padded[..word.len()].copy_from_slice(word);
            let _infallible = write!(text, " {:08x}", u32::from_le_bytes(padded));
        }
    }
}
