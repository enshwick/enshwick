//! Prop types from mods: placeable items that place a template of the mod's
//! own.
//!
//! A prop is built from a vanilla prop, with a recipe that lists it in the
//! build menu beside the vanilla one, and optionally a crafting prop that
//! recipes made near it require.
//!
//! [`Load::add_prop`](../../struct.Load.html#method.add_prop) copies the vanilla prop's item and the template it
//! places; the item places the copy, so a placed mod prop reports the mod's
//! item in `world::Prop::item`. A crafting prop, such as
//! a mortar beside a workbench, takes one bit of the game's crafting prop
//! mask, which all mods share; the loader refuses one past its width.
//!
//! ```
//! use enshwick::content::{Guid, id};
//! use enshwick::content::items::NewItem;
//! use enshwick::content::props::NewProp;
//!
//! const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let crate_prop = load.add_prop(NewProp {
//!         recipe_cost: Some(1),
//!         ..NewProp::new(NewItem::new(id!("crate"), CHEST))
//!     })?;
//!     assert_eq!(crate_prop.template.guid(), id!("crate").part("template").guid());
//!     Ok(())
//! }
//! ```

use crate::__load::*;
use crate::engine_ids as ids;
use crate::resources::{Registry, Value};
use crate::{ItemId, Off};

use super::items::NewItem;
use super::recipes::{NewRecipe, RECIPE_REGISTRY};
use super::templates::{ComponentChange, NewTemplate, TemplateId};
use super::workshops::WORKSHOP_REGISTRY;
use super::{Blob, EngineFacts, Guid, Id, types};

/// The parts a prop's other ids take after its item's id.
const TEMPLATE: &str = "template";
const RECIPE: &str = "recipe";
const CRAFTING_PROP: &str = "crafting-prop";

/// The game's names of the fields a crafting prop is written in: the
/// workshop registry's props and their fields, a recipe's required props and
/// their mask, the component that marks a placed crafting prop, and the
/// crafting menu's mask of props in range, whose width bounds them.
const PROPS: &str = "craftingProps";
pub(crate) const RECIPES: &str = "recipes";
const RECIPE_GUID: &str = "recipeGuid";
const REQUIRED: &str = "requiredProps";
const REQUIRED_MASK: &str = "requiredPropsMask";
const MARK: &str = "ecs::RequiredCraftingProp";
const IN_RANGE: &str = "craftingPropsInRange";

/// A crafting prop: placed in a base, it lets the base's workshops make the
/// recipes that require it.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::props::CraftingPropSetup;
///
/// let mortar = CraftingPropSetup { recipes: vec![Guid::parse("267ab50e-7727-4336-b0e3-bf38e567189f")] };
/// assert_eq!(mortar.recipes.len(), 1);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, crate::Script)]
pub struct CraftingPropSetup {
    /// The recipes, vanilla or added before [`Load::add_prop`](../../struct.Load.html#method.add_prop), that
    /// require the prop besides the props they require already.
    #[script(default)]
    pub recipes: Vec<Guid>,
}

/// A new prop type for [`Load::add_prop`](../../struct.Load.html#method.add_prop).
///
/// The prop's item is [`NewProp::item`]; its other ids derive from the
/// item's with [`Id::part`]: its template `<id>.template`, its recipe
/// `<id>.recipe` and its crafting prop `<id>.crafting-prop`. So the item's
/// id is a plain id, never one derived with [`Id::part`]. Every id is the
/// mod's own and never changes: saves keep the template GUID of each placed
/// prop.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, id};
/// use enshwick::content::items::NewItem;
/// use enshwick::content::props::NewProp;
///
/// let chest = NewProp::new(NewItem::new(id!("crate"), Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d")));
/// assert!(chest.template.is_empty() && chest.recipe_cost.is_none() && chest.crafting_prop.is_none());
/// ```
#[derive(Clone, Debug, crate::Script)]
pub struct NewProp {
    /// The prop's item, a copy of a vanilla prop's item: what it looks like
    /// in the inventory and which template it copies.
    pub item: NewItem,
    /// The changes of the copied template's components, in order.
    #[script(default)]
    pub template: Vec<ComponentChange>,
    /// With a cost, the prop is built by the recipe `<id>.recipe`, a copy
    /// of the vanilla prop's whose ingredient counts are times this, listed
    /// in the build menu after it; `None` adds no recipe.
    pub recipe_cost: Option<u32>,
    /// Makes the prop a crafting prop.
    pub crafting_prop: Option<CraftingPropSetup>,
}

impl NewProp {
    /// A prop of `item`, its template unchanged, with no recipe.
    ///
    /// # Examples
    ///
    /// See [`NewProp`].
    #[must_use]
    pub const fn new(item: NewItem) -> Self {
        Self {
            item,
            template: Vec::new(),
            recipe_cost: None,
            crafting_prop: None,
        }
    }
}

/// A prop type a mod added: its item, the template the item places, and
/// its crafting prop when it is one.
///
/// # Examples
///
/// See [`Load::add_prop`](../../struct.Load.html#method.add_prop).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct PropId {
    /// The prop's item, which `world::Prop::item`
    /// reports for every placed one.
    pub item: ItemId,
    /// The template the item places.
    pub template: TemplateId,
    /// The crafting prop's GUID, for a crafting prop.
    pub crafting_prop: Option<Guid>,
}

/// The template the vanilla item `item` places.
fn placed_by(load: &crate::BaseLoad, item: Guid) -> Result<Guid, Off> {
    let at = load.offset(ids::ITEM_INFO_PLACED_ENTITY)?;
    let info = load
        .host()
        .game_resource(types::ITEM_INFO.key(item)?)
        .ok_or_else(|| Off::new(format!("prop: {item} is no vanilla item")))?;
    info.get(at..at + 16)
        .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
        .map(Guid)
        .filter(|template| template.0 != [0; 16])
        .ok_or_else(|| Off::new(format!("prop: the vanilla item {item} places nothing")))
}

/// `info` placing `template`; `None` when it is too short.
fn place(info: &[u8], at: usize, template: Guid) -> Option<Vec<u8>> {
    let mut item = Blob::new(info);
    item.write(at, &template.0).then(|| item.into_bytes())
}

/// A game id as the codec reads one: a struct of its `value`.
fn game_id(id: u32) -> Value {
    Value::Struct(vec![("value".into(), Value::Uint(id.into()))])
}

/// The change that marks a template as the crafting prop `guid`.
fn crafting_mark(guid: Guid) -> ComponentChange {
    ComponentChange::set_value(
        MARK,
        Value::Struct(vec![
            ("type".into(), Value::Guid(guid)),
            ("craftingPropId".into(), game_id(guid.fnv1a())),
        ]),
    )
}

/// The first bit of the crafting prop mask no vanilla prop of `props`
/// takes: one past the highest index, which need not be the count.
fn first_free(props: &[Value]) -> Result<u8, Off> {
    let next = props
        .iter()
        .map(|prop| match prop.field("craftingPropIndex") {
            Some(Value::Uint(index)) => index.saturating_add(1),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
        .max(u64::try_from(props.len()).unwrap_or(u64::MAX));
    u8::try_from(next).map_err(|_| Off::new("crafting props: more than 255 in the game"))
}

/// The workshop registry `registry` with the crafting prop `entry` after
/// its others.
fn list_prop(mut registry: Value, entry: &Value) -> Result<Value, Off> {
    match registry.field_mut(PROPS) {
        Some(Value::List(props)) => {
            props.push(entry.clone());
            Ok(registry)
        }
        _ => Err(Off::new("workshop registry: no list of crafting props")),
    }
}

/// The recipe registry `registry` with each of `recipes` requiring the
/// crafting prop `id` at mask bit `index`.
fn require_prop(mut registry: Value, recipes: &[Guid], id: u32, index: u8) -> Result<Value, Off> {
    let Some(Value::List(entries)) = registry.field_mut(RECIPES) else {
        return Err(Off::new("recipe registry: no list of recipes"));
    };
    for recipe in entries
        .iter_mut()
        .filter(|recipe| matches!(recipe.field(RECIPE_GUID), Some(Value::Guid(guid)) if recipes.contains(guid)))
    {
        if let Some(Value::List(required)) = recipe.field_mut(REQUIRED) {
            required.push(game_id(id));
        }
        if let Some(Value::Uint(mask)) = recipe.field_mut(REQUIRED_MASK) {
            *mask |= 1 << index;
        }
    }
    Ok(registry)
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_prop`.
    #[doc(hidden)]
    fn add_prop(&mut self, new: NewProp) -> Result<PropId, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn add_prop(&mut self, new: NewProp) -> Result<PropId, Off> {
        let item = new.item.id;
        let part = |part| {
            item.try_part(part)
                .ok_or_else(|| Off::new(format!("prop {item}: the id takes no part {part}")))
        };
        let (template, recipe) = (part(TEMPLATE)?, part(RECIPE)?);
        let like = placed_by(self, new.item.like)?;
        let mut changes = new.template;
        let crafting = match new.crafting_prop {
            Some(setup) => {
                let prop = part(CRAFTING_PROP)?;
                changes.push(crafting_mark(prop.guid()));
                Some((prop, setup))
            }
            None => None,
        };
        let template = self.add_template(NewTemplate {
            changes,
            ..NewTemplate::new(template, like)
        })?;
        let vanilla = new.item.like;
        let name = new
            .item
            .name
            .or_else(|| super::items::vanilla_name_tag(vanilla));
        self.add_item(new.item)?;
        let crafting_prop = crafting.as_ref().map(|(prop, _)| prop.guid());
        if let Some((prop, setup)) = crafting {
            self.add_crafting_prop(prop, item.guid(), name, setup)?;
        }
        let at = self.offset(ids::ITEM_INFO_PLACED_ENTITY)?;
        let (placed, base) = (template.guid(), types::ITEM_INFO.key(vanilla)?);
        for guid in super::renames::old_guids(item.guid())
            .into_iter()
            .chain([item.guid()])
        {
            // A second link on the item, after `add_item`'s on the same base.
            self.serve(types::ITEM_INFO.key(guid)?, base, move |info| {
                place(info, at, placed)
            })?;
        }
        if let Some(cost) = new.recipe_cost {
            self.push_recipe(NewRecipe {
                id: recipe,
                like: vanilla,
                output: item.guid(),
                output_count: 1,
                cost,
                workshop: None,
            })?;
        }
        Ok(PropId {
            item: ItemId::from_guid(item.guid()),
            template,
            crafting_prop,
        })
    }
}

/// The base handle's helpers of this module for the crate's own code.
pub(crate) trait LoadHelpers {
    fn crafting_prop_bounds(&self) -> Result<(u8, u8), Off>;

    fn crafting_prop_index(&self, prop: Guid) -> Result<u8, Off>;

    fn add_crafting_prop(
        &mut self,
        prop: Id,
        item: Guid,
        name: Option<Guid>,
        setup: CraftingPropSetup,
    ) -> Result<(), Off>;
}

impl LoadHelpers for crate::BaseLoad {
    fn crafting_prop_bounds(&self) -> Result<(u8, u8), Off> {
        let registry = Registry::game()?;
        let menu = registry
            .hashed(self.hash(ids::CLIENT_CRAFTING_INTERACTION)?)
            .ok_or_else(|| Off::new("crafting props: the crafting menu is not reflected"))?;
        let width = registry
            .all_fields(menu)
            .iter()
            .find(|field| field.name() == IN_RANGE)
            .and_then(|field| field.type_name())
            .and_then(|name| registry.named(name))
            .and_then(|mask| u8::try_from(mask.size() * 8).ok())
            .ok_or_else(|| Off::new("crafting props: the mask is not reflected"))?;
        let vanilla = self
            .host()
            .game_value(types::WORKSHOP_REGISTRY_RESOURCE.key(WORKSHOP_REGISTRY)?)?;
        let Some(Value::List(props)) = vanilla.field(PROPS) else {
            return Err(Off::new("workshop registry: no list of crafting props"));
        };
        Ok((first_free(props)?, width))
    }

    fn crafting_prop_index(&self, prop: Guid) -> Result<u8, Off> {
        let (first, width) = self.crafting_prop_bounds()?;
        let host = self.host();
        let mut index = 0u8;
        let status = (host.api().crafting_prop)(
            host.handle(),
            &raw const prop.0,
            first,
            width,
            &raw mut index,
        );
        crate::load::ok(status, index, || format!("crafting prop {prop}"))
    }

    fn add_crafting_prop(
        &mut self,
        prop: Id,
        item: Guid,
        name: Option<Guid>,
        setup: CraftingPropSetup,
    ) -> Result<(), Off> {
        let guid = prop.guid();
        self.declare(prop)?;
        let index = self.crafting_prop_index(guid)?;
        let entry = Value::Struct(vec![
            ("craftingPropGuid".into(), Value::Guid(guid)),
            ("craftingPropId".into(), game_id(guid.fnv1a())),
            ("craftingPropIndex".into(), Value::Uint(index.into())),
            ("name".into(), game_id(name.map_or(0, Guid::fnv1a))),
            ("item".into(), game_id(item.fnv1a())),
            ("itemRef".into(), Value::Guid(item)),
        ]);
        let host = self.host();
        let registry = types::WORKSHOP_REGISTRY_RESOURCE.key(WORKSHOP_REGISTRY)?;
        crate::resources::serve_value(host, registry, registry, move |value| {
            list_prop(value, &entry)
        })?;
        if !setup.recipes.is_empty() {
            let registry = types::RECIPE_REGISTRY_RESOURCE.key(RECIPE_REGISTRY)?;
            crate::resources::serve_value(host, registry, registry, move |value| {
                require_prop(value, &setup.recipes, guid.fnv1a(), index)
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(props: usize) -> Value {
        Value::Struct(vec![(
            PROPS.into(),
            Value::List(vec![Value::Struct(Vec::new()); props]),
        )])
    }

    fn recipe(guid: Guid, required: Vec<Value>, mask: u64) -> Value {
        Value::Struct(vec![
            (RECIPE_GUID.into(), Value::Guid(guid)),
            (REQUIRED.into(), Value::List(required)),
            (REQUIRED_MASK.into(), Value::Uint(mask)),
        ])
    }

    #[test]
    fn a_crafting_prop_lists_after_the_game_s() {
        let entry = Value::Struct(vec![("craftingPropIndex".into(), Value::Uint(51))]);
        let listed = list_prop(registry(51), &entry).unwrap();
        let Some(Value::List(props)) = listed.field(PROPS) else {
            panic!("{listed:?}")
        };
        assert_eq!((props.len(), props.last()), (52, Some(&entry)));
        assert!(list_prop(Value::Struct(Vec::new()), &entry).is_err());
    }

    #[test]
    fn the_first_free_bit_is_past_the_highest_vanilla_index_not_the_count() {
        let indexed = |indices: &[u64]| -> Vec<Value> {
            indices
                .iter()
                .map(|&i| Value::Struct(vec![("craftingPropIndex".into(), Value::Uint(i))]))
                .collect()
        };
        assert_eq!(first_free(&indexed(&[0, 1, 2])), Ok(3));
        assert_eq!(first_free(&indexed(&[0, 1, 5])), Ok(6), "a gap");
        assert_eq!(first_free(&[]), Ok(0));
        assert!(first_free(&indexed(&[255])).is_err());
    }

    #[test]
    fn the_named_recipes_require_the_prop_by_id_and_mask_bit_and_the_rest_keep_theirs() {
        let (named, other) = (Guid([1; 16]), Guid([2; 16]));
        let mortar = game_id(2_371_410_940);
        let registry = Value::Struct(vec![(
            RECIPES.into(),
            Value::List(vec![
                recipe(named, vec![mortar.clone()], 1),
                recipe(other, Vec::new(), 0),
            ]),
        )]);
        let changed = require_prop(registry, &[named], 7, 51).unwrap();
        let Some(Value::List(recipes)) = changed.field(RECIPES) else {
            panic!("{changed:?}")
        };
        assert_eq!(
            recipes[0],
            recipe(named, vec![mortar, game_id(7)], 1 | 1 << 51)
        );
        assert_eq!(recipes[1], recipe(other, Vec::new(), 0));
    }

    #[test]
    fn a_vanilla_crafting_prop_s_id_is_the_hash_of_its_guid() {
        // The mortar's in the workshop registry of server 387.
        let mortar = Guid::parse("b3ff29bd-d4f2-4f41-91c1-4d977e8f399d");
        assert_eq!(mortar.fnv1a(), 2_371_410_940);
    }

    #[test]
    fn placing_writes_the_template_where_the_item_names_it() {
        let placed = place(&[0; 24], 8, Guid([5; 16])).unwrap();
        assert_eq!(&placed[8..24], &[5; 16]);
        assert!(place(&[0; 8], 8, Guid([5; 16])).is_none());
    }
}
