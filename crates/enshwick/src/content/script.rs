//! Content as a script sees it: ids and GUIDs as text, item categories by
//! the game's names, and the module's free functions as the namespace
//! `content`.

use super::categories::ItemCategoryMask;
use super::changes::ChangeHandle;
use super::items::{FuelSetup, fill};
use super::recipes::{self as rules, RecipeKind, RecipeRule, RecipeView, derived_guid};
use super::renames::{Renames, current_id};
use super::vanilla::{self, LootTable, Vanilla};
use super::workshops::{Station, WorkshopId};
use super::{Guid, Id, is_id};
use crate::resources::{Bytes, Value as Data};
use crate::script::{
    Class, Error, Field, FromScript, Function, IntoScript, Object, Record, Ty, Typed, Value,
};
use crate::{ItemId, Off};

impl Typed for Guid {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for Guid {
    fn into_value(self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromScript for Guid {
    fn from_value(value: Value) -> Result<Self, Error> {
        let text = String::from_value(value)?;
        Self::try_parse(&text).ok_or_else(|| Error::new(format!("{text:?} is no GUID")))
    }
}

impl Typed for Id {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for Id {
    fn into_value(self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromScript for Id {
    fn from_value(value: Value) -> Result<Self, Error> {
        let text = String::from_value(value)?;
        if !is_id(&text) {
            return Err(Error::new(format!(
                "{text:?} is no content id, <mod>:<name>"
            )));
        }
        let text = ScriptIds::keep(text);
        Self::try_new(text).ok_or_else(|| Error::new(format!("{text:?} is no content id")))
    }
}

/// The content id texts scripts passed, each kept once for the process, as
/// a pack's are: a handler that names an id on every event keeps no more.
struct ScriptIds(rustc_hash::FxHashSet<&'static str>);

static SCRIPT_IDS: std::sync::Mutex<ScriptIds> = std::sync::Mutex::new(ScriptIds(
    rustc_hash::FxHashSet::with_hasher(rustc_hash::FxBuildHasher),
));

impl ScriptIds {
    /// `text` for the process, the copy kept before when there is one.
    fn keep(text: String) -> &'static str {
        use crate::Relock as _;
        let mut ids = SCRIPT_IDS.relock();
        if let Some(&kept) = ids.0.get(text.as_str()) {
            return kept;
        }
        let kept: &'static str = Box::leak(text.into_boxed_str());
        ids.0.insert(kept);
        kept
    }
}

impl Typed for ItemCategoryMask {
    const TY: &'static Ty = &Ty::List(&Ty::String);
}

impl IntoScript for ItemCategoryMask {
    fn into_value(self) -> Value {
        Value::List(
            Self::NAMES
                .iter()
                .filter(|(_, mask)| self.contains(*mask))
                .map(|(name, _)| Value::String((*name).to_owned()))
                .collect(),
        )
    }
}

impl FromScript for ItemCategoryMask {
    fn from_value(value: Value) -> Result<Self, Error> {
        let names: Vec<String> = Vec::from_value(value)?;
        names.iter().try_fold(Self::default(), |mask, name| {
            Self::named(name)
                .map(|one| mask.union(one))
                .ok_or_else(|| Error::new(format!("{name:?} is no item category of the game")))
        })
    }
}

impl Typed for &'static [u8] {
    const TY: &'static Ty = &Ty::Bytes;
}

impl IntoScript for &'static [u8] {
    fn into_value(self) -> Value {
        Value::Bytes(self.to_vec())
    }
}

impl FromScript for &'static [u8] {
    fn from_value(value: Value) -> Result<Self, Error> {
        let Value::Bytes(bytes) = value else {
            return Err(Error::new("expected a buffer"));
        };
        // shortcut: an icon's PNG from a script lives for the process, as an
        // embedded one does; icons are few and given while the mod starts.
        Ok(Box::leak(bytes.into_boxed_slice()))
    }
}

impl Typed for RecipeView<'_> {
    const TY: &'static Ty = &Ty::Record(&Record::new(
        "RecipeView",
        "A vanilla recipe as `derive_recipes` shows it.",
        &[
            Field::new("guid", "The recipe's GUID.", <Guid as Typed>::TY),
            Field::new(
                "debug_name",
                "Its name in the registry's debug list.",
                &Ty::String,
            ),
            Field::new(
                "output",
                "The item it makes; nil for a recipe that makes nothing.",
                &Ty::Optional(<Guid as Typed>::TY),
            ),
            Field::new("output_count", "How many it makes.", &Ty::Number),
            Field::new(
                "kind",
                "What kind of thing it makes, when the game says.",
                &Ty::Optional(<RecipeKind as Typed>::TY),
            ),
        ],
    ));
}

impl IntoScript for RecipeView<'_> {
    fn into_value(self) -> Value {
        let output = self.output();
        Value::Record(vec![
            self.guid().into_value(),
            self.debug_name().into_value(),
            output.map(|(item, _)| item).into_value(),
            output.map_or(0, |(_, count)| count).into_value(),
            self.kind().into_value(),
        ])
    }
}

impl Typed for Renames {
    const TY: &'static Ty = &Ty::List(&Ty::Record(&Record::new(
        "ContentRename",
        "One content GUID the save holds and the GUID it reads as from now on.",
        &[
            Field::new(
                "from",
                "The GUID as the save holds it.",
                <Guid as Typed>::TY,
            ),
            Field::new("to", "The GUID it reads as.", <Guid as Typed>::TY),
        ],
    )));
}

impl FromScript for Renames {
    fn from_value(value: Value) -> Result<Self, Error> {
        let Value::List(pairs) = value else {
            return Err(Error::new("expected a list of { from, to }"));
        };
        pairs
            .into_iter()
            .enumerate()
            .try_fold(Self::new(), |renames, (i, pair)| {
                let mut fields = crate::script::convert::record_fields(pair, 2)
                    .map_err(|e| e.within(&format!("[{}]", i + 1)))?;
                let from: Guid = crate::script::convert::argument(&mut fields, "from")?;
                let to: Guid = crate::script::convert::argument(&mut fields, "to")?;
                Ok(renames.rename(from, to))
            })
    }
}

impl Object for ChangeHandle {
    const CLASS: &'static Class = &Class::of::<Self>(
        "ChangeHandle",
        "A change of a vanilla value the mod sets and resets while the game runs.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_HANDLE];
}

/// The free functions of [`content`](super) as a script's namespace
/// `content`: vanilla lookups, recipe rules and helpers.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct Content;

#[crate::script(content_lookups)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl Content {
    /// The vanilla item named `name`, as the game's data names it, such as
    /// `Chest_Wood`; an error names the nearest names.
    ///
    /// # Errors
    ///
    /// As [`vanilla::item`].
    #[script]
    pub fn item(self, name: &str) -> Result<Guid, vanilla::VanillaError> {
        vanilla::item(name)
    }

    /// The item id of the content id `id` in full (`<mod>:<name>`), as
    /// `ItemId::named` gives it in Rust, for a mod's own items in
    /// `enshwick.items`.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed content id.
    #[script]
    pub fn item_id(self, id: &str) -> Result<ItemId, Off> {
        Guid::try_named(id)
            .map(ItemId::from_guid)
            .ok_or_else(|| Off::new(format!("{id:?} is no content id")))
    }

    /// The vanilla recipe named `name`.
    ///
    /// # Errors
    ///
    /// As [`vanilla::recipe`].
    #[script]
    pub fn recipe(self, name: &str) -> Result<Guid, vanilla::VanillaError> {
        vanilla::recipe(name)
    }

    /// The vanilla loot table named `name` and how many containers roll it.
    ///
    /// # Errors
    ///
    /// As [`vanilla::loot_table`].
    #[script]
    pub fn loot_table(self, name: &str) -> Result<LootTable, vanilla::VanillaError> {
        vanilla::loot_table(name)
    }

    /// The vanilla station item named `name` and the template it places.
    ///
    /// # Errors
    ///
    /// As [`vanilla::station`].
    #[script]
    pub fn station(self, name: &str) -> Result<Station, vanilla::VanillaError> {
        vanilla::station(name)
    }

    /// The workshop of the vanilla station item named `name`.
    ///
    /// # Errors
    ///
    /// As [`vanilla::workshop`].
    #[script]
    pub fn workshop(self, name: &str) -> Result<WorkshopId, vanilla::VanillaError> {
        vanilla::workshop(name)
    }

    /// How many slots the container that `item` places holds; nil for an
    /// item that places none.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, else as [`Vanilla::container_slots`].
    #[script]
    pub fn container_slots(self, item: Guid) -> Result<Option<u8>, Off> {
        Ok(game()?.container_slots(item)?)
    }

    /// Every vanilla recipe that makes `item`.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, else as [`Vanilla::recipes_making`].
    #[script]
    pub fn recipes_making(self, item: Guid) -> Result<Vec<Guid>, Off> {
        Ok(game()?.recipes_making(item)?)
    }

    /// Turns off the recipes `rule` names: `{ Recipe = name }`,
    /// `{ Output = item name }`, `{ Workshop = station item name }` or
    /// `"Hand"`; a name is a GUID, a content id or a debug name.
    ///
    /// # Errors
    ///
    /// As [`rules::disable`].
    #[script]
    pub fn disable(self, rule: &Rule) -> Result<(), Off> {
        rules::disable(rule.as_rule())
    }

    /// Turns the recipes `rule` names back on.
    ///
    /// # Errors
    ///
    /// As [`rules::enable`].
    #[script]
    pub fn enable(self, rule: &Rule) -> Result<(), Off> {
        rules::enable(rule.as_rule())
    }

    /// Whether the recipe `recipe` is turned off.
    #[script]
    #[must_use]
    pub fn is_disabled(self, recipe: Guid) -> bool {
        rules::is_disabled(recipe)
    }

    /// Builds the game's recipes again, with every rule as it is now.
    ///
    /// # Errors
    ///
    /// As [`super::recipes::rebuild_recipes`].
    #[script]
    pub fn rebuild_recipes(self) -> Result<(), Off> {
        super::recipes::rebuild_recipes()
    }

    /// The item id `id` reads as now, after every mod's renames.
    #[script]
    #[must_use]
    pub fn current_id(self, id: ItemId) -> ItemId {
        current_id(id)
    }

    /// The tag of a vanilla item's name, from the game's archive.
    #[script]
    #[must_use]
    pub fn vanilla_name_tag(self, item: Guid) -> Option<Guid> {
        super::items::vanilla_name_tag(item)
    }

    /// The name a vanilla item has in `language`, on a client.
    #[script]
    #[must_use]
    pub fn vanilla_name(self, item: Guid, language: crate::i18n::Language) -> Option<String> {
        super::items::vanilla_name(item, language).map(str::to_owned)
    }

    /// The fill that makes the game give an item `count` of `max`.
    #[script]
    #[must_use]
    pub fn fill(self, count: u16, max: u16) -> f32 {
        fill(count, max)
    }

    /// The fill a meter's `start` stands for.
    #[script]
    #[must_use]
    pub fn start_fill(self, meter: FuelSetup) -> f32 {
        meter.start_fill()
    }

    /// The GUID a derived copy of the recipe `base` takes under `salt`.
    #[script]
    #[must_use]
    pub fn derived_guid(self, base: Guid, salt: u32) -> Guid {
        derived_guid(base, salt)
    }

    /// Whether `id` is a content id, `<mod>:<name>`.
    #[script]
    #[must_use]
    pub fn is_id(self, id: &str) -> bool {
        is_id(id)
    }

    /// The GUID of the content id `id`.
    #[script]
    #[must_use]
    pub fn guid(self, id: Id) -> Guid {
        id.guid()
    }

    /// The pixels of PNG `png` as `{ width, height, pixels }`, the pixels a
    /// buffer of RGBA bytes, row by row; nil when it does not read or is
    /// larger than 2048 on a side.
    #[script]
    #[must_use]
    pub fn decode_png(self, png: &Bytes) -> Option<Data> {
        super::icons::decode_png(&png.0).map(|(pixels, width, height)| {
            Data::Struct(vec![
                ("width".into(), Data::Uint(width.into())),
                ("height".into(), Data::Uint(height.into())),
                ("pixels".into(), Data::Bytes(pixels)),
            ])
        })
    }

    /// RGBA `rgba`, `width` by `height`, as PNG bytes.
    ///
    /// # Errors
    ///
    /// [`Off`] when the pixels are not `width * height * 4` bytes.
    #[script]
    pub fn encode_png(self, rgba: &Bytes, width: u32, height: u32) -> Result<Bytes, Off> {
        super::icons::encode_png(&rgba.0, width, height)
            .map(Bytes)
            .ok_or_else(|| Off::new(format!("pixels are not {width} by {height} RGBA")))
    }

    /// The RGBA pixels of a texture's content `data`, `width` by `height`,
    /// in the game's `format` code (a block-compressed format or RGBA).
    ///
    /// # Errors
    ///
    /// [`Off`] for a format this API does not read and data too short.
    #[script]
    pub fn decode(self, format: u8, width: u32, height: u32, data: &Bytes) -> Result<Bytes, Off> {
        super::icons::decode(format, width, height, &data.0)
            .map(Bytes)
            .ok_or_else(|| Off::new(format!("texture format {format} does not read")))
    }

    /// RGBA `rgba` moved toward `tint`'s colour by its amount; a script's
    /// buffer comes back changed, as Rust changes the slice in place.
    #[script]
    #[must_use]
    pub fn tint(self, rgba: &Bytes, tint: super::icons::Tint) -> Bytes {
        let mut pixels = rgba.0.clone();
        super::icons::tint(&mut pixels, tint);
        Bytes(pixels)
    }

    /// The most recipes one workshop lists.
    #[script]
    #[must_use]
    pub fn workshop_limit(self) -> usize {
        super::recipes::workshop_limit()
    }

    /// The new ceiling a roll scaled to `scaled` of `max` takes when the
    /// most it rolls becomes `new_max`.
    #[script]
    #[must_use]
    pub fn scaled_ceiling(self, scaled: u16, max: u16, new_max: u16) -> u16 {
        super::loot_tables::scaled_ceiling(scaled, max, new_max)
    }
}

impl crate::script::Bound for Content {
    const NAME: &'static str = "content";
    const DOC: &'static str =
        "Game content from mods: new items, recipes and workshops, and changes to vanilla ones.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_CONTENT_LOOKUPS];

    fn resolve(_load: &mut crate::Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

#[cfg(feature = "unstable")]
#[crate::script(content_recipe_with)]
impl crate::Load<'_> {
    /// Adds a recipe as `add_recipe` does, asking `recipe` for it each time
    /// the registry is built, so a cost read from settings applies through
    /// `rebuild_recipes`. Every answer keeps the id of the first; an answer
    /// that fails or is nil keeps the first recipe. Unstable.
    #[script(name = "add_recipe_with")]
    pub(crate) fn add_recipe_with_script(
        &mut self,
        recipe: impl Fn() -> Option<super::recipes::NewRecipe> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let first =
            recipe().ok_or_else(|| Off::new("add_recipe_with: the first answer is no recipe"))?;
        self.add_recipe_with(move || recipe().unwrap_or(first))
    }
}

/// The running game's vanilla content.
fn game() -> Result<&'static Vanilla, Off> {
    Vanilla::game().ok_or_else(|| Off::new("the game's content: outside the game"))
}

/// A [`RecipeRule`] a script gives, owning its names.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub enum Rule {
    /// One recipe, by its name.
    Recipe(String),
    /// Every recipe that makes this item, by its name.
    Output(String),
    /// Every recipe made at the workshop of this station item, by its name.
    Workshop(String),
    /// Every recipe crafted by hand.
    Hand,
}

impl Rule {
    fn as_rule(&self) -> RecipeRule<'_> {
        match self {
            Self::Recipe(name) => RecipeRule::recipe(Self::name(name)),
            Self::Output(name) => RecipeRule::output(Self::name(name)),
            Self::Workshop(name) => RecipeRule::workshop(Self::name(name)),
            Self::Hand => RecipeRule::Hand,
        }
    }

    /// `text` as a name: a GUID, a content id, else a debug name.
    fn name(text: &str) -> rules::Name<'_> {
        if let Some(guid) = Guid::try_parse(text) {
            return rules::Name::Guid(guid);
        }
        match Id::from_value(Value::String(text.to_owned())) {
            Ok(id) if is_id(text) => rules::Name::Content(id),
            _ => rules::Name::Debug(super::DebugName(text)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_content_id_a_script_names_twice_keeps_one_text() {
        let first = Id::from_value(Value::String("you.mod:twice".into())).unwrap();
        let second = Id::from_value(Value::String("you.mod:twice".into())).unwrap();
        assert!(core::ptr::eq(first.namespace(), second.namespace()));
    }
}
