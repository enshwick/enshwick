//! The bounds of the game's crafting prop mask: its bits, and the first bit
//! the game's own props leave free, read once from the reflection and the
//! archive, so a mod adds a crafting prop without naming either.

#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
use enshwick_base::engine_ids as ids;
use enshwick_base::resources::Value;
#[cfg(windows)]
use enshwick_core::content::types as content_types;
#[cfg(windows)]
use enshwick_core::content::workshops::WORKSHOP_REGISTRY;
#[cfg(windows)]
use enshwick_gamedata::catalogue as names;

#[cfg(windows)]
use crate::engine::catalogue::types;

/// The game's names of the fields the bounds are read from: the crafting
/// menu's mask of the props in range, and the workshop registry's props
/// with their indices.
#[cfg(windows)]
const IN_RANGE: &str = "craftingPropsInRange";
#[cfg(windows)]
const PROPS: &str = "craftingProps";
const INDEX: &str = "craftingPropIndex";

/// The first bit of the mask no vanilla prop takes and the mask's bits;
/// `None` while the reflection or the archive does not give them, which a
/// later call reads again.
#[cfg(windows)]
pub(crate) fn bounds() -> Option<(u8, u8)> {
    static BOUNDS: OnceLock<(u8, u8)> = OnceLock::new();
    if let Some(bounds) = BOUNDS.get() {
        return Some(*bounds);
    }
    let read = read()?;
    Some(*BOUNDS.get_or_init(|| read))
}

#[cfg(windows)]
fn read() -> Option<(u8, u8)> {
    let loader = crate::engine::loader::loader()?;
    let registry = loader.reflection.registry();
    let menu = registry.named(names::type_name(ids::CLIENT_CRAFTING_INTERACTION))?;
    let width = registry
        .all_fields(menu)
        .iter()
        .find(|field| field.name() == IN_RANGE)
        .and_then(|field| field.type_name())
        .and_then(|name| registry.named(name))
        .and_then(|mask| u8::try_from(mask.size() * 8).ok())?;
    let bytes = crate::engine::archive::game()?
        .resource(&WORKSHOP_REGISTRY.key_by_hash(types::WORKSHOP_REGISTRY))?;
    let workshops = registry
        .decode(
            names::type_name(content_types::WORKSHOP_REGISTRY_RESOURCE.id()),
            &bytes,
        )
        .ok()?;
    let Some(Value::List(props)) = workshops.field(PROPS) else {
        return None;
    };
    Some((first_free(props)?, width))
}

/// The first bit of the crafting prop mask no vanilla prop of `props`
/// takes: one past the highest index, which need not be the count.
fn first_free(props: &[Value]) -> Option<u8> {
    let next = props
        .iter()
        .map(|prop| match prop.field(INDEX) {
            Some(Value::Uint(index)) => index.saturating_add(1),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
        .max(u64::try_from(props.len()).unwrap_or(u64::MAX));
    u8::try_from(next).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(index: u64) -> Value {
        Value::Struct(vec![(INDEX.into(), Value::Uint(index))])
    }

    #[test]
    fn the_first_free_bit_is_past_the_highest_index_or_the_count() {
        assert_eq!(first_free(&[]), Some(0));
        assert_eq!(first_free(&[prop(0), prop(1)]), Some(2));
        assert_eq!(first_free(&[prop(5), prop(1)]), Some(6), "a gap below");
        assert_eq!(
            first_free(&[prop(0), prop(0), prop(0)]),
            Some(3),
            "no fewer than the props listed"
        );
        assert_eq!(first_free(&[prop(255)]), None, "past a byte");
    }
}
