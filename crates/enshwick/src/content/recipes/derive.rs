//! Recipes derived from the registry's own, and recipes changed in place.

use super::super::menu::MenuEntry;
use super::super::workshops::WorkshopId;
use super::super::{Blob, Guid, types};
use super::RecipeKind;
use super::add::{
    NewRecipe, RECIPE_REGISTRY, RESERVED_FOR_SINGLE, RecipeLayout, retarget, workshop_counts,
};

/// Whether the recipe at `at` makes `item`.
pub(super) fn makes(layout: &RecipeLayout, registry: &Blob, at: usize, item: Guid) -> bool {
    let Some((outputs, count)) = registry.array(at + layout.output) else {
        return false;
    };
    (0..count as usize).any(|i| {
        let stack = outputs + i * layout.stack_size;
        registry.bytes().get(stack..stack + 16) == Some(&item.0[..])
    })
}

/// The vanilla recipe registry from the game's archive.
pub(super) fn vanilla_registry() -> Option<Vec<u8>> {
    super::super::game_resource(RECIPE_REGISTRY.key(types::RECIPE_REGISTRY_RESOURCE).ok()?)
}

/// The GUIDs of the recipes in `registry`.
#[cfg(feature = "unstable")]
pub(super) fn recipe_guids(
    layout: &RecipeLayout,
    registry: &[u8],
) -> Option<rustc_hash::FxHashSet<Guid>> {
    let blob = Blob::new(registry);
    let (start, count) = blob.array(layout.recipes)?;
    Some(
        (0..usize::try_from(count).ok()?)
            .map(|i| {
                RecipeView {
                    blob: &blob,
                    at: start + i * layout.recipe,
                    layout,
                }
                .guid()
            })
            .collect(),
    )
}

/// The GUID of the first recipe in `registry` that makes `item`.
pub(super) fn recipe_making(layout: &RecipeLayout, registry: &[u8], item: Guid) -> Option<Guid> {
    let registry = Blob::new(registry);
    let (start, count) = registry.array(layout.recipes)?;
    let at = (0..usize::try_from(count).ok()?)
        .map(|i| start + i * layout.recipe)
        .find(|&at| makes(layout, &registry, at, item))?;
    let view = RecipeView {
        blob: &registry,
        at,
        layout,
    };
    Some(view.guid())
}

/// Where the crafting menu lists `recipe`: after the vanilla recipe it
/// copies.
pub(super) fn menu_entry(layout: &RecipeLayout, recipe: &NewRecipe) -> MenuEntry {
    MenuEntry {
        recipe: recipe.id.guid(),
        after: vanilla_registry()
            .and_then(|registry| recipe_making(layout, &registry, recipe.like)),
        workshop: recipe.workshop,
    }
}

/// Where the crafting menu lists the copies [`derive_recipes_in`] makes of
/// the recipes in `registry`: each after its original.
#[cfg(feature = "unstable")]
pub(super) fn derived_entries(
    layout: &RecipeLayout,
    registry: &[u8],
    salt: u32,
    choose: &dyn Fn(&RecipeView<'_>) -> Option<Derived>,
) -> Option<Vec<MenuEntry>> {
    let blob = Blob::new(registry);
    let (start, count) = blob.array(layout.recipes)?;
    Some(
        (0..usize::try_from(count).ok()?)
            .filter_map(|i| {
                let view = RecipeView {
                    blob: &blob,
                    at: start + i * layout.recipe,
                    layout,
                };
                let derived = choose(&view)?;
                Some(MenuEntry {
                    recipe: derived_guid(view.guid(), salt),
                    after: Some(view.guid()),
                    workshop: derived.workshop,
                })
            })
            .collect(),
    )
}

/// The recipe's inputs with every count times `cost`; an input's item stack
/// comes first in it.
pub(super) fn scaled_inputs(
    layout: &RecipeLayout,
    registry: &Blob,
    at: usize,
    cost: u32,
) -> Option<Vec<u8>> {
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

/// Every recipe of `registry`: its GUID, debug name and the item it makes.
pub(crate) fn list(
    layout: &RecipeLayout,
    registry: &[u8],
) -> Option<Vec<(Guid, String, Option<Guid>)>> {
    let blob = Blob::new(registry);
    let (start, count) = blob.array(layout.recipes)?;
    Some(
        (0..usize::try_from(count).ok()?)
            .map(|i| {
                let view = RecipeView {
                    blob: &blob,
                    at: start + i * layout.recipe,
                    layout,
                };
                (
                    view.guid(),
                    view.debug_name(),
                    view.output().map(|(item, _)| item),
                )
            })
            .collect(),
    )
}

/// A vanilla recipe as `Load::derive_recipes` shows it.
///
/// # Examples
///
/// See `Load::derive_recipes`.
#[derive(Clone, Copy, Debug)]
pub struct RecipeView<'a> {
    pub(super) blob: &'a Blob,
    pub(super) at: usize,
    pub(super) layout: &'a RecipeLayout,
}

impl RecipeView<'_> {
    /// The recipe's GUID.
    ///
    /// # Examples
    ///
    /// See `Load::derive_recipes`.
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
    /// See `Load::derive_recipes`.
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
    /// See `Load::derive_recipes`.
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

    /// What the item the recipe makes is, as the game classes it; `None`
    /// until the game has loaded that item, and outside the game.
    ///
    /// A dedicated server builds its recipe registry before it loads the
    /// items, so the first build of a recipe patch sees no kinds there: a
    /// mod that changes recipes by kind calls [`rebuild_recipes`](super::rebuild_recipes) on
    /// [`WorldLoaded`](crate::events::WorldLoaded), and the rebuild applies
    /// them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::recipes::{Derived, RecipeKind, RecipeView};
    ///
    /// // Doubles what every decorative recipe makes, at the same cost.
    /// fn choose(recipe: &RecipeView<'_>) -> Option<Derived> {
    ///     (recipe.kind() == Some(RecipeKind::Decorative))
    ///         .then_some(Derived { output: 2, cost: 1, workshop: None })
    /// }
    /// ```
    #[must_use]
    pub fn kind(&self) -> Option<RecipeKind> {
        let (item, _) = self.output()?;
        RecipeKind::of(item)
    }
}

/// How a derived recipe differs from the vanilla one it copies.
///
/// # Examples
///
/// See `Load::derive_recipes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Derived {
    /// The vanilla output count times this.
    pub output: u32,
    /// The vanilla ingredient counts times this.
    pub cost: u32,
    /// The workshop the copy is made at, such as one from
    /// [`Load::add_workshop`](crate::Load::add_workshop); `None` keeps the vanilla recipe's.
    pub workshop: Option<WorkshopId>,
}

/// A derived recipe's GUID: the vanilla one marked with the mod's salt, so it
/// stays the same across starts and differs between mods.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::recipes::derived_guid;
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
/// recipe `choose` picks.
///
/// With a workshop `limit` other than [`usize::MAX`], copies stop
/// [`RESERVED_FOR_SINGLE`] short of it. `Load::derive_recipes` serves it;
/// `salt` marks the copies as in [`derived_guid`].
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::{
///     Derived, RECIPE_REGISTRY, RecipeLayout, derive_recipes_in, workshop_limit,
/// };
/// use enshwick::content::types;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let layout = RecipeLayout::resolve(load)?;
///     let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY_RESOURCE)?;
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
        let id = guid.fnv1a().to_le_bytes();
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

/// A recipe in the registry as [`modify_recipes_in`] shows it, with its
/// output open to change in place; the loader's recipe output rates use it.
#[doc(hidden)]
#[derive(Debug)]
pub struct RecipeEdit<'a> {
    blob: &'a mut Blob,
    at: usize,
    layout: &'a RecipeLayout,
    changed: bool,
}

impl RecipeEdit<'_> {
    /// The recipe as it reads now: GUID, debug name and output.
    #[must_use]
    pub fn view(&self) -> RecipeView<'_> {
        RecipeView {
            blob: self.blob,
            at: self.at,
            layout: self.layout,
        }
    }

    /// Multiplies the count of each item the recipe makes by `factor`,
    /// rounded half away from zero by [`scale_whole`](crate::rates::scale_whole):
    /// a stack the recipe made still makes at least one.
    pub fn scale_output_by(&mut self, factor: f32) {
        let Some((start, count)) = self.blob.array(self.at + self.layout.output) else {
            return;
        };
        for i in 0..count as usize {
            let at = start + i * self.layout.stack_size + self.layout.stack_count;
            let Some(made) = self.blob.read_u32(at) else {
                return;
            };
            if made == 0 {
                continue;
            }
            let scaled = crate::rates::scale_whole(i64::from(made), factor);
            let scaled = u32::try_from(scaled.max(1)).unwrap_or(u32::MAX);
            if scaled != made && self.blob.write(at, &scaled.to_le_bytes()) {
                self.changed = true;
            }
        }
    }
}

/// The recipe registry with `modify` called on each recipe in turn; the
/// second value is how many recipes it changed. The loader's recipe output
/// rates serve it.
#[doc(hidden)]
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
