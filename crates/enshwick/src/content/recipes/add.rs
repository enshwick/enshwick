//! New recipes: a vanilla recipe copied for a new output, placed in its
//! workshops within their limit.

use crate::{Load, Off};

use super::super::workshops::WorkshopId;
use super::super::{Blob, Guid, Id};
use super::derive::{makes, scaled_inputs};

/// The most recipes the game itself takes at one workshop; more ends the game
/// with "`SmallDynamicArray` is out of elements" unless the loader lifts the
/// limit (see [`workshop_limit`]).
pub const WORKSHOP_RECIPES: usize = 512;

/// Places at each workshop derived recipes leave free for single ones,
/// which [`Load::add_recipe`] adds, while the game's limit holds.
pub const RESERVED_FOR_SINGLE: usize = 16;

/// The most recipes one workshop takes on the running game: [`usize::MAX`]
/// once the loader has lifted the game's limit, else [`WORKSHOP_RECIPES`],
/// also outside the game. The answer is final once
/// the hooks are armed, before the game first reads the recipe registry.
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::{WORKSHOP_RECIPES, workshop_limit};
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
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let layout = enshwick::content::recipes::RecipeLayout::resolve(load)?;
    ///     assert_eq!(layout.recipe, 0xa8);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        Self::with_lookup(
            |ty, name| {
                load.field_offset(ty, name)
                    .map(|o| usize::try_from(o).unwrap_or(usize::MAX))
            },
            |ty| load.type_size(ty).map(|s| s as usize),
        )
    }

    /// The layout of a game whose reflection `field` (a type's field offset)
    /// and `size` (a type's size) read, for the loader and for tools that
    /// read a game image rather than run in the game.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a lookup returns.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Off;
    /// use enshwick::content::recipes::RecipeLayout;
    ///
    /// let none = RecipeLayout::with_lookup(|ty, f| Err(Off::new(format!("{ty}.{f}"))), |_| Ok(0));
    /// assert!(none.is_err());
    /// ```
    pub fn with_lookup(
        field: impl Fn(&str, &str) -> Result<usize, Off>,
        size: impl Fn(&str) -> Result<usize, Off>,
    ) -> Result<Self, Off> {
        let recipe = |name: &str| field("keen::RecipeInfo", name);
        let stack = |name: &str| field("keen::RecipeItemStackResource", name);
        Ok(Self {
            recipes: field("keen::RecipeRegistryResource", "recipes")?,
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
pub(super) fn fnv1a(bytes: &[u8]) -> u32 {
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
/// use enshwick::content::recipes::{
///     NewRecipe, RECIPE_REGISTRY, RecipeLayout, add_recipe_to, workshop_limit,
/// };
/// use enshwick::content::types;
///
/// fn init(
///     load: &mut enshwick::Load<'_>,
///     recipe: NewRecipe,
/// ) -> Result<(), enshwick::Off> {
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
pub(super) fn retarget(
    layout: &RecipeLayout,
    blob: &mut Blob,
    at: usize,
    workshop: WorkshopId,
) -> bool {
    blob.write(at + layout.workshop_guid, &workshop.guid().0)
        && blob.write(at + layout.workshop, &workshop.id().to_le_bytes())
}

/// How many recipes each workshop has.
pub(super) fn workshop_counts(
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
