//! Changes of one field of every vanilla recipe a selection picks, which the
//! loader turns into one change per recipe from the game's data.

use crate::abi::{ChangeOp, DataChange, DataSelect, Header, Str};
use crate::content::Guid;
use crate::content::recipes::RecipeKind;
use crate::content::workshops::WorkshopId;
use crate::{Load, Off};

use super::{Entry, Field, Value, ValueError, field_and_value};

/// The vanilla recipes a [`RecipesChange`] reaches at once.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::changes::{RecipeTraits, Recipes};
/// use enshwick::content::recipes::RecipeKind;
/// use enshwick::content::workshops::WorkshopId;
///
/// let kiln = WorkshopId::new(Guid([7; 16]));
/// let bricks = RecipeTraits { kind: RecipeKind::Other, workshop: kiln.id() };
/// assert!(Recipes::Workshop(kiln).picks(bricks));
/// assert!(!Recipes::Kind(RecipeKind::Roof).picks(bricks));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Recipes {
    /// Every recipe whose output is of this kind.
    Kind(RecipeKind),
    /// Every recipe made at this workshop.
    Workshop(WorkshopId),
}

/// What a selection reads of one vanilla recipe, from the game's data.
///
/// # Examples
///
/// See [`Recipes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeTraits {
    /// The kind of the item it makes.
    pub kind: RecipeKind,
    /// The game's id of its workshop, 0 for crafting by hand.
    pub workshop: u32,
}

impl Recipes {
    /// Whether the selection reaches `recipe`.
    ///
    /// # Examples
    ///
    /// See [`Recipes`].
    #[must_use]
    pub fn picks(self, recipe: RecipeTraits) -> bool {
        match self {
            Self::Kind(kind) => recipe.kind == kind,
            Self::Workshop(workshop) => recipe.workshop == workshop.id(),
        }
    }

    /// The selection's number in the ABI, its entry and its categories.
    const fn to_abi(self) -> (DataSelect, [u8; 16], u32) {
        match self {
            Self::Kind(kind) => (DataSelect::RECIPE_KIND, [0; 16], kind.code().0),
            Self::Workshop(workshop) => (DataSelect::WORKSHOP, workshop.guid().0, 0),
        }
    }

    /// The selection an ABI record names; `None` for any other.
    fn from_abi(raw: &DataChange) -> Option<Self> {
        match raw.select {
            DataSelect::RECIPE_KIND => {
                let kind = RecipeKind::from(crate::abi::RecipeKind(raw.categories));
                (kind.code().0 == raw.categories).then_some(Self::Kind(kind))
            }
            DataSelect::WORKSHOP => Some(Self::Workshop(WorkshopId::new(Guid(raw.entry)))),
            _ => None,
        }
    }
}

/// One change of a field of every vanilla recipe a selection picks.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, Recipes, RecipesChange, Value};
/// use enshwick::content::recipes::RecipeKind;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let roofs = RecipesChange::new(Recipes::Kind(RecipeKind::Roof), Field::Cost, Value::Times(0.5));
///     load.change_recipes(&roofs, "my-mod")
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecipesChange {
    /// The recipes.
    pub recipes: Recipes,
    /// What changes.
    pub field: Field,
    /// How.
    pub value: Value,
}

impl RecipesChange {
    /// The change of `field` of every recipe `recipes` picks by `value`.
    ///
    /// # Examples
    ///
    /// See [`RecipesChange`].
    #[must_use]
    pub const fn new(recipes: Recipes, field: Field, value: Value) -> Self {
        Self {
            recipes,
            field,
            value,
        }
    }

    /// Whether the loader takes the change: a field of a whole recipe,
    /// not one ingredient's, and a value [`Field::check`] takes.
    ///
    /// # Errors
    ///
    /// [`ValueError`] saying why not.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{Field, Recipes, RecipesChange, Value};
    /// use enshwick::content::recipes::RecipeKind;
    ///
    /// let stacks = RecipesChange::new(Recipes::Kind(RecipeKind::Roof), Field::MaxStackSize, Value::Set(2.0));
    /// assert_eq!(
    ///     stacks.check().unwrap_err().to_string(),
    ///     "max_stack is no field a selection of recipes changes"
    /// );
    /// ```
    pub fn check(&self) -> Result<(), ValueError> {
        if self.field.entry() != Entry::Recipe || self.field == Field::Ingredient {
            return Err(ValueError::NotRecipeSelectable { field: self.field });
        }
        self.field.check(self.value)
    }

    /// The same change of the one recipe `recipe`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::changes::{Field, Recipes, RecipesChange, Value};
    /// use enshwick::content::recipes::RecipeKind;
    ///
    /// let roofs = RecipesChange::new(Recipes::Kind(RecipeKind::Roof), Field::Cost, Value::Times(0.5));
    /// assert_eq!(roofs.of(Guid([7; 16])).entry, Guid([7; 16]));
    /// ```
    #[must_use]
    pub const fn of(&self, recipe: Guid) -> super::Change {
        super::Change::new(recipe, self.field, self.value)
    }

    /// The change an ABI record describes, checked as
    /// [`RecipesChange::check`] checks it; `None` for a record that selects
    /// no recipes, or a field or operation this crate does not know.
    ///
    /// # Errors
    ///
    /// [`ValueError`] for a field or value the selection does not take.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ChangeOp, DataChange, DataField, DataSelect, Header, RecipeKind, Str};
    /// use enshwick::content::changes::{Recipes, RecipesChange};
    ///
    /// let raw = DataChange {
    ///     header: Header::of::<DataChange>(),
    ///     entry: [0; 16],
    ///     field: DataField::RECIPE_TIME,
    ///     op: ChangeOp::TIMES,
    ///     value: 0.5,
    ///     source: Str::EMPTY,
    ///     item: [0; 16],
    ///     select: DataSelect::RECIPE_KIND,
    ///     categories: RecipeKind::ROOF.0,
    /// };
    /// let change = RecipesChange::from_abi(&raw)?.ok_or("unknown")?;
    /// assert_eq!(change.recipes, Recipes::Kind(enshwick::content::recipes::RecipeKind::Roof));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_abi(raw: &DataChange) -> Result<Option<Self>, ValueError> {
        let Some(recipes) = Recipes::from_abi(raw) else {
            return Ok(None);
        };
        let Some((field, value)) = field_and_value(raw) else {
            return Ok(None);
        };
        let change = Self::new(recipes, field, value);
        change.check()?;
        Ok(Some(change))
    }

    /// The ABI record of the change, with `source` borrowed into it.
    fn to_abi(self, source: &str) -> DataChange {
        let (op, value) = match self.value {
            Value::Set(v) => (ChangeOp::SET, v),
            Value::Times(v) => (ChangeOp::TIMES, v),
        };
        let (select, entry, categories) = self.recipes.to_abi();
        DataChange {
            header: Header::of::<DataChange>(),
            entry,
            field: self.field.to_abi(),
            op,
            value,
            source: Str::new(source),
            item: [0; 16],
            select,
            categories,
        }
    }
}

impl Load<'_> {
    /// Declares `change` during `init`, from `source`, as
    /// [`Load::change`] declares a change of one recipe; the loader turns it
    /// into one change of each vanilla recipe the selection picks, read from
    /// the game's data, in this mod's place in load order. Recipes of mods
    /// are not picked, and a recipe the mod changes by name keeps that
    /// change of the field alone.
    ///
    /// # Errors
    ///
    /// [`Off`] for a field a selection of recipes does not change, a value
    /// the field does not take, after init, and when the running build does
    /// not reflect the field.
    ///
    /// # Examples
    ///
    /// See [`RecipesChange`].
    pub fn change_recipes(&mut self, change: &RecipesChange, source: &str) -> Result<(), Off> {
        change.field.unstable_gate()?;
        change
            .check()
            .map_err(|why| Off::new(format!("change of {:?}: {why}", change.recipes)))?;
        let raw = change.to_abi(source);
        let api = self.host().api();
        let status = (api.change_data)(self.host().handle(), &raw const raw);
        crate::load::ok(status, (), || {
            format!("change {} of {:?}", change.field, change.recipes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KILN: WorkshopId = WorkshopId::new(Guid([7; 16]));

    #[test]
    fn a_selection_survives_its_abi_record() {
        for recipes in [Recipes::Kind(RecipeKind::Terrain), Recipes::Workshop(KILN)] {
            let change = RecipesChange::new(recipes, Field::Output, Value::Set(3.0));
            let raw = change.to_abi("x");
            assert_eq!(RecipesChange::from_abi(&raw), Ok(Some(change)));
        }
    }

    #[test]
    fn an_unknown_kind_code_selects_nothing() {
        let mut raw = RecipesChange::new(
            Recipes::Kind(RecipeKind::Roof),
            Field::Cost,
            Value::Set(1.0),
        )
        .to_abi("");
        raw.categories = 99;
        assert_eq!(RecipesChange::from_abi(&raw), Ok(None));
    }

    #[test]
    fn only_fields_of_a_whole_recipe_are_selected() {
        for field in Field::ALL {
            let value = if *field == Field::Station {
                Value::Set(0.0)
            } else {
                Value::Times(1.0)
            };
            let change = RecipesChange::new(Recipes::Workshop(KILN), *field, value);
            let whole = field.entry() == Entry::Recipe && *field != Field::Ingredient;
            assert_eq!(change.check().is_ok(), whole, "{field}");
        }
    }
}
