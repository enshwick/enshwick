//! Disabling and enabling recipes by name, GUID or id.

use crate::Off;

use super::super::{DebugName, Guid, Id};

/// A recipe, an item or a workshop item as a recipe rule names it: by GUID,
/// by content id, or by the debug name the loader resolves against the
/// registry.
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::Name;
/// use enshwick::content::{DebugName, Guid, Id};
///
/// let by_guid = Name::from(Guid::parse("7fa62007-a38e-4790-8591-873055fc3f34"));
/// let by_id = Name::from(Id::new("safes:safe.recipe"));
/// let by_debug = Name::from(DebugName("Tool_Torch01_T0_PK"));
/// assert_ne!(by_guid, by_id);
/// assert_ne!(by_id, by_debug);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Name<'a> {
    /// A GUID, such as a vanilla recipe's.
    Guid(Guid),
    /// A mod's content id.
    Content(Id),
    /// A debug name, such as `Tool_Torch01_T0_PK`.
    Debug(DebugName<'a>),
}

/// What a recipe rule disables, in the forms the loader's recipe file
/// takes.
///
/// # Examples
///
/// See [`disable`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RecipeRule<'a> {
    /// One recipe.
    Recipe(Name<'a>),
    /// Every recipe that makes this item.
    Output(Name<'a>),
    /// Every recipe made at the workshop of this item, such as
    /// `Craftingstation_Workbench` or a survivor's `NPC_Crafter_Blacksmith01`.
    Workshop(Name<'a>),
    /// Every recipe crafted by hand.
    Hand,
}

fn rule(rule: RecipeRule<'_>, disable: bool) -> Result<(), Off> {
    let host = crate::host().ok_or_else(|| Off::new("recipe rules: no loader"))?;
    let api = host.api();
    let (kind, what, name) = rule.parts();
    if name.trim().is_empty() {
        return Err(Off::new(format!("recipe rule {what}: no name")));
    }
    let status = (api.recipe_rule)(
        host.handle(),
        kind,
        crate::abi::Str::new(&name),
        u32::from(disable),
    );
    crate::load::ok(status, (), || format!("recipe rule {what} {name:?}"))
}

/// Disables recipes in the whole game while the mod runs: they leave every
/// crafting menu, and the server refuses to craft them. On the server and in
/// single player, the server's rules sync to the players who join. The
/// loader resolves the name against the recipe registry and logs one it
/// cannot resolve in the mod's log; the server's recipe file
/// (`mod-configs/enshwick.recipes.toml`) wins over every mod's rules. Call it
/// at any time, from any thread; it applies within a second.
///
/// # Errors
///
/// [`Off`] outside the game, or for an empty debug name.
///
/// # Examples
///
/// ```
/// use enshwick::content::DebugName;
/// use enshwick::content::recipes::{RecipeRule, disable, enable};
///
/// fn init(_load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     disable(RecipeRule::output(DebugName("Tool_Torch01_T0")))?;
///     disable(RecipeRule::Hand)?;
///     enable(RecipeRule::Hand)
/// }
/// ```
pub fn disable(rule: RecipeRule<'_>) -> Result<(), Off> {
    self::rule(rule, true)
}

/// Takes back the mod's own [`disable`] of `rule`; another mod's rules and
/// the server's recipe file stay as they are.
///
/// # Errors
///
/// As [`disable`].
///
/// # Examples
///
/// See [`disable`].
pub fn enable(rule: RecipeRule<'_>) -> Result<(), Off> {
    self::rule(rule, false)
}

/// Whether the recipe `recipe`, a GUID or a content id, is disabled in this
/// process now, by the server's recipe file or a mod's rule; on a client
/// joined to a server, by the server's. `false` outside the game.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id};
/// use enshwick::content::recipes::is_disabled;
///
/// let torch = Guid::parse("7fa62007-a38e-4790-8591-873055fc3f34");
/// assert!(!is_disabled(torch));
/// assert!(!is_disabled(Id::new("safes:safe.recipe")));
/// ```
#[must_use]
pub fn is_disabled(recipe: impl Into<Guid>) -> bool {
    let Some(host) = crate::host() else {
        return false;
    };
    let api = host.api();
    let mut out = 0u32;
    (api.recipe_disabled)(host.handle(), recipe.into().id(), &raw mut out).is_ok() && out != 0
}

impl From<Guid> for Name<'_> {
    fn from(guid: Guid) -> Self {
        Self::Guid(guid)
    }
}

impl From<Id> for Name<'_> {
    fn from(id: Id) -> Self {
        Self::Content(id)
    }
}

impl<'a> From<DebugName<'a>> for Name<'a> {
    fn from(name: DebugName<'a>) -> Self {
        Self::Debug(name)
    }
}

impl Name<'_> {
    /// The name as the loader's recipe file writes it.
    fn text(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Guid(guid) => guid.to_string().into(),
            Self::Content(id) => id.name().into(),
            Self::Debug(name) => name.0.to_owned().into(),
        }
    }
}

impl<'a> RecipeRule<'a> {
    /// The rule for one recipe.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Id;
    /// use enshwick::content::recipes::{Name, RecipeRule};
    ///
    /// const SAFE_RECIPE: Id = Id::new("safes:safe.recipe");
    /// assert_eq!(RecipeRule::recipe(SAFE_RECIPE), RecipeRule::Recipe(Name::Content(SAFE_RECIPE)));
    /// ```
    #[must_use]
    pub fn recipe(name: impl Into<Name<'a>>) -> Self {
        Self::Recipe(name.into())
    }

    /// The rule for every recipe that makes an item.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::DebugName;
    /// use enshwick::content::recipes::RecipeRule;
    ///
    /// let torches = RecipeRule::output(DebugName("Tool_Torch01_T0"));
    /// # let _ = torches;
    /// ```
    #[must_use]
    pub fn output(item: impl Into<Name<'a>>) -> Self {
        Self::Output(item.into())
    }

    /// The rule for every recipe made at the workshop of an item.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::DebugName;
    /// use enshwick::content::recipes::RecipeRule;
    ///
    /// let workbench = RecipeRule::workshop(DebugName("Craftingstation_Workbench"));
    /// # let _ = workbench;
    /// ```
    #[must_use]
    pub fn workshop(item: impl Into<Name<'a>>) -> Self {
        Self::Workshop(item.into())
    }

    /// The rule's kind, its kind in words and the name it carries.
    fn parts(
        self,
    ) -> (
        crate::abi::RecipeRuleKind,
        &'static str,
        std::borrow::Cow<'static, str>,
    ) {
        use crate::abi::RecipeRuleKind;
        match self {
            Self::Recipe(name) => (RecipeRuleKind::RECIPE, "recipe", name.text()),
            Self::Output(name) => (RecipeRuleKind::OUTPUT, "output", name.text()),
            Self::Workshop(name) => (RecipeRuleKind::WORKSHOP, "workshop", name.text()),
            Self::Hand => (RecipeRuleKind::WORKSHOP, "workshop", "hand".into()),
        }
    }
}
