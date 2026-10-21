//! Recipes: a vanilla recipe copied for a new output, with its ingredients
//! scaled, or the recipes in the registry changed in place.

use super::workshops::WorkshopId;
use super::{Blob, Guid, Id, types};
use crate::{Load, Off};

/// The most recipes the game itself takes at one workshop; more ends the game
/// with "`SmallDynamicArray` is out of elements" unless the loader lifts the
/// limit (see [`workshop_limit`]).
pub const WORKSHOP_RECIPES: usize = 512;

/// Places at each workshop derived recipes leave free for single ones,
/// which [`Load::add_recipe`] adds, while the game's limit holds.
pub const RESERVED_FOR_SINGLE: usize = 16;

/// The most recipes one workshop takes on the running game: [`usize::MAX`]
/// once the loader has lifted the game's limit, else [`WORKSHOP_RECIPES`],
/// also outside the game and with an older loader. The answer is final once
/// the hooks are armed, before the game first reads the recipe registry.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::recipes::{WORKSHOP_RECIPES, workshop_limit};
///
/// // Outside the game the limit holds.
/// assert_eq!(workshop_limit(), WORKSHOP_RECIPES);
/// ```
#[must_use]
pub fn workshop_limit() -> usize {
    let Some(host) = crate::host() else {
        return WORKSHOP_RECIPES;
    };
    let api = host.api();
    let end = core::mem::offset_of!(crate::abi::HostApi, workshop_recipes) + size_of::<usize>();
    if !api.header.covers(end) {
        return WORKSHOP_RECIPES;
    }
    match (api.workshop_recipes)() {
        u32::MAX => usize::MAX,
        limit => usize::try_from(limit).unwrap_or(WORKSHOP_RECIPES),
    }
}

/// The game's recipe registry resource.
pub const RECIPE_REGISTRY: Guid = Guid::parse("5e57de0f-280c-4559-8191-f02c1ba656e2");

/// A new recipe for [`Load::add_recipe`].
///
/// # Examples
///
/// See [`Load::add_recipe`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewRecipe {
    /// The new recipe's id; its game id is [`Id::id`].
    pub id: Id,
    /// The item whose recipe is copied: same workshop, unlock and duration.
    pub like: Guid,
    /// The item the recipe makes.
    pub output: Guid,
    /// How many it makes.
    pub output_count: u32,
    /// The copied recipe's ingredient counts times this.
    pub cost: u32,
    /// The workshop the recipe is made at, such as one from
    /// [`Load::add_workshop`]; `None` keeps the copied recipe's.
    pub workshop: Option<WorkshopId>,
}

/// Where the fields a recipe edit touches sit, from the game's reflection.
///
/// # Examples
///
/// See [`RecipeLayout::resolve`] and [`add_recipe_to`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeLayout {
    /// `RecipeRegistryResource.recipes`.
    pub recipes: usize,
    /// `RecipeInfo` size.
    pub recipe: usize,
    /// `RecipeInfo.recipeGuid`.
    pub guid: usize,
    /// `RecipeInfo.recipeId`.
    pub id: usize,
    /// `RecipeInfo.workshopGuid`.
    pub workshop_guid: usize,
    /// `RecipeInfo.workshopId`.
    pub workshop: usize,
    /// `RecipeInfo.input`.
    pub input: usize,
    /// `RecipeInfo.output`.
    pub output: usize,
    /// `RecipeInfo.completionRequirementQuery`, whose flag id comes first.
    pub completion: usize,
    /// The array and string headers inside a `RecipeInfo`.
    pub relative: [usize; 4],
    /// `RecipeInputResource` size.
    pub input_size: usize,
    /// `RecipeItemStackResource` size.
    pub stack_size: usize,
    /// `RecipeItemStackResource.item`.
    pub stack_item: usize,
    /// `RecipeItemStackResource.count`.
    pub stack_count: usize,
}

impl RecipeLayout {
    /// The layout on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when a field or type is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let layout = enshwick_loader_api::content::recipes::RecipeLayout::resolve(load)?;
    ///     assert_eq!(layout.recipe, 0xa8);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let wide = |v: u64| usize::try_from(v).unwrap_or(usize::MAX);
        let recipe = |name: &str| load.field_offset("keen::RecipeInfo", name).map(wide);
        let stack = |name: &str| {
            load.field_offset("keen::RecipeItemStackResource", name)
                .map(wide)
        };
        let size = |ty: &str| load.type_size(ty).map(|s| s as usize);
        Ok(Self {
            recipes: load
                .field_offset("keen::RecipeRegistryResource", "recipes")
                .map(wide)?,
            recipe: size("keen::RecipeInfo")?,
            guid: recipe("recipeGuid")?,
            id: recipe("recipeId")?,
            workshop_guid: recipe("workshopGuid")?,
            workshop: recipe("workshopId")?,
            input: recipe("input")?,
            output: recipe("output")?,
            completion: recipe("completionRequirementQuery")?,
            relative: [
                recipe("input")?,
                recipe("output")?,
                recipe("requiredProps")?,
                recipe("debugName")?,
            ],
            input_size: size("keen::RecipeInputResource")?,
            stack_size: size("keen::RecipeItemStackResource")?,
            stack_item: stack("item")?,
            stack_count: stack("count")?,
        })
    }
}

/// FNV-1a over `bytes`.
fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5_u32, |hash, &b| {
        (hash ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// The recipe registry with `recipe` after the others, unless its workshop
/// already holds `limit` recipes. [`Load::add_recipe`] serves it with
/// [`workshop_limit`]; call it from a [`Load::serve`] builder of your own to
/// combine it with other edits.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::recipes::{
///     NewRecipe, RECIPE_REGISTRY, RecipeLayout, add_recipe_to, workshop_limit,
/// };
/// use enshwick_loader_api::content::types;
///
/// fn init(
///     load: &mut enshwick_loader_api::Load<'_>,
///     recipe: NewRecipe,
/// ) -> Result<(), enshwick_loader_api::Off> {
///     let layout = RecipeLayout::resolve(load)?;
///     let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY);
///     load.serve(registry, registry, move |base| {
///         add_recipe_to(&layout, base, &recipe, workshop_limit())
///     })
/// }
/// ```
#[must_use]
pub fn add_recipe_to(
    layout: &RecipeLayout,
    registry: &[u8],
    recipe: &NewRecipe,
    limit: usize,
) -> Option<Vec<u8>> {
    let mut registry = Blob::new(registry);
    let (start, count) = registry.array(layout.recipes)?;
    let like = (0..usize::try_from(count).ok()?)
        .map(|i| start + i * layout.recipe)
        .find(|&at| makes(layout, &registry, at, recipe.like))?;
    let workshop = match recipe.workshop {
        Some(target) => target.id(),
        None => registry.read_u32(like + layout.workshop)?,
    };
    if workshop_counts(layout, &registry)?
        .get(&workshop)
        .copied()
        .unwrap_or(0)
        >= limit
    {
        crate::warn!("recipe skipped: workshop {workshop} holds {limit} recipes, the game's most");
        return Some(registry.into_bytes());
    }
    let template = registry.bytes().get(like..like + layout.recipe)?.to_vec();
    let inputs = scaled_inputs(layout, &registry, like, recipe.cost)?;
    let index = registry.grow_array(
        layout.recipes,
        layout.recipe,
        8,
        &layout.relative,
        &[(template, Some(like))],
    )?;
    let (start, _) = registry.array(layout.recipes)?;
    let at = start + usize::try_from(index).ok()? * layout.recipe;
    let id = recipe.id.id().to_le_bytes();
    let mut output = vec![0u8; layout.stack_size];
    output.get_mut(..16)?.copy_from_slice(&recipe.output.0);
    output
        .get_mut(layout.stack_item..layout.stack_item + 4)?
        .copy_from_slice(&recipe.output.id().to_le_bytes());
    output
        .get_mut(layout.stack_count..layout.stack_count + 4)?
        .copy_from_slice(&recipe.output_count.to_le_bytes());
    let input_count = u32::try_from(inputs.len() / layout.input_size.max(1)).ok()?;
    let inputs_at = registry.append(&inputs, 8);
    let output_at = registry.append(&output, 8);
    let written = registry.write(at + layout.guid, &recipe.id.guid().0)
        && registry.write(at + layout.id, &id)
        // The recipe's completion flag is its own id.
        && registry.write(at + layout.completion, &id)
        && registry.set_array(at + layout.input, inputs_at, input_count)
        && registry.set_array(at + layout.output, output_at, 1)
        && recipe
            .workshop
            .is_none_or(|target| retarget(layout, &mut registry, at, target));
    written.then(|| registry.into_bytes())
}

/// Moves the recipe at `at` to `workshop`.
fn retarget(layout: &RecipeLayout, blob: &mut Blob, at: usize, workshop: WorkshopId) -> bool {
    blob.write(at + layout.workshop_guid, &workshop.guid().0)
        && blob.write(at + layout.workshop, &workshop.id().to_le_bytes())
}

/// How many recipes each workshop has.
fn workshop_counts(
    layout: &RecipeLayout,
    blob: &Blob,
) -> Option<std::collections::HashMap<u32, usize>> {
    let (start, count) = blob.array(layout.recipes)?;
    let mut counts = std::collections::HashMap::new();
    for i in 0..usize::try_from(count).ok()? {
        let workshop = blob.read_u32(start + i * layout.recipe + layout.workshop)?;
        *counts.entry(workshop).or_insert(0) += 1;
    }
    Some(counts)
}

/// Whether the recipe at `at` makes `item`.
fn makes(layout: &RecipeLayout, registry: &Blob, at: usize, item: Guid) -> bool {
    let Some((outputs, count)) = registry.array(at + layout.output) else {
        return false;
    };
    (0..count as usize).any(|i| {
        let stack = outputs + i * layout.stack_size;
        registry.bytes().get(stack..stack + 16) == Some(&item.0[..])
    })
}

/// The recipe's inputs with every count times `cost`; an input's item stack
/// comes first in it.
fn scaled_inputs(layout: &RecipeLayout, registry: &Blob, at: usize, cost: u32) -> Option<Vec<u8>> {
    let (inputs, count) = registry.array(at + layout.input)?;
    let mut bytes = registry
        .bytes()
        .get(inputs..inputs + usize::try_from(count).ok()? * layout.input_size)?
        .to_vec();
    for input in bytes.chunks_mut(layout.input_size.max(1)) {
        let field = input.get_mut(layout.stack_count..layout.stack_count + 4)?;
        let count = u32::from_le_bytes(field.try_into().ok()?);
        field.copy_from_slice(&count.saturating_mul(cost).to_le_bytes());
    }
    Some(bytes)
}

/// A vanilla recipe as [`Load::derive_recipes`] shows it.
///
/// # Examples
///
/// See [`Load::derive_recipes`].
#[derive(Clone, Copy, Debug)]
pub struct RecipeView<'a> {
    blob: &'a Blob,
    at: usize,
    layout: &'a RecipeLayout,
}

impl RecipeView<'_> {
    /// The recipe's GUID.
    ///
    /// # Examples
    ///
    /// See [`Load::derive_recipes`].
    #[must_use]
    pub fn guid(&self) -> Guid {
        let mut guid = [0u8; 16];
        if let Some(bytes) = self
            .blob
            .bytes()
            .get(self.at + self.layout.guid..self.at + self.layout.guid + 16)
        {
            guid.copy_from_slice(bytes);
        }
        Guid(guid)
    }

    /// The recipe's debug name, such as `Block_T2_Metal_Copper_CRAFTING`.
    ///
    /// # Examples
    ///
    /// See [`Load::derive_recipes`].
    #[must_use]
    pub fn debug_name(&self) -> String {
        self.blob
            .array(self.at + self.layout.relative[3])
            .and_then(|(start, len)| self.blob.bytes().get(start..start + len as usize))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default()
    }

    /// The item the recipe makes and how many.
    ///
    /// # Examples
    ///
    /// See [`Load::derive_recipes`].
    #[must_use]
    pub fn output(&self) -> Option<(Guid, u32)> {
        let (outputs, count) = self.blob.array(self.at + self.layout.output)?;
        if count == 0 {
            return None;
        }
        let mut guid = [0u8; 16];
        guid.copy_from_slice(self.blob.bytes().get(outputs..outputs + 16)?);
        let made = self.blob.read_u32(outputs + self.layout.stack_count)?;
        Some((Guid(guid), made))
    }
}

/// How a derived recipe differs from the vanilla one it copies.
///
/// # Examples
///
/// See [`Load::derive_recipes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Derived {
    /// The vanilla output count times this.
    pub output: u32,
    /// The vanilla ingredient counts times this.
    pub cost: u32,
    /// The workshop the copy is made at, such as one from
    /// [`Load::add_workshop`]; `None` keeps the vanilla recipe's.
    pub workshop: Option<WorkshopId>,
}

/// A derived recipe's GUID: the vanilla one marked with the mod's salt, so it
/// stays the same across starts and differs between mods.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::content::recipes::derived_guid;
///
/// let vanilla = Guid::parse("5e57de0f-280c-4559-8191-f02c1ba656e2");
/// assert_eq!(derived_guid(vanilla, 7), derived_guid(vanilla, 7));
/// assert_ne!(derived_guid(vanilla, 7), derived_guid(vanilla, 8));
/// assert_ne!(derived_guid(vanilla, 7), vanilla);
/// ```
#[must_use]
pub fn derived_guid(base: Guid, salt: u32) -> Guid {
    let mut guid = base.0;
    for (byte, salt) in guid[..4].iter_mut().zip(salt.to_le_bytes()) {
        *byte ^= salt;
    }
    for (byte, salt) in guid[12..]
        .iter_mut()
        .zip(salt.rotate_left(16).to_le_bytes())
    {
        *byte ^= salt;
    }
    Guid(guid)
}

/// The recipe registry with a derived copy after the others for each vanilla
/// recipe `choose` picks. With a workshop `limit` other than [`usize::MAX`],
/// copies stop [`RESERVED_FOR_SINGLE`] short of it. [`Load::derive_recipes`]
/// serves it; `salt` marks the copies as in [`derived_guid`].
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::recipes::{
///     Derived, RECIPE_REGISTRY, RecipeLayout, derive_recipes_in, workshop_limit,
/// };
/// use enshwick_loader_api::content::types;
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let layout = RecipeLayout::resolve(load)?;
///     let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY);
///     load.serve(registry, registry, move |base| {
///         derive_recipes_in(&layout, base, 0x5afe, workshop_limit(), &|recipe| {
///             recipe.debug_name().starts_with("Block_").then_some(Derived {
///                 output: 2,
///                 cost: 1,
///                 workshop: None,
///             })
///         })
///     })
/// }
/// ```
#[must_use]
pub fn derive_recipes_in(
    layout: &RecipeLayout,
    registry: &[u8],
    salt: u32,
    limit: usize,
    choose: &dyn Fn(&RecipeView<'_>) -> Option<Derived>,
) -> Option<Vec<u8>> {
    let cap = if limit == usize::MAX {
        limit
    } else {
        limit.saturating_sub(RESERVED_FOR_SINGLE)
    };
    let mut blob = Blob::new(registry);
    let (start, count) = blob.array(layout.recipes)?;
    let mut counts = workshop_counts(layout, &blob)?;
    let mut skipped = 0usize;
    let mut picked: Vec<(usize, Derived, Guid)> = Vec::new();
    for at in (0..usize::try_from(count).ok()?).map(|i| start + i * layout.recipe) {
        let view = RecipeView {
            blob: &blob,
            at,
            layout,
        };
        let Some(derived) = choose(&view) else {
            continue;
        };
        let workshop = match derived.workshop {
            Some(target) => target.id(),
            None => blob.read_u32(at + layout.workshop)?,
        };
        let held = counts.entry(workshop).or_insert(0);
        if *held >= cap {
            skipped += 1;
            continue;
        }
        *held += 1;
        picked.push((at, derived, derived_guid(view.guid(), salt)));
    }
    if skipped > 0 {
        crate::warn!(
            "{skipped} derived recipes skipped: a workshop takes {limit} recipes at most, {RESERVED_FOR_SINGLE} kept for single ones; many recipes belong at a workshop or NPC of the mod's own"
        );
    }
    if picked.is_empty() {
        return Some(blob.into_bytes());
    }
    let extra: Vec<(Vec<u8>, Option<usize>)> = picked
        .iter()
        .map(|&(at, _, _)| Some((blob.bytes().get(at..at + layout.recipe)?.to_vec(), Some(at))))
        .collect::<Option<_>>()?;
    let mut changes = Vec::with_capacity(picked.len());
    for &(at, derived, _) in &picked {
        let inputs = scaled_inputs(layout, &blob, at, derived.cost)?;
        let (outputs, _) = blob.array(at + layout.output)?;
        let mut output = blob
            .bytes()
            .get(outputs..outputs + layout.stack_size)?
            .to_vec();
        let count_field = output.get_mut(layout.stack_count..layout.stack_count + 4)?;
        let made = u32::from_le_bytes(count_field.try_into().ok()?);
        count_field.copy_from_slice(&made.saturating_mul(derived.output).to_le_bytes());
        changes.push((inputs, output));
    }
    let first = blob.grow_array(layout.recipes, layout.recipe, 8, &layout.relative, &extra)?;
    let (start, _) = blob.array(layout.recipes)?;
    for (index, ((_, derived, guid), (inputs, output))) in picked.iter().zip(changes).enumerate() {
        let derived = *derived;
        let at = start + (usize::try_from(first).ok()? + index) * layout.recipe;
        let id = guid.id().to_le_bytes();
        let input_count = u32::try_from(inputs.len() / layout.input_size.max(1)).ok()?;
        let inputs_at = blob.append(&inputs, 8);
        let output_at = blob.append(&output, 8);
        let written = blob.write(at + layout.guid, &guid.0)
            && blob.write(at + layout.id, &id)
            && blob.write(at + layout.completion, &id)
            && blob.set_array(at + layout.input, inputs_at, input_count)
            && blob.set_array(at + layout.output, output_at, 1)
            && derived
                .workshop
                .is_none_or(|target| retarget(layout, &mut blob, at, target));
        if !written {
            return None;
        }
    }
    Some(blob.into_bytes())
}

/// A recipe in the registry as [`Load::modify_recipes`] shows it, with its
/// counts open to change in place.
///
/// # Examples
///
/// See [`Load::modify_recipes`].
#[derive(Debug)]
pub struct RecipeEdit<'a> {
    blob: &'a mut Blob,
    at: usize,
    layout: &'a RecipeLayout,
    changed: bool,
}

impl RecipeEdit<'_> {
    /// The recipe as it reads now: GUID, debug name and output.
    ///
    /// # Examples
    ///
    /// See [`Load::modify_recipes`].
    #[must_use]
    pub fn view(&self) -> RecipeView<'_> {
        RecipeView {
            blob: self.blob,
            at: self.at,
            layout: self.layout,
        }
    }

    /// Multiplies the count of each item the recipe makes by `by`.
    ///
    /// # Examples
    ///
    /// See [`Load::modify_recipes`].
    pub fn scale_output(&mut self, by: u32) {
        self.scale(self.layout.output, self.layout.stack_size, by);
    }

    /// Multiplies the count of each ingredient by `by`.
    ///
    /// # Examples
    ///
    /// See [`Load::modify_recipes`].
    pub fn scale_inputs(&mut self, by: u32) {
        self.scale(self.layout.input, self.layout.input_size, by);
    }

    /// Multiplies the stack counts of the array at `field`, whose elements
    /// are `size` bytes and start with an item stack.
    fn scale(&mut self, field: usize, size: usize, by: u32) {
        let Some((start, count)) = self.blob.array(self.at + field) else {
            return;
        };
        for i in 0..count as usize {
            let at = start + i * size + self.layout.stack_count;
            let Some(made) = self.blob.read_u32(at) else {
                return;
            };
            let scaled = made.saturating_mul(by);
            if scaled != made && self.blob.write(at, &scaled.to_le_bytes()) {
                self.changed = true;
            }
        }
    }
}

/// The recipe registry with `modify` called on each recipe in turn; the
/// second value is how many recipes it changed. [`Load::modify_recipes`]
/// serves it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::recipes::{RECIPE_REGISTRY, RecipeLayout, modify_recipes_in};
/// use enshwick_loader_api::content::types;
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     let layout = RecipeLayout::resolve(load)?;
///     let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY);
///     load.serve(registry, registry, move |base| {
///         let (built, changed) = modify_recipes_in(&layout, base, &|recipe| recipe.scale_output(2))?;
///         enshwick_loader_api::info!("{changed} recipes doubled");
///         Some(built)
///     })
/// }
/// ```
#[must_use]
pub fn modify_recipes_in(
    layout: &RecipeLayout,
    registry: &[u8],
    modify: &dyn Fn(&mut RecipeEdit<'_>),
) -> Option<(Vec<u8>, usize)> {
    let mut blob = Blob::new(registry);
    let (start, count) = blob.array(layout.recipes)?;
    let mut changed = 0;
    for at in (0..usize::try_from(count).ok()?).map(|i| start + i * layout.recipe) {
        let mut edit = RecipeEdit {
            blob: &mut blob,
            at,
            layout,
            changed: false,
        };
        modify(&mut edit);
        changed += usize::from(edit.changed);
    }
    Some((blob.into_bytes(), changed))
}

impl Load<'_> {
    /// Adds a recipe to the game, on the server and the client alike: a copy
    /// of the recipe that makes `like`, at the same workshop and unlocked with
    /// it, making `output` instead, beside recipes other mods add.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old, the recipe layout is not
    /// reflected or the loader refuses the recipe's id (see
    /// [`Load::declare`]); a `like` with no recipe turns the mod off when the
    /// game first reads the registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::{Guid, Id};
    /// use enshwick_loader_api::content::recipes::NewRecipe;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.add_recipe(NewRecipe {
    ///         id: Id::new("my-mod:safe.recipe"),
    ///         like: Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         output: Guid::named("my-mod:safe"),
    ///         output_count: 1,
    ///         cost: 2,
    ///         workshop: None,
    ///     })
    /// }
    /// ```
    pub fn add_recipe(&mut self, recipe: NewRecipe) -> Result<(), Off> {
        self.declare(recipe.id)?;
        let layout = RecipeLayout::resolve(self)?;
        let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY);
        self.serve(registry, registry, move |base| {
            add_recipe_to(&layout, base, &recipe, workshop_limit())
        })
    }

    /// Adds a derived copy of each vanilla recipe `choose` picks: same
    /// workshop and unlock, its output and cost scaled as [`Derived`] says,
    /// beside the original. `salt` keeps the copies' GUIDs apart from other
    /// mods'; use the mod id.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old or the recipe layout is not
    /// reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::recipes::Derived;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.derive_recipes("my-mod", |recipe| {
    ///         recipe
    ///             .debug_name()
    ///             .starts_with("Block_")
    ///             .then_some(Derived { output: 2, cost: 1, workshop: None })
    ///     })
    /// }
    /// ```
    pub fn derive_recipes(
        &mut self,
        salt: &str,
        choose: impl Fn(&RecipeView<'_>) -> Option<Derived> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let layout = RecipeLayout::resolve(self)?;
        let salt = fnv1a(salt.as_bytes());
        let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY);
        self.serve(registry, registry, move |base| {
            derive_recipes_in(&layout, base, salt, workshop_limit(), &choose)
        })
    }

    /// Changes recipes in the registry in place, on the server and the
    /// client alike, so the client's crafting menu shows the new counts:
    /// `modify` is called with each recipe and may scale its output and
    /// ingredient counts. It sees the registry as it reaches this patch,
    /// with the recipes and changes of mods that patched before, and the
    /// mods after see its changes. The mod's log says how many recipes
    /// changed.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old or the recipe layout is not
    /// reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::recipes::RecipeEdit;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.modify_recipes(|recipe: &mut RecipeEdit<'_>| {
    ///         if recipe.view().debug_name().starts_with("Block_") {
    ///             recipe.scale_output(2);
    ///         }
    ///     })
    /// }
    /// ```
    pub fn modify_recipes(
        &mut self,
        modify: impl Fn(&mut RecipeEdit<'_>) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let layout = RecipeLayout::resolve(self)?;
        let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY);
        self.serve(registry, registry, move |base| {
            let (built, changed) = modify_recipes_in(&layout, base, &modify)?;
            crate::info!("{changed} recipes changed");
            Some(built)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: RecipeLayout = RecipeLayout {
        recipes: 8,
        recipe: 0xa8,
        guid: 0,
        id: 0x10,
        workshop_guid: 0x14,
        workshop: 0x24,
        input: 0x28,
        output: 0x30,
        completion: 0x5c,
        relative: [0x28, 0x30, 0x40, 0x78],
        input_size: 48,
        stack_size: 24,
        stack_item: 0x10,
        stack_count: 0x14,
    };
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const RECIPE_ID: Id = Id::new("test:recipe");
    const RECIPE: Guid = RECIPE_ID.guid();

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn the_copy_costs_more_and_makes_the_new_output() {
        let mut blob = Blob::new(&[0u8; 16]);
        let recipe = blob.append(&[0; 0xa8], 8);
        let mut input = vec![0u8; 48];
        input[0x14..0x18].copy_from_slice(&5u32.to_le_bytes());
        let input_at = blob.append(&input, 8);
        let mut output = vec![0u8; 24];
        output[..16].copy_from_slice(&CHEST.0);
        let output_at = blob.append(&output, 8);
        let name_at = blob.append(b"Chest\0", 1);
        assert!(blob.set_array(recipe + 0x28, input_at, 1));
        assert!(blob.set_array(recipe + 0x30, output_at, 1));
        assert!(blob.set_array(recipe + 0x78, name_at, 5));
        assert!(blob.set_array(8, recipe, 1));

        let new = NewRecipe {
            id: RECIPE_ID,
            like: CHEST,
            output: SAFE,
            output_count: 1,
            cost: 2,
            workshop: None,
        };
        let built =
            Blob::new(&add_recipe_to(&LAYOUT, blob.bytes(), &new, WORKSHOP_RECIPES).unwrap());
        let (recipes, count) = built.array(8).unwrap();
        assert_eq!(count, 2);
        let copy = recipes + 0xa8;
        assert_eq!(&built.bytes()[copy..copy + 16], &RECIPE.0);
        assert_eq!(u32_at(built.bytes(), copy + 0x10), RECIPE.id());
        assert_eq!(u32_at(built.bytes(), copy + 0x5c), RECIPE.id());
        let (inputs, _) = built.array(copy + 0x28).unwrap();
        assert_eq!(u32_at(built.bytes(), inputs + 0x14), 10);
        let (outputs, _) = built.array(copy + 0x30).unwrap();
        assert_eq!(&built.bytes()[outputs..outputs + 16], &SAFE.0);
        assert_eq!(u32_at(built.bytes(), outputs + 0x10), SAFE.id());
        let (name, len) = built.array(copy + 0x78).unwrap();
        assert_eq!(&built.bytes()[name..name + len as usize], b"Chest");
        let (inputs, _) = built.array(recipes + 0x28).unwrap();
        assert_eq!(u32_at(built.bytes(), inputs + 0x14), 5);
    }

    #[test]
    fn derived_copies_double_the_output_of_the_chosen_recipes_only() {
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&[0; 0xa8 * 2], 8);
        for (i, (name, made)) in [(&b"Block_A "[..], 100u32), (&b"Prop_B "[..], 1)]
            .iter()
            .enumerate()
        {
            let at = first + i * 0xa8;
            let mut output = vec![0u8; 24];
            output[0x14..0x18].copy_from_slice(&made.to_le_bytes());
            let output_at = blob.append(&output, 8);
            let name_at = blob.append(name, 1);
            assert!(blob.set_array(at + 0x30, output_at, 1));
            assert!(blob.set_array(at + 0x78, name_at, u32::try_from(name.len() - 1).unwrap()));
        }
        assert!(blob.set_array(8, first, 2));
        let built = derive_recipes_in(&LAYOUT, blob.bytes(), 7, WORKSHOP_RECIPES, &|r| {
            r.debug_name().starts_with("Block_").then_some(Derived {
                output: 2,
                cost: 1,
                workshop: None,
            })
        })
        .unwrap();
        let built = Blob::new(&built);
        let (recipes, count) = built.array(8).unwrap();
        assert_eq!(count, 3);
        let copy = RecipeView {
            blob: &built,
            at: recipes + 2 * 0xa8,
            layout: &LAYOUT,
        };
        assert_eq!(copy.debug_name(), "Block_A");
        assert_eq!(copy.output().unwrap().1, 200);
        assert_eq!(copy.guid(), derived_guid(Guid([0; 16]), 7));
        let original = RecipeView {
            blob: &built,
            at: recipes,
            layout: &LAYOUT,
        };
        assert_eq!(original.output().unwrap().1, 100);
    }

    #[test]
    fn a_recipe_can_move_to_another_workshop() {
        let mut blob = Blob::new(&[0u8; 16]);
        let recipe = blob.append(&[0; 0xa8], 8);
        assert!(blob.write(recipe + 0x24, &7u32.to_le_bytes()));
        let mut output = vec![0u8; 24];
        output[..16].copy_from_slice(&CHEST.0);
        let output_at = blob.append(&output, 8);
        assert!(blob.set_array(recipe + 0x30, output_at, 1));
        assert!(blob.set_array(8, recipe, 1));
        let bench = WorkshopId::new(Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6"));
        let new = NewRecipe {
            id: RECIPE_ID,
            like: CHEST,
            output: SAFE,
            output_count: 1,
            cost: 1,
            workshop: Some(bench),
        };
        let built = add_recipe_to(&LAYOUT, blob.bytes(), &new, WORKSHOP_RECIPES).unwrap();
        let built = Blob::new(&built);
        let (recipes, _) = built.array(8).unwrap();
        let copy = recipes + 0xa8;
        assert_eq!(&built.bytes()[copy + 0x14..copy + 0x24], &bench.guid().0);
        assert_eq!(u32_at(built.bytes(), copy + 0x24), bench.id());
        assert_eq!(u32_at(built.bytes(), recipes + 0x24), 7);

        // Derived copies move too, and count against the new workshop.
        let derived = derive_recipes_in(&LAYOUT, blob.bytes(), 7, 1, &|_| {
            Some(Derived {
                output: 2,
                cost: 1,
                workshop: Some(bench),
            })
        })
        .unwrap();
        assert_eq!(
            Blob::new(&derived).array(8).unwrap().1,
            1,
            "limit 1, held 0"
        );
        let derived = derive_recipes_in(&LAYOUT, blob.bytes(), 7, RESERVED_FOR_SINGLE + 1, &|_| {
            Some(Derived {
                output: 2,
                cost: 1,
                workshop: Some(bench),
            })
        })
        .unwrap();
        let derived = Blob::new(&derived);
        let (recipes, count) = derived.array(8).unwrap();
        assert_eq!(count, 2);
        assert_eq!(u32_at(derived.bytes(), recipes + 0xa8 + 0x24), bench.id());
    }

    #[test]
    fn modified_recipes_change_in_place_and_stay_one_each() {
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&[0; 0xa8 * 2], 8);
        for (i, name) in [&b"Block_A"[..], &b"Prop_Decoration_Chest"[..]]
            .iter()
            .enumerate()
        {
            let at = first + i * 0xa8;
            let mut input = vec![0u8; 48 * 2];
            input[0x14..0x18].copy_from_slice(&3u32.to_le_bytes());
            input[48 + 0x14..48 + 0x18].copy_from_slice(&4u32.to_le_bytes());
            let input_at = blob.append(&input, 8);
            let mut output = vec![0u8; 24];
            output[0x14..0x18].copy_from_slice(&10u32.to_le_bytes());
            let output_at = blob.append(&output, 8);
            let name_at = blob.append(name, 1);
            assert!(blob.set_array(at + 0x28, input_at, 2));
            assert!(blob.set_array(at + 0x30, output_at, 1));
            assert!(blob.set_array(at + 0x78, name_at, u32::try_from(name.len()).unwrap()));
        }
        assert!(blob.set_array(8, first, 2));
        let (built, changed) = modify_recipes_in(&LAYOUT, blob.bytes(), &|recipe| {
            if recipe.view().debug_name().starts_with("Block_") {
                recipe.scale_output(3);
                recipe.scale_inputs(1);
            }
        })
        .unwrap();
        assert_eq!(changed, 1);
        assert_eq!(built.len(), blob.bytes().len(), "in place, nothing added");
        let built = Blob::new(&built);
        let (recipes, count) = built.array(8).unwrap();
        assert_eq!(count, 2);
        let view = |at| RecipeView {
            blob: &built,
            at,
            layout: &LAYOUT,
        };
        assert_eq!(view(recipes).output().unwrap().1, 30);
        assert_eq!(view(recipes + 0xa8).output().unwrap().1, 10);
        let (inputs, _) = built.array(recipes + 0x28).unwrap();
        assert_eq!(u32_at(built.bytes(), inputs + 0x14), 3);
        assert_eq!(u32_at(built.bytes(), inputs + 48 + 0x14), 4);

        // A second patch composes: it sees the first one's counts.
        let (again, _) =
            modify_recipes_in(&LAYOUT, built.bytes(), &|recipe| recipe.scale_inputs(2)).unwrap();
        let again = Blob::new(&again);
        let (inputs, _) = again.array(recipes + 0x28).unwrap();
        assert_eq!(u32_at(again.bytes(), inputs + 48 + 0x14), 8);
        assert_eq!(view(recipes).output().unwrap().1, 30);
    }

    #[test]
    fn an_item_without_a_recipe_builds_nothing() {
        let mut blob = Blob::new(&[0u8; 16]);
        assert!(blob.set_array(8, 16, 0));
        let new = NewRecipe {
            id: RECIPE_ID,
            like: CHEST,
            output: SAFE,
            output_count: 1,
            cost: 1,
            workshop: None,
        };
        assert!(add_recipe_to(&LAYOUT, blob.bytes(), &new, WORKSHOP_RECIPES).is_none());
    }
}
