//! Presets: every engine fact one area's layouts resolve, set at once.
//!
//! A preset's values claim nothing about a game build; they only agree with
//! each other, so a test builds its blobs from the layout the preset gives
//! (through [`EngineFacts`]) rather than from offsets of its own.

use crate::Off;
use crate::content::EngineFacts;
use crate::content::icons::IMAGE_FIELDS;
use crate::engine_ids as ids;

use super::FakeHost;

impl FakeHost {
    /// Items and the item registry: what
    /// [`ItemLayout`](crate::content::items::ItemLayout) resolves, with the
    /// type hashes `add_item` serves under.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{EngineFacts, types};
    /// use enshwick::content::items::ItemLayout;
    ///
    /// let host = enshwick::test::FakeHost::new().items();
    /// let layout = ItemLayout::resolve(&host)?;
    /// assert!(layout.name < host.size(types::ITEM_INFO.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn items(self) -> Self {
        self.preset(
            &[ids::ITEM_INFO, ids::ITEM_REGISTRY],
            &[(ids::ITEM_INFO, 0xc0), (ids::ITEM_STAT, 0x28)],
            &[
                (ids::ITEM_INFO_ID, 0),
                (ids::ITEM_INFO_OBJECT, 4),
                (ids::ITEM_INFO_NAME, 0x48),
                (ids::ITEM_INFO_ICON_IMAGE, 0x58),
                (ids::ITEM_INFO_DESCRIPTION, 0x68),
                (ids::ITEM_INFO_FLAGS, 0x78),
                (ids::ITEM_INFO_PID_ENTITY, 0x80),
                (ids::ITEM_INFO_UI_VALUES, 0x90),
                (ids::ITEM_INFO_PLACED_ENTITY, 0xb0),
                (ids::ITEM_STAT_LABEL, 0),
                (ids::ITEM_STAT_FORMAT, 4),
                (ids::ITEM_STAT_VALUE, 8),
                (ids::ITEM_STAT_SIGNED, 0x24),
                (ids::ITEM_REGISTRY_REFS, 0),
                (ids::ITEM_REGISTRY_NAMES, 0x38),
            ],
        )
    }

    /// The recipe registry: what
    /// [`RecipeLayout`](crate::content::recipes::RecipeLayout) resolves, with
    /// the registry's type hash.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::recipes::RecipeLayout;
    ///
    /// let host = enshwick::test::FakeHost::new().recipes();
    /// let layout = RecipeLayout::resolve(&host)?;
    /// assert!(layout.output < layout.recipe);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn recipes(self) -> Self {
        self.preset(
            &[ids::RECIPE_REGISTRY],
            &[
                (ids::RECIPE, 0xa8),
                (ids::RECIPE_INGREDIENT, 0x18),
                (ids::RECIPE_STACK, 0x18),
            ],
            &[
                (ids::RECIPE_REGISTRY_RECIPES, 0),
                (ids::RECIPE_GUID, 0),
                (ids::RECIPE_ID, 0x10),
                (ids::RECIPE_WORKSHOP_GUID, 0x14),
                (ids::RECIPE_WORKSHOP, 0x24),
                (ids::RECIPE_INPUT, 0x28),
                (ids::RECIPE_OUTPUT, 0x30),
                (ids::RECIPE_REQUIRED_PROPS, 0x38),
                (ids::RECIPE_DEBUG_NAME, 0x40),
                (ids::RECIPE_COMPLETION, 0x5c),
                (ids::RECIPE_STACK_ITEM, 0x10),
                (ids::RECIPE_STACK_COUNT, 0x14),
            ],
        )
    }

    /// Workshops, templates and their crafting components: what
    /// [`WorkshopLayout`](crate::content::workshops::WorkshopLayout) resolves
    /// beside [`FakeHost::items`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::workshops::WorkshopLayout;
    ///
    /// let host = enshwick::test::FakeHost::new().items().workshops();
    /// let layout = WorkshopLayout::resolve(&host)?;
    /// assert!(layout.entry_npc < layout.entry);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn workshops(self) -> Self {
        self.preset(
            &[
                ids::TEMPLATE,
                ids::TEMPLATE_COLLECTION,
                ids::WORKSHOP_REGISTRY,
                ids::CRAFTING,
                ids::CRAFTING_INTERACTION,
                ids::CLIENT_CRAFTING_INTERACTION,
            ],
            &[
                (ids::CRAFTING, 0x18),
                (ids::CRAFTING_INTERACTION, 0x18),
                (ids::CLIENT_CRAFTING_INTERACTION, 8),
                (ids::WORKSHOP, 0x40),
            ],
            &[
                (ids::TEMPLATE_COMPONENTS, 0x10),
                (ids::TEMPLATE_COLLECTION_TEMPLATES, 0),
                (ids::CRAFTING_WORKSHOP, 0),
                (ids::CRAFTING_WORKSHOP_ID, 0x10),
                (ids::CRAFTING_INTERACTION_WORKSHOP, 0),
                (ids::CRAFTING_INTERACTION_WORKSHOP_ID, 0x10),
                (ids::CLIENT_CRAFTING_INTERACTION_WORKSHOP_ID, 0),
                (ids::WORKSHOP_REGISTRY_WORKSHOPS, 0),
                (ids::WORKSHOP_GUID, 0),
                (ids::WORKSHOP_ID, 0x10),
                (ids::WORKSHOP_NAME, 0x14),
                (ids::WORKSHOP_ITEM, 0x24),
                (ids::WORKSHOP_ITEM_REF, 0x34),
                (ids::WORKSHOP_NPC, 0x3c),
            ],
        )
    }

    /// The client's crafting menu in its UI bundle: what
    /// [`MenuLayout`](crate::content::menu::MenuLayout) resolves, with the
    /// bundle's type hash.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::menu::MenuLayout;
    ///
    /// let host = enshwick::test::FakeHost::new().crafting_menu();
    /// let layout = MenuLayout::resolve(&host)?;
    /// assert!(layout.sets < layout.group);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn crafting_menu(self) -> Self {
        self.preset(
            &[ids::UI_BUNDLE],
            &[
                (ids::CRAFTING_MENU_TREE, 8),
                (ids::CRAFTING_MENU_GROUP, 0x38),
                (ids::CRAFTING_MENU_SET, 0x28),
            ],
            &[
                (ids::UI_BUNDLE_CRAFTING_MENU, 0x40),
                (ids::CRAFTING_MENU_WORKSHOPS, 0),
                (ids::CRAFTING_MENU_TREES, 8),
                (ids::CRAFTING_MENU_MAX_GROUPS, 0x10),
                (ids::CRAFTING_MENU_MAX_SETS, 0x14),
                (ids::CRAFTING_MENU_TREE_GROUPS, 0),
                (ids::CRAFTING_MENU_GROUP_ID, 0),
                (ids::CRAFTING_MENU_GROUP_SETS, 0x30),
                (ids::CRAFTING_MENU_SET_ENTRIES, 0x20),
            ],
        )
    }

    /// The client's UI images, item icons among them: what
    /// [`ImageLayout`](crate::content::icons::ImageLayout) resolves, with
    /// the image type's hash and size.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{EngineFacts, types};
    /// use enshwick::content::icons::ImageLayout;
    ///
    /// let host = enshwick::test::FakeHost::new().ui_images();
    /// let layout = ImageLayout::resolve(&host)?;
    /// assert!(layout.levels < host.size(types::UI_TEXTURE_RESOURCE.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn ui_images(self) -> Self {
        let [content, width, height, flags, format, levels] = IMAGE_FIELDS;
        self.preset(
            &[ids::UI_IMAGE],
            &[(ids::UI_IMAGE, 0x40)],
            &[
                (content, 0),
                (width, 0x28),
                (height, 0x2c),
                (flags, 0x30),
                (format, 0x31),
                (levels, 0x32),
            ],
        )
    }

    /// The world's own component types: what `World` needs before it
    /// resolves, the current transform and the used item.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    ///
    /// let host = enshwick::test::FakeHost::new().world();
    /// let transform = host.hash(enshwick::__private::ids::CURRENT_TRANSFORM)?;
    /// let loader = host.placed(transform, enshwick::Entity(2), [0.0; 3]).install();
    /// # let _ = loader;
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn world(self) -> Self {
        self.preset(&[ids::CURRENT_TRANSFORM, ids::USED_ITEM], &[], &[])
    }

    /// Sets a hash for each of `types`, then `sizes` and `offsets`.
    fn preset(mut self, types: &[&str], sizes: &[(&str, u32)], offsets: &[(&str, u64)]) -> Self {
        for &id in types {
            self = self.type_hash(id, preset_hash(id));
        }
        for &(id, size) in sizes {
            self = self.type_size(id, size);
        }
        for &(id, offset) in offsets {
            self = self.constant(id, offset);
        }
        self
    }
}

/// The type hash a preset gives `id`: its FNV-1a, so presets never share a
/// hash and a test's own small hashes stay clear of them.
fn preset_hash(id: &str) -> u32 {
    id.bytes().fold(0x811c_9dc5, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

/// The facts the builder holds, so a test lays out its blobs by the same
/// layouts the mod resolves.
impl EngineFacts for FakeHost {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        self.constants
            .get(id)
            .and_then(|&offset| usize::try_from(offset).ok())
            .ok_or_else(|| Off::new(format!("offset {id}: not set on the fake")))
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        self.type_sizes
            .get(id)
            .and_then(|&size| usize::try_from(size).ok())
            .ok_or_else(|| Off::new(format!("type {id}: no size set on the fake")))
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        self.type_hashes
            .get(id)
            .copied()
            .ok_or_else(|| Off::new(format!("type {id}: no hash set on the fake")))
    }
}
