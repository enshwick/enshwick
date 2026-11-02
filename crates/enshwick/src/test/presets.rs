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
use super::facts::Fact;

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

    /// The world's own component types and where a placed thing keeps its
    /// transform: what `World` needs before it resolves, and what its prop
    /// reads and the effects' transforms lay out by.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().world();
    /// let transform = host.hash(facts::CURRENT_TRANSFORM.id())?;
    /// let loader = host.placed(transform, enshwick::Entity(2), [0.0; 3]).install();
    /// # let _ = loader;
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn world(self) -> Self {
        self.preset(
            &[ids::CURRENT_TRANSFORM, ids::USED_ITEM],
            &[],
            &[
                (ids::CURRENT_TRANSFORM_TRANSFORM, 0),
                (ids::WORLD_TRANSFORM_POSITION, 0),
                (ids::WORLD_TRANSFORM_ORIENTATION, 24),
                (ids::WORLD_TRANSFORM_SCALE, 40),
            ],
        )
    }

    /// Inventories on the server: the component types, stack size and
    /// offsets [`Inventories`](crate::inventory::Inventories) reads slots,
    /// durability and meters by. The ECS reads and the skipped entity stay
    /// the test's own symbols.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().inventory();
    /// let stack = host.size(facts::ITEM_STACK.id())?;
    /// assert!(host.offset(facts::ITEM_STACK_PIDE.id())? + 4 <= stack);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn inventory(self) -> Self {
        self.preset(
            &[
                ids::INVENTORY_SETUP,
                ids::INVENTORY,
                ids::DURABILITY,
                ids::FUEL,
                ids::OWNER,
            ],
            &[(ids::ITEM_STACK, 12)],
            &[
                (ids::INVENTORY_SETUP_LINKS, 0),
                (ids::INVENTORY_SETUP_CATEGORIES, 0x40),
                (ids::INVENTORY_SETUP_INITIALIZED, 0x53),
                (ids::ITEM_STACK_COUNT, 4),
                (ids::ITEM_STACK_PIDE, 8),
                (ids::DURABILITY_MAX, 2),
                (ids::FUEL_MAX, 2),
            ],
        )
    }

    /// The hit, death and equipment change game events, with the hit
    /// history and experience credit a death reads its killer from, and
    /// damage bits, an attack category and an equipment slot numbered unlike
    /// the API's own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().game_events();
    /// let target = host.offset(facts::HIT_EVENT_TARGET_ID.id())?;
    /// assert!(target + 4 <= host.size(facts::HIT_EVENT.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn game_events(self) -> Self {
        const HIT: u64 = 64;
        self.preset(
            &[
                ids::HIT_EVENT,
                ids::ENTITY_DIED_EVENT,
                ids::HIT_HISTORY,
                ids::EXPERIENCE_CREDIT,
                ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT,
            ],
            &[
                (ids::HIT_EVENT, 64),
                (ids::ENTITY_DIED_EVENT, 16),
                (ids::HIT_HISTORY, 152),
                (ids::EXPERIENCE_CREDIT, 8),
                (ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT, 16),
            ],
            &[
                (ids::HIT_EVENT_ROOT_SOURCE_ID, 8),
                (ids::HIT_EVENT_TARGET_ID, 12),
                (ids::HIT_EVENT_SOURCE_ID, 16),
                (ids::HIT_EVENT_ROOT_TARGET_ID, 20),
                (ids::HIT_EVENT_WEAPON_PIDE_ID, 24),
                (ids::HIT_HEALTH_CHANGE, 28),
                (ids::HIT_STAMINA_CHANGE, 32),
                (ids::HIT_DAMAGE_TYPES, 36),
                (ids::HIT_EVENT_ATTACK_CATEGORY, 38),
                (ids::HIT_TARGET_FACTION, 39),
                (ids::ENTITY_DIED_EVENT_DEAD_ENTITY_ID, 8),
                (ids::HIT_HISTORY_LAST_INDEX, 0),
                (ids::HIT_HISTORY_ENTRIES, 8),
                (ids::HIT_HISTORY_END, 8 + 2 * HIT),
                (ids::EXPERIENCE_CREDIT_LAST_PLAYER, 4),
                (ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT, 12),
                (ids::DAMAGE_TYPE_MAGICAL_FIRE, 0),
                (ids::DAMAGE_TYPE_PHYSICAL_CUT, 5),
                (ids::ATTACK_CATEGORY_MELEE, 2),
                (ids::EQUIPMENT_SLOT_SHIELD, 40),
            ],
        )
    }

    /// Each player's map waypoint: what `Map::waypoint` reads beside the
    /// markers.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().map_waypoints();
    /// let position = host.offset(facts::PLAYER_WAYPOINT_POSITION.id())?;
    /// assert!(position + 24 <= host.size(facts::PLAYER_WAYPOINT.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn map_waypoints(self) -> Self {
        self.preset(
            &[ids::PLAYER_WAYPOINT],
            &[(ids::PLAYER_WAYPOINT, 32)],
            &[
                (ids::PLAYER_WAYPOINT_ENABLED, 0),
                (ids::PLAYER_WAYPOINT_POSITION, 8),
            ],
        )
    }

    /// The events [`Effects`](crate::effects::Effects) raises for an NPC
    /// command, a threat and a ping; the toggle event stays off.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().effect_events();
    /// let position = host.offset(facts::UI_PING_EVENT_POSITION.id())?;
    /// assert!(position + 24 <= host.size(facts::UI_PING_EVENT.id())?);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn effect_events(self) -> Self {
        self.preset(
            &[
                ids::NPC_COMMAND_EVENT,
                ids::ADD_THREAT_TARGET_EVENT,
                ids::UI_PING_EVENT,
            ],
            &[
                (ids::NPC_COMMAND_EVENT, 72),
                (ids::ADD_THREAT_TARGET_EVENT, 24),
                (ids::UI_PING_EVENT, 40),
            ],
            &[
                (ids::NPC_COMMAND_EVENT_TYPE, 8),
                (ids::NPC_COMMAND_EVENT_NPC_ID, 12),
                (ids::NPC_COMMAND_EVENT_TARGET_TRANSFORM, 16),
                (ids::ADD_THREAT_TARGET_EVENT_ATTACKER_ID, 8),
                (ids::ADD_THREAT_TARGET_EVENT_TARGET_ID, 12),
                (ids::ADD_THREAT_TARGET_EVENT_THREAT_VALUE, 16),
                (ids::UI_PING_EVENT_SENDER_ENTITY_ID, 8),
                (ids::UI_PING_POSITION, 16),
            ],
        )
    }

    /// The components loot from destroyed things checks: destroyed on loot,
    /// boss and the interaction lock with its item. The loot slot stays the
    /// test's own, as the single roll links only where a build names it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new().loot();
    /// assert!(host.size(facts::LOOT_SLOT.id()).is_err());
    /// ```
    #[must_use]
    pub fn loot(self) -> Self {
        self.preset(
            &[ids::DESTROY_ON_LOOT, ids::BOSS, ids::INTERACTION_LOCK],
            &[],
            &[(ids::INTERACTION_LOCK_ITEM, 0)],
        )
    }

    /// Plays a build that lacks `fact`, whichever preset set it, so a test
    /// of what turns off without it starts from the preset.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::test::facts;
    ///
    /// let host = enshwick::test::FakeHost::new()
    ///     .world()
    ///     .without_fact(facts::WORLD_TRANSFORM_ORIENTATION);
    /// assert!(host.offset(facts::WORLD_TRANSFORM_ORIENTATION.id()).is_err());
    /// ```
    #[must_use]
    pub fn without_fact(mut self, fact: Fact) -> Self {
        self.type_hashes.remove(fact.id());
        self.type_sizes.remove(fact.id());
        self.constants.remove(fact.id());
        self
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
