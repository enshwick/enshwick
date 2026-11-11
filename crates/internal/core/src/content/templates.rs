//! Entity templates from mods: a copy of a vanilla template under the mod's
//! content id, and changes to a template's components, in memory on the
//! server and the client alike.
//!
//! A template is what a placed object or a creature is made of: its family
//! and a list of components. [`Load::add_template`] copies a vanilla one,
//! which keeps its family, model and proxy model, so the copy looks like it;
//! [`Load::change_template`] changes a vanilla one. Neither is written to a
//! save or the archive. Mods that change one template change it in load
//! order, each on the one before's result. With the `typed` feature a
//! [`ComponentChange`] names its component by its generated type.
//!
//! ```
//! # extern crate enshwick_core as enshwick;
//! # #[cfg(feature = "typed")]
//! # mod typed {
//! use enshwick::content::{Guid, Id, id};
//! use enshwick::content::templates::{ComponentChange, NewTemplate};
//! use enshwick::typed::FactionType;
//! use enshwick::typed::ecs::Faction;
//!
//! const WOLF: Guid = Guid::parse("b7a4f1c0-4c1e-4f39-9d5b-2a0f3c6d8e11");
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let vukah_wolf = load.add_template(NewTemplate {
//!         changes: vec![ComponentChange::edit(|faction: &mut Faction| {
//!             faction.faction = FactionType::Vukah;
//!         })],
//!         ..NewTemplate::new(id!("vukah-wolf"), WOLF)
//!     })?;
//!     assert_eq!(vukah_wolf.guid(), Id::new("you.mod:vukah-wolf").guid());
//!     Ok(())
//! }
//! # }
//! ```

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::resources::{Registry, Type, Value, Variant};
use crate::{Load, Off};

use super::{Blob, Guid, Id, types};

/// The game's list of every template.
pub const TEMPLATE_COLLECTION: Guid = Guid::parse("377f61dc-c7b8-4e4f-830f-7d2c3e8b7fd8");

/// An entity template the mod added: its GUID, which saves keep for every
/// placed object or creature made from it.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::id;
/// use enshwick::content::templates::TemplateId;
///
/// let id = id!("vukah-wolf");
/// assert_eq!(TemplateId::new(id.guid()).guid(), id.guid());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct TemplateId(Guid);

impl TemplateId {
    /// The template with GUID `guid`.
    ///
    /// # Examples
    ///
    /// See [`TemplateId`].
    #[must_use]
    pub const fn new(guid: Guid) -> Self {
        Self(guid)
    }

    /// The template's GUID.
    ///
    /// # Examples
    ///
    /// See [`TemplateId`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.0
    }
}

/// A component's value edit, or a check of one: the codec's value in, the
/// new value or the reason it does not fit out.
pub(crate) type ValueFn = Arc<dyn Fn(&Value) -> Result<Value, Off> + Send + Sync>;

/// Whether a value reads as a generated type.
#[cfg(feature = "typed")]
type Check = Arc<dyn Fn(&Value) -> Result<(), crate::typed::ShapeError> + Send + Sync>;

/// A component's byte edit: its bytes as the reflected type lays them out;
/// `false` refuses them.
type BytesFn = Arc<dyn Fn(&mut [u8]) -> bool + Send + Sync>;

/// A change of one component of a template: set it whole, remove it, or edit
/// it.
///
/// The component is named by enshwick's id for its type, which the loader
/// maps to the running build's: a generated type's with the `typed` feature,
/// such as `ecs::Faction`.
///
/// A set adds a component the template lacks only when templates of the
/// same family carry its type somewhere in the vanilla data; the loader
/// refuses any other.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::templates::ComponentChange;
///
/// let change = ComponentChange::remove_named("ecs::Aggro");
/// assert_eq!(change.component(), "ecs::Aggro");
/// ```
#[derive(Clone)]
pub struct ComponentChange {
    component: &'static str,
    /// A value of the component's type on the running build must pass it, for
    /// a change made from a generated type.
    #[cfg(feature = "typed")]
    check: Option<Check>,
    op: Op,
}

/// What a [`ComponentChange`] does with its component.
#[derive(Clone)]
enum Op {
    Set(Value),
    Remove,
    Edit(ValueFn),
    Bytes(BytesFn),
}

impl core::fmt::Debug for ComponentChange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let op = match &self.op {
            Op::Set(_) => "set",
            Op::Remove => "remove",
            Op::Edit(_) | Op::Bytes(_) => "edit",
        };
        f.debug_struct("ComponentChange")
            .field("component", &self.component)
            .field("op", &op)
            .finish_non_exhaustive()
    }
}

impl ComponentChange {
    /// Removes the component of type `component`; a template without one
    /// refuses the change.
    ///
    /// # Examples
    ///
    /// See [`ComponentChange`].
    #[must_use]
    pub const fn remove_named(component: &'static str) -> Self {
        Self {
            component,
            #[cfg(feature = "typed")]
            check: None,
            op: Op::Remove,
        }
    }

    /// Sets the component of type `component` to `value`, by the game's
    /// field names, adding it when the template lacks one.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::templates::ComponentChange;
    /// use enshwick::resources::Value;
    ///
    /// let calm = ComponentChange::set_value("ecs::Aggro", Value::Struct(Vec::new()));
    /// assert_eq!(calm.component(), "ecs::Aggro");
    /// ```
    #[must_use]
    pub const fn set_value(component: &'static str, value: Value) -> Self {
        Self {
            component,
            #[cfg(feature = "typed")]
            check: None,
            op: Op::Set(value),
        }
    }

    /// Edits the component of type `component` as the codec's value, by the
    /// game's field names; fields the value leaves out keep their bytes. A
    /// template without one refuses the change.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::templates::ComponentChange;
    /// use enshwick::resources::Value;
    ///
    /// let vukah = ComponentChange::edit_value("ecs::Faction", |faction| {
    ///     if let Some(held) = faction.field_mut("faction") {
    ///         *held = Value::Name("Vukah".into());
    ///     }
    /// });
    /// assert_eq!(vukah.component(), "ecs::Faction");
    /// ```
    #[must_use]
    pub fn edit_value(
        component: &'static str,
        edit: impl Fn(&mut Value) + Send + Sync + 'static,
    ) -> Self {
        Self {
            component,
            #[cfg(feature = "typed")]
            check: None,
            op: Op::Edit(Arc::new(move |value| {
                let mut value = value.clone();
                edit(&mut value);
                Ok(value)
            })),
        }
    }

    /// Edits the bytes of the component of type `component`, laid out as the
    /// running build reflects it, with offsets from [`EngineFacts`]; `edit`
    /// returns `false` to refuse them. A template without one refuses the
    /// change.
    ///
    /// [`EngineFacts`]: super::EngineFacts
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::templates::ComponentChange;
    ///
    /// let first = ComponentChange::edit_bytes("ecs::Faction", |bytes| {
    ///     bytes.first_mut().map(|faction| *faction = 3).is_some()
    /// });
    /// assert_eq!(first.component(), "ecs::Faction");
    /// ```
    #[must_use]
    pub fn edit_bytes(
        component: &'static str,
        edit: impl Fn(&mut [u8]) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            component,
            #[cfg(feature = "typed")]
            check: None,
            op: Op::Bytes(Arc::new(edit)),
        }
    }

    /// A change a script gave: `component` by its generated id, set to a
    /// value or edited by `edit`, or neither to remove it.
    #[cfg(feature = "typed")]
    pub(crate) fn scripted(
        component: &'static str,
        check: Check,
        set: Option<Value>,
        edit: Option<ValueFn>,
    ) -> Self {
        let op = match (set, edit) {
            (Some(value), _) => Op::Set(value),
            (None, Some(edit)) => Op::Edit(edit),
            (None, None) => Op::Remove,
        };
        Self {
            component,
            check: Some(check),
            op,
        }
    }

    /// enshwick's id of the component type it changes, such as `ecs::Aggro`.
    ///
    /// # Examples
    ///
    /// See [`ComponentChange`].
    #[must_use]
    pub const fn component(&self) -> &'static str {
        self.component
    }
}

#[cfg(feature = "typed")]
impl ComponentChange {
    /// Sets the component of type `T` to `value`, adding it when the
    /// template lacks one.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::templates::ComponentChange;
    /// use enshwick::typed::ecs::Faction;
    ///
    /// fn same_faction(faction: &Faction) -> ComponentChange {
    ///     ComponentChange::set(faction)
    /// }
    /// ```
    #[must_use]
    pub fn set<T: crate::typed::Component>(value: &T) -> Self {
        Self {
            component: T::ID,
            check: Some(Arc::new(crate::typed::check::<T>)),
            op: Op::Set(value.to_value()),
        }
    }

    /// Removes the component of type `T`; a template without one refuses
    /// the change.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::templates::ComponentChange;
    /// use enshwick::typed::ecs::Aggro;
    ///
    /// assert_eq!(ComponentChange::remove::<Aggro>().component(), "ecs::Aggro");
    /// ```
    #[must_use]
    pub fn remove<T: crate::typed::Component>() -> Self {
        Self {
            check: Some(Arc::new(crate::typed::check::<T>)),
            ..Self::remove_named(T::ID)
        }
    }

    /// Changes the template's component of type `T` with `edit`, keeping
    /// the bytes of what `T` does not hold; a template without one refuses
    /// the change.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::templates::ComponentChange;
    /// use enshwick::typed::FactionType;
    /// use enshwick::typed::ecs::Faction;
    ///
    /// let vukah = ComponentChange::edit(|faction: &mut Faction| faction.faction = FactionType::Vukah);
    /// assert_eq!(vukah.component(), "ecs::Faction");
    /// ```
    #[must_use]
    pub fn edit<T: crate::typed::Component>(edit: impl Fn(&mut T) + Send + Sync + 'static) -> Self {
        Self {
            component: T::ID,
            check: Some(Arc::new(crate::typed::check::<T>)),
            op: Op::Edit(Arc::new(move |value| {
                let mut typed = T::from_value(value)?;
                edit(&mut typed);
                Ok(typed.to_value())
            })),
        }
    }
}

/// A new entity template for [`Load::add_template`]: a copy of the vanilla
/// template `like` under the mod's content id `id`, with `changes` applied
/// in order.
///
/// `id` is the mod's own and never changes: saves keep the template GUID of
/// every placed object and creature made from it.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::{Guid, id};
/// use enshwick::content::templates::NewTemplate;
///
/// let copy = NewTemplate::new(id!("crate"), Guid::parse("ebf450c3-fb96-4240-a3be-8ebe0713eccd"));
/// assert!(copy.changes.is_empty());
/// ```
#[derive(Clone, Debug, crate::Script)]
pub struct NewTemplate {
    /// The new template's content id.
    pub id: Id,
    /// The vanilla template copied.
    pub like: Guid,
    /// The changes of the copy's components, in order.
    pub changes: Vec<ComponentChange>,
}

impl NewTemplate {
    /// A copy of `like` under `id`, unchanged.
    ///
    /// # Examples
    ///
    /// See [`NewTemplate`].
    #[must_use]
    pub const fn new(id: Id, like: Guid) -> Self {
        Self {
            id,
            like,
            changes: Vec::new(),
        }
    }
}

/// A [`ComponentChange`] resolved on the running build: its component's
/// qualified name, as the codec names it.
#[derive(Clone)]
struct Resolved {
    name: &'static str,
    op: Op,
}

/// `template`'s value with `changes` applied in order; `registry` lays out a
/// component a byte edit changes.
///
/// # Errors
///
/// [`Off`] when the value is not a template, a remove or an edit finds no
/// component of its type, or an edit refuses the component.
fn apply(registry: &Registry<'_>, mut template: Value, changes: &[Resolved]) -> Result<Value, Off> {
    let Some(Value::List(components)) = template.field_mut(COMPONENTS) else {
        return Err(Off::new("template: no components"));
    };
    for change in changes {
        let at = components.iter().position(
            |component| matches!(component, Value::Variant(held) if held.type_name() == change.name),
        );
        let held = at.and_then(|at| match components.get(at) {
            Some(Value::Variant(held)) => Some(held.value().clone()),
            _ => None,
        });
        match (&change.op, at) {
            (Op::Set(value), Some(at)) => {
                components[at] = Value::Variant(Box::new(Variant::new(change.name, value.clone())));
            }
            (Op::Set(value), None) => components.push(Value::Variant(Box::new(Variant::new(
                change.name,
                value.clone(),
            )))),
            (Op::Remove, Some(at)) => {
                components.remove(at);
            }
            (Op::Edit(edit), Some(at)) => {
                let value = held.ok_or_else(|| Off::new("template: not a component"))?;
                let edited = edit(&value)?;
                components[at] = Value::Variant(Box::new(Variant::new(change.name, edited)));
            }
            (Op::Bytes(edit), Some(at)) => {
                let value = held.ok_or_else(|| Off::new("template: not a component"))?;
                let mut bytes = registry.encode(change.name, &value, None)?;
                if !edit(&mut bytes) {
                    return Err(Off::new(format!(
                        "template: the edit of {} refused it",
                        change.name
                    )));
                }
                let edited = registry.decode(change.name, &bytes)?;
                components[at] = Value::Variant(Box::new(Variant::new(change.name, edited)));
            }
            (Op::Remove | Op::Edit(_) | Op::Bytes(_), None) => {
                return Err(Off::new(format!(
                    "template: no component {} to change",
                    change.name
                )));
            }
        }
    }
    Ok(template)
}

/// The field of a template that lists its components, the one that names
/// its family, and the collection's list of templates: the game's own names,
/// which the generated items hold too.
const COMPONENTS: &str = "components";
const FAMILY: &str = "family";
const TEMPLATES: &str = "templates";

/// A `BlobVariant` in a blob: type hash, offset, size.
const VARIANT: usize = 12;

/// The component type hashes the vanilla templates of every family carry,
/// read raw from the archive, so thousands of templates read in little time.
///
/// shortcut: read again by each call that adds a component; keep it in a
/// content state struct if mods come to add many.
fn families(
    host: crate::Host,
    registry: &Registry<'static>,
    template: &Type<'static>,
) -> Result<FxHashMap<String, FxHashSet<u32>>, Off> {
    let fields = registry.all_fields(template);
    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field.name() == name)
            .ok_or_else(|| Off::new(format!("templates: the template has no field {name}")))
    };
    let offset = |name: &str| {
        usize::try_from(field(name)?.offset())
            .map_err(|_| Off::new("templates: an offset past usize"))
    };
    let (family_at, components_at) = (offset(FAMILY)?, offset(COMPONENTS)?);
    let family_ty = field(FAMILY)?
        .type_name()
        .and_then(|name| registry.named(name))
        .ok_or_else(|| Off::new("templates: the family is not reflected"))?;
    let mut families: FxHashMap<String, FxHashSet<u32>> = FxHashMap::default();
    for key in host.resource_keys(Some(template))? {
        let Some(bytes) = host.game_resource(key) else {
            continue;
        };
        let (Some(&raw), Some((start, count))) =
            (bytes.get(family_at), Blob::new(&bytes).array(components_at))
        else {
            continue;
        };
        let blob = Blob::new(&bytes);
        let family = family_ty
            .constants()
            .iter()
            .find(|constant| constant.value() == u64::from(raw))
            .map_or_else(|| raw.to_string(), |constant| constant.name().to_owned());
        let carried = families.entry(family).or_default();
        for i in 0..usize::try_from(count).unwrap_or(0) {
            if let Some(hash) = blob.read_u32(start + i * VARIANT) {
                carried.insert(hash);
            }
        }
    }
    Ok(families)
}

/// The family a template value names.
fn family_of(template: &Value) -> String {
    match template.field(FAMILY) {
        Some(Value::Name(name)) => name.clone(),
        Some(Value::Uint(raw)) => raw.to_string(),
        _ => String::new(),
    }
}

/// The qualified names of a template value's components.
pub(super) fn components_of(template: &Value) -> Vec<String> {
    match template.field(COMPONENTS) {
        Some(Value::List(components)) => components
            .iter()
            .filter_map(|component| match component {
                Value::Variant(held) => Some(held.type_name().to_owned()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The running build's template type.
fn template_type() -> Result<&'static Type<'static>, Off> {
    let hash = types::TEMPLATE_RESOURCE.hash()?;
    Registry::game()?
        .hashed(hash)
        .ok_or_else(|| Off::new("templates: the type registry lacks the template"))
}

/// The running build's type enshwick's id `id` names.
fn reflected(
    load: &Load<'_>,
    registry: &'static Registry<'static>,
    id: &str,
) -> Result<&'static Type<'static>, Off> {
    let hash = crate::ecs::ComponentType::resolve(load, id)?.hash();
    registry
        .hashed(hash)
        .ok_or_else(|| Off::new(format!("type {id}: the type registry lacks it")))
}

impl Load<'_> {
    /// `changes` resolved on the running build and checked against the
    /// vanilla template `vanilla`, which `what` names in a refusal.
    fn resolve_changes(
        &self,
        template: &'static Type<'static>,
        vanilla: Guid,
        changes: Vec<ComponentChange>,
        what: &str,
    ) -> Result<Vec<Resolved>, Off> {
        let host = self.host();
        let registry = Registry::game()?;
        let value = host
            .game_value(vanilla.key_by_hash(template.qualified_hash()))
            .map_err(|why| Off::new(format!("{what}: {vanilla} is no vanilla template: {why}")))?;
        let mut present = components_of(&value);
        let mut carried = None;
        let mut resolved = Vec::with_capacity(changes.len());
        for change in changes {
            let ty = reflected(self, registry, change.component)
                .map_err(|why| Off::new(format!("{what}: {why}")))?;
            let name = ty.qualified_name();
            #[cfg(feature = "typed")]
            if let Some(check) = &change.check {
                crate::typed::sample(registry, name)
                    .ok_or_else(|| Off::new(format!("{what}: {name} does not read")))
                    .and_then(|sample| check(&sample).map_err(Off::from))
                    .map_err(|why| Off::new(format!("{what}: {}: {why}", change.component)))?;
            }
            match (present.iter().position(|held| held == name), &change.op) {
                (Some(at), Op::Remove) => {
                    present.remove(at);
                }
                (Some(_), _) => {}
                (None, Op::Set(_)) => {
                    let family = family_of(&value);
                    if carried.is_none() {
                        carried = Some(families(host, registry, template)?);
                    }
                    let in_family = carried
                        .as_ref()
                        .and_then(|families| families.get(&family))
                        .is_some_and(|hashes| hashes.contains(&ty.qualified_hash()));
                    if !in_family {
                        return Err(Off::new(format!(
                            "{what}: no vanilla template of family {family} carries {}; \
                             a template takes only components its family has",
                            change.component
                        )));
                    }
                    present.push(name.to_owned());
                }
                (None, _) => {
                    return Err(Off::new(format!(
                        "{what}: {vanilla} has no component {} to change",
                        change.component
                    )));
                }
            }
            resolved.push(Resolved {
                name,
                op: change.op,
            });
        }
        Ok(resolved)
    }

    /// Serves `key`, built from the template `base` with `changes`.
    fn serve_template(
        &self,
        template: &'static Type<'static>,
        key: Guid,
        base: Guid,
        changes: Vec<Resolved>,
    ) -> Result<(), Off> {
        let registry = Registry::game()?;
        let hash = template.qualified_hash();
        crate::resources::serve_value(
            self.host(),
            key.key_by_hash(hash),
            base.key_by_hash(hash),
            move |value| apply(registry, value, &changes),
        )
    }

    /// Lists `template` in the game's template collection after the others.
    fn list_template(&self, template: Guid) -> Result<(), Off> {
        let key = types::TEMPLATE_COLLECTION_RESOURCE.key(TEMPLATE_COLLECTION)?;
        crate::resources::serve_value(self.host(), key, key, move |mut value| {
            match value.field_mut(TEMPLATES) {
                Some(Value::List(templates)) => {
                    templates.push(Value::Guid(template));
                    Ok(value)
                }
                _ => Err(Off::new("template collection: no list of templates")),
            }
        })
    }
}

#[crate::script(content_templates)]
impl Load<'_> {
    /// Adds an entity template: a copy of the vanilla template
    /// [`NewTemplate::like`] under the mod's content id, its family, model
    /// and proxy model kept, with the changes applied in order, and lists
    /// it in the game's template collection. On the server and the client
    /// alike, so a mod that adds one is a both mod.
    ///
    /// When the mod is removed, the loader serves the vanilla template under
    /// the copy's GUID, so saved objects and creatures load as the vanilla
    /// one.
    ///
    /// # Errors
    ///
    /// [`Off`] when `like` is no vanilla template, a change's component type
    /// is not reflected or does not fit (see [`ComponentChange`]), or the
    /// loader refuses the id (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::templates::{ComponentChange, NewTemplate};
    ///
    /// const WOLF: Guid = Guid::parse("b7a4f1c0-4c1e-4f39-9d5b-2a0f3c6d8e11");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let calm = load.add_template(NewTemplate {
    ///         changes: vec![ComponentChange::remove_named("ecs::Aggro")],
    ///         ..NewTemplate::new(id!("calm-wolf"), WOLF)
    ///     })?;
    ///     enshwick::info!("calm wolves are {}", calm.guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_template(&mut self, new: NewTemplate) -> Result<TemplateId, Off> {
        self.declare(new.id)?;
        let template = template_type()?;
        let what = format!("template {}", new.id);
        let changes = self.resolve_changes(template, new.like, new.changes, &what)?;
        let guid = new.id.guid();
        self.serve_template(template, guid, new.like, changes)?;
        self.list_template(guid)?;
        Ok(TemplateId::new(guid))
    }

    /// Changes the components of the vanilla template `template` in memory,
    /// the changes applied in order; mods that change one template change it
    /// in load order, each on the one before's result. Nothing is written to
    /// a save or the archive.
    ///
    /// # Errors
    ///
    /// [`Off`] when `template` is no vanilla template, or a change's
    /// component type is not reflected or does not fit (see
    /// [`ComponentChange`]).
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::Guid;
    /// use enshwick::content::templates::ComponentChange;
    ///
    /// const WOLF: Guid = Guid::parse("b7a4f1c0-4c1e-4f39-9d5b-2a0f3c6d8e11");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.change_template(WOLF, vec![ComponentChange::remove_named("ecs::Aggro")])
    /// }
    /// ```
    #[script]
    pub fn change_template(
        &mut self,
        template: Guid,
        changes: Vec<ComponentChange>,
    ) -> Result<(), Off> {
        let ty = template_type()?;
        let what = format!("template {template}");
        let changes = self.resolve_changes(ty, template, changes, &what)?;
        self.serve_template(ty, template, template, changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{Field, Kind};

    const HEALTH: &str = "demo::Health";
    const AGGRO: &str = "demo::Aggro";

    fn registry() -> Registry<'static> {
        Registry::new([
            Type::new("demo::sint32", Kind::Sint32, 4, 4),
            Type::new(HEALTH, Kind::Struct, 4, 4).with_fields(vec![Field::new(
                "max",
                0,
                "demo::sint32",
            )]),
            Type::new(AGGRO, Kind::Struct, 1, 1),
        ])
    }

    fn template() -> Value {
        let component =
            |name: &str, value: Value| Value::Variant(Box::new(Variant::new(name, value)));
        Value::Struct(vec![
            (FAMILY.into(), Value::Name("Npc".into())),
            (
                COMPONENTS.into(),
                Value::List(vec![
                    component(HEALTH, Value::Struct(vec![("max".into(), Value::Int(40))])),
                    component(AGGRO, Value::Struct(Vec::new())),
                ]),
            ),
        ])
    }

    fn resolved(name: &'static str, op: Op) -> Resolved {
        Resolved { name, op }
    }

    fn doubled() -> Op {
        Op::Edit(Arc::new(|value| {
            let mut value = value.clone();
            if let Some(Value::Int(max)) = value.field_mut("max") {
                *max *= 2;
            }
            Ok(value)
        }))
    }

    fn max(template: &Value) -> Option<&Value> {
        let Some(Value::List(components)) = template.field(COMPONENTS) else {
            return None;
        };
        let Some(Value::Variant(health)) = components.first() else {
            return None;
        };
        health.value().field("max")
    }

    #[test]
    fn changes_apply_in_order_to_the_named_components_only() {
        let changes = [
            resolved(HEALTH, doubled()),
            resolved(HEALTH, doubled()),
            resolved(AGGRO, Op::Remove),
        ];
        let after = apply(&registry(), template(), &changes).unwrap();
        assert_eq!(components_of(&after), [HEALTH]);
        assert_eq!(max(&after), Some(&Value::Int(160)));
        assert_eq!(family_of(&after), "Npc");
    }

    #[test]
    fn a_byte_edit_changes_the_component_as_the_build_lays_it_out() {
        let tripled = Op::Bytes(Arc::new(|bytes| {
            let Some(max) = bytes.get_mut(..4) else {
                return false;
            };
            max.copy_from_slice(&120i32.to_le_bytes());
            true
        }));
        let after = apply(&registry(), template(), &[resolved(HEALTH, tripled)]).unwrap();
        assert_eq!(max(&after), Some(&Value::Int(120)));
        let refused = Op::Bytes(Arc::new(|_| false));
        assert!(apply(&registry(), template(), &[resolved(HEALTH, refused)]).is_err());
    }

    #[test]
    fn a_set_replaces_a_component_or_adds_it_after_the_others() {
        let max = |value: i64| Op::Set(Value::Struct(vec![("max".into(), Value::Int(value))]));
        let changes = [
            resolved(AGGRO, Op::Remove),
            resolved(HEALTH, max(7)),
            resolved(AGGRO, Op::Set(Value::Struct(Vec::new()))),
        ];
        let after = apply(&registry(), template(), &changes).unwrap();
        assert_eq!(components_of(&after), [HEALTH, AGGRO]);
        assert_eq!(self::max(&after), Some(&Value::Int(7)));
    }

    #[test]
    fn a_remove_or_edit_of_a_missing_component_is_refused() {
        let twice = [resolved(AGGRO, Op::Remove), resolved(AGGRO, Op::Remove)];
        let gone = apply(&registry(), template(), &twice).unwrap_err();
        assert!(gone.to_string().contains(AGGRO), "{gone}");
        assert!(
            apply(
                &registry(),
                template(),
                &[resolved("demo::Boss", doubled())]
            )
            .is_err()
        );
        assert!(
            apply(&registry(), Value::Struct(Vec::new()), &[]).is_err(),
            "no components"
        );
    }

    #[test]
    fn a_change_shows_its_component_and_operation() {
        let change = ComponentChange::remove_named("ecs::Aggro");
        assert_eq!(
            format!("{change:?}"),
            r#"ComponentChange { component: "ecs::Aggro", op: "remove", .. }"#
        );
    }
}
