//! Content: items, recipes, workshops, archives, resources, texts, rates,
//! templates and what the mod serves.

use enshwick_core::__fake::IMAGE_FIELDS;
use enshwick_core::__fake::ids;
use enshwick_core::abi::Header;
use enshwick_core::abi::ItemData;
use enshwick_core::abi::ResourceKey;
use enshwick_core::abi::Status;
use enshwick_core::content::Guid;

use crate::test::{FakeHost, Installed, table};

pub(crate) mod rates;
#[cfg(feature = "typed")]
pub(crate) mod removal;
pub(crate) mod templates;

/// The facts the builder holds, so a test lays out its blobs by the same
/// layouts the mod resolves.
///
/// A preset's values claim nothing about a game build; they only agree with
/// each other, so a test builds its blobs from the layout the preset gives
/// rather than from offsets of its own.
impl enshwick_core::content::EngineFacts for FakeHost {
    fn offset(&self, id: &str) -> Result<usize, enshwick_core::Off> {
        self.constants
            .get(id)
            .and_then(|&offset| usize::try_from(offset).ok())
            .ok_or_else(|| enshwick_core::Off::new(format!("offset {id}: not set on the fake")))
    }

    fn size(&self, id: &str) -> Result<usize, enshwick_core::Off> {
        self.type_sizes
            .get(id)
            .and_then(|&size| usize::try_from(size).ok())
            .ok_or_else(|| enshwick_core::Off::new(format!("type {id}: no size set on the fake")))
    }

    fn hash(&self, id: &str) -> Result<u32, enshwick_core::Off> {
        self.type_hashes
            .get(id)
            .copied()
            .ok_or_else(|| enshwick_core::Off::new(format!("type {id}: no hash set on the fake")))
    }
}

impl FakeHost {
    /// Items and the item registry: what
    /// [`ItemLayout`](enshwick_core::content::items::ItemLayout) resolves, with the
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
    /// [`RecipeLayout`](enshwick_core::content::recipes::RecipeLayout) resolves, with
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
    /// [`WorkshopLayout`](enshwick_core::content::workshops::WorkshopLayout) resolves
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
                (ids::CRAFTING_INTERACTION, 0x20),
                (ids::CLIENT_CRAFTING_INTERACTION, 0x10),
                (ids::WORKSHOP, 0x40),
            ],
            &[
                (ids::TEMPLATE_COMPONENTS, 0x10),
                (ids::TEMPLATE_COLLECTION_TEMPLATES, 0),
                (ids::CRAFTING_WORKSHOP, 0),
                (ids::CRAFTING_WORKSHOP_ID, 0x10),
                (ids::CRAFTING_INTERACTION_WORKSHOP, 12),
                (ids::CRAFTING_INTERACTION_WORKSHOP_ID, 28),
                (ids::CLIENT_CRAFTING_INTERACTION_WORKSHOP_ID, 12),
                (ids::WORKSHOP_REGISTRY_WORKSHOPS, 0),
                (ids::WORKSHOP_GUID, 0),
                (ids::WORKSHOP_ID, 0x10),
                (ids::WORKSHOP_NAME, 0x14),
                (ids::WORKSHOP_ITEM, 0x24),
                (ids::WORKSHOP_ITEM_REF, 0x34),
                (ids::WORKSHOP_NPC, 0x3c),
            ],
        )
        .templates()
    }

    /// The client's crafting menu in its UI bundle: what
    /// [`MenuLayout`](enshwick_core::content::menu::MenuLayout) resolves, with the
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
    /// [`ImageLayout`](enshwick_core::content::icons::ImageLayout) resolves, with
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

    /// The static data of item `item`, for `item_info`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ItemData;
    ///
    /// let loader = enshwick::test::FakeHost::new().item(enshwick::ItemId::new(0x5e01), 1, ItemData::HAS_PIDE).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn item(mut self, item: enshwick_core::ItemId, max_stack: u16, flags: u32) -> Self {
        self.items.insert(
            item.get(),
            ItemData {
                header: Header::of::<ItemData>(),
                id: item,
                max_stack,
                category: 0,
                rarity: 0,
                flags,
                placed_entity: [0; 16],
            },
        );
        self
    }

    /// Puts item `item`, set with [`FakeHost::item`], in the game's
    /// `ItemCategory` numbered `category`.
    ///
    /// # Examples
    ///
    /// ```
    /// let host = enshwick::test::FakeHost::new().item(enshwick::ItemId::new(0x5e01), 1, 0).item_category(enshwick::ItemId::new(0x5e01), 1);
    /// let _loader = host.install();
    /// ```
    #[must_use]
    pub fn item_category(mut self, item: enshwick_core::ItemId, category: u8) -> Self {
        if let Some(data) = self.items.get_mut(&item.get()) {
            data.category = category;
        }
        self
    }

    /// Makes item `item`, set with [`FakeHost::item`], one a player places
    /// as the template `template`, by its GUID in file order.
    ///
    /// # Examples
    ///
    /// ```
    /// let host = enshwick::test::FakeHost::new().item(enshwick::ItemId::new(0x5e01), 1, 0).item_places(enshwick::ItemId::new(0x5e01), [0x3a; 16]);
    /// let _loader = host.install();
    /// ```
    #[must_use]
    pub fn item_places(mut self, item: enshwick_core::ItemId, template: [u8; 16]) -> Self {
        if let Some(data) = self.items.get_mut(&item.get()) {
            data.placed_entity = template;
        }
        self
    }

    /// Plays a client that shows served UI images: `icons_ready` answers
    /// [`Status::OK`].
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().icons_ready().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn icons_ready(mut self) -> Self {
        self.icons_ready = true;
        self
    }

    /// Puts content `bytes` under `hash` in the game's archive, for
    /// `content_read`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().archive([4, 1, 2, 3], vec![1, 2, 3, 4]).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn archive(mut self, hash: [u32; 4], bytes: Vec<u8>) -> Self {
        self.archive.insert(hash, bytes);
        self
    }

    /// Puts resource `bytes` under `key` in the game's archive, for
    /// `resource_read`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    ///
    /// let key = Guid::parse("2f04a05f-697d-489b-a32f-76bb3439f5bc").key_by_hash(1);
    /// let loader = enshwick::test::FakeHost::new().resource(key, vec![1, 2, 3]).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn resource(mut self, key: ResourceKey, bytes: Vec<u8>) -> Self {
        self.resources
            .insert((key.guid, key.type_hash, key.part), bytes);
        self
    }

    /// Gives the game's archive the version string `version`, for
    /// `Host::game_version`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().game_version("1|demo|2026").install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn game_version(mut self, version: &'static str) -> Self {
        self.game_version = Some(version);
        self
    }

    /// Gives the game the reflected `types`, for
    /// [`Registry::game`](enshwick_core::resources::Registry::game), beside those of
    /// earlier calls and presets; a type named again replaces the earlier one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Type};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .types([Type::new("demo::uint32", Kind::Uint32, 4, 4)])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn types(
        mut self,
        types: impl IntoIterator<Item = enshwick_core::resources::Type<'static>>,
    ) -> Self {
        for ty in types {
            self.type_list
                .retain(|held| held.qualified_name() != ty.qualified_name());
            self.type_list.push(ty);
        }
        self.types = Some(std::sync::Arc::new(
            enshwick_core::resources::TypeTable::new(&self.type_list),
        ));
        self
    }

    /// Lists `named` in the game's item registry by their debug names, laid
    /// out by the host's item layout, so `content::vanilla::item` finds each
    /// item by its name in a mod the host runs. It needs
    /// [`FakeHost::items`] and [`FakeHost::workshops`] first, which `Vanilla`
    /// resolves; without the item layout it serves nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    ///
    /// let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    /// let loader = enshwick::test::FakeHost::new()
    ///     .items()
    ///     .workshops()
    ///     .vanilla_items(&[(chest, "Prop_Storage_32_T2_Chest_Wood")])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn vanilla_items(self, named: &[(Guid, &str)]) -> Self {
        use enshwick_core::content::items::{ITEM_REGISTRY, ItemLayout};
        use enshwick_core::content::{Blob, EngineFacts, types};

        let (Ok(layout), Ok(hash)) = (
            ItemLayout::resolve(&self),
            self.hash(types::ITEM_REGISTRY_RESOURCE.id()),
        ) else {
            return self;
        };
        let Ok(count) = u32::try_from(named.len()) else {
            return self;
        };
        let mut blob = Blob::new(&vec![0; layout.refs.max(layout.names) + 8]);
        let guids: Vec<u8> = named.iter().flat_map(|(guid, _)| guid.0).collect();
        let refs = blob.append(&guids, 4);
        let slots = blob.append(&vec![0; named.len() * 8], 4);
        let mut laid =
            blob.set_array(layout.refs, refs, count) && blob.set_array(layout.names, slots, count);
        for (i, (_, name)) in named.iter().enumerate() {
            let text = blob.append(name.as_bytes(), 1);
            laid &=
                u32::try_from(name.len()).is_ok_and(|len| blob.set_array(slots + i * 8, text, len));
        }
        if !laid {
            return self;
        }
        self.resource(ITEM_REGISTRY.key_by_hash(hash), blob.into_bytes())
    }

    /// Gives each vanilla item of `tags` the tag of its name, as the game's
    /// item infos carry it: for
    /// [`vanilla_name_tag`](enshwick_core::content::items::vanilla_name_tag) and
    /// [`vanilla::item_by_display_name`](enshwick_core::content::vanilla::item_by_display_name),
    /// which read the texts [`game_text`](Self::game_text) gives under the
    /// tag's hash. Call [`items`](Self::items) first: it gives the layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::i18n::Language;
    ///
    /// let (glider, name) = (Guid([7; 16]), Guid([9; 16]));
    /// let loader = enshwick::test::FakeHost::new()
    ///     .items()
    ///     .vanilla_items(&[(glider, "Tool_Glider_T1")])
    ///     .vanilla_item_names(&[(glider, name)])
    ///     .game_text(name.fnv1a(), Language::EN_US, "Glider")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn vanilla_item_names(self, tags: &[(Guid, Guid)]) -> Self {
        use enshwick_core::__fake::ids;
        use enshwick_core::content::EngineFacts;

        let (Ok(at), Ok(hash)) = (self.offset(ids::ITEM_INFO_NAME), self.hash(ids::ITEM_INFO))
        else {
            return self;
        };
        tags.iter().fold(self, |host, (item, tag)| {
            let mut info = vec![0; at + 16];
            info[at..].copy_from_slice(&tag.0);
            host.resource(item.key_by_hash(hash), info)
        })
    }

    /// Gives the game the text `text` for tag `id` in `language`, for
    /// `game_text`; a tag without a text in a language falls back to
    /// [`Language::EN_US`](enshwick_core::i18n::Language::EN_US) as the game's does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Language;
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .game_text(7, Language::UK_UA, "Бальтазар")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn game_text(
        mut self,
        id: u32,
        language: enshwick_core::i18n::Language,
        text: &'static str,
    ) -> Self {
        self.game_texts.insert((id, language.0), text);
        self
    }

    /// Plays a game whose recipe `recipe` is disabled, as
    /// [`recipes::is_disabled`](enshwick_core::content::recipes::is_disabled) asks.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    ///
    /// let torch = Guid::parse("7fa62007-a38e-4790-8591-873055fc3f34");
    /// let loader = enshwick::test::FakeHost::new().disabled_recipe(torch).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn disabled_recipe(mut self, recipe: Guid) -> Self {
        self.disabled_recipes.insert(recipe.fnv1a());
        self
    }

    /// Plays a game that has loaded the item `item` as one of `kind`, as
    /// [`RecipeView::kind`](enshwick_core::content::recipes::RecipeView::kind) asks;
    /// every other item reads as not loaded yet.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::recipes::RecipeKind;
    ///
    /// let plank = Guid::parse("0b6c5a52-1df4-4a3b-bd0e-6c1f7e2a9c44");
    /// let loader = enshwick::test::FakeHost::new()
    ///     .recipe_kind(plank, RecipeKind::BuildingBlock)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn recipe_kind(
        mut self,
        item: Guid,
        kind: enshwick_core::content::recipes::RecipeKind,
    ) -> Self {
        self.recipe_kinds.insert(item.fnv1a(), kind.into());
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The resources the mod serves, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.resources().is_empty());
    /// ```
    #[must_use]
    pub fn resources(&self) -> Vec<ServedResource> {
        table::with(Vec::new(), |s| {
            let mut keys: Vec<ServedResource> = Vec::new();
            for served in &s.served {
                if !keys.iter().any(|k| k.key == served.0.key) {
                    keys.push(served.0);
                }
            }
            keys
        })
    }

    /// The changes of vanilla values the mod declared, in order, each with
    /// its source.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.changes().is_empty());
    /// ```
    #[must_use]
    pub fn changes(&self) -> Vec<(enshwick_core::content::changes::Change, String)> {
        table::with(Vec::new(), |s| s.changes.one.clone())
    }

    /// The changes of every item a selection picks the mod declared, in
    /// order, each with its source.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.item_changes().is_empty());
    /// ```
    #[must_use]
    pub fn item_changes(&self) -> Vec<(enshwick_core::content::changes::ItemsChange, String)> {
        table::with(Vec::new(), |s| s.changes.items.clone())
    }

    /// The changes of every recipe a selection picks the mod declared, in
    /// order, each with its source.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.recipe_changes().is_empty());
    /// ```
    #[must_use]
    pub fn recipe_changes(&self) -> Vec<(enshwick_core::content::changes::RecipesChange, String)> {
        table::with(Vec::new(), |s| s.changes.recipes.clone())
    }

    /// The changes the mod declared to set later, in order, each with its
    /// value in force: `None` until set or after a reset.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.live_changes().is_empty());
    /// ```
    #[must_use]
    pub fn live_changes(&self) -> Vec<LiveChange> {
        table::with(Vec::new(), |s| s.changes.live.clone())
    }

    /// The mod's recipe rules in force, from
    /// [`recipes::disable`](enshwick_core::content::recipes::disable) less those
    /// [`recipes::enable`](enshwick_core::content::recipes::enable) took back: kind
    /// and name.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.recipe_rules().is_empty());
    /// ```
    #[must_use]
    pub fn recipe_rules(&self) -> Vec<(enshwick_core::abi::RecipeRuleKind, String)> {
        table::with(Vec::new(), |s| s.recipe_rules.clone())
    }

    /// The served resources the mod asked to rebuild, in order: each
    /// [`Load::allow_rebuild`](enshwick_core::Load::allow_rebuild) at init and each
    /// [`rebuild`](enshwick_core::content::rebuild) after.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.rebuilds().is_empty());
    /// ```
    #[must_use]
    pub fn rebuilds(&self) -> Vec<ResourceKey> {
        table::with(Vec::new(), |s| s.rebuilds.clone())
    }

    /// How many patch outputs the fake's builds got as bytes of the patch's
    /// own, which stay for the process, rather than in a buffer it lent
    /// ([`HostApi::resource_buffer`](enshwick_core::abi::HostApi::resource_buffer)).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.kept_outputs(), 0);
    /// ```
    #[must_use]
    pub fn kept_outputs(&self) -> usize {
        table::with(0, |s| s.kept_outputs)
    }

    /// Builds the served resource `key` from `base` with the mod's patches, in order, as
    /// the game's first read of it would.
    ///
    /// # Errors
    ///
    /// [`Status::MISSING`] when the mod serves no `key`, or the patch's own
    /// status when it fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ResourceKey, Status};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let key = ResourceKey { guid: [0x3a; 16], type_hash: 0xb5ce_8765, part: 0 };
    /// assert_eq!(loader.build_resource(&key, &[]), Err(Status::MISSING));
    /// ```
    pub fn build_resource(&self, key: &ResourceKey, base: &[u8]) -> Result<Vec<u8>, Status> {
        table::content::build_resource(key, base)
    }

    /// The texts the mod added, as `(tag, language, text)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.texts().is_empty());
    /// ```
    #[must_use]
    pub fn texts(&self) -> Vec<(u32, u32, String)> {
        table::with(Vec::new(), |s| s.texts.clone())
    }

    /// The item data kinds the mod opened, by full id, and every value set:
    /// `(kind id, entity, item, bytes)`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.item_data().is_empty());
    /// ```
    #[must_use]
    pub fn item_data(&self) -> Vec<(String, u32, u32, Vec<u8>)> {
        table::with(Vec::new(), |s| {
            s.item_values
                .iter()
                .map(|(&(kind, pide), (item, bytes))| {
                    let id = s.item_kinds.get(kind as usize).cloned().unwrap_or_default();
                    (id, pide, *item, bytes.clone())
                })
                .collect()
        })
    }

    /// The content the mod served, by hash.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.served_content().is_empty());
    /// ```
    #[must_use]
    pub fn served_content(&self) -> Vec<([u32; 4], Vec<u8>)> {
        table::with(Vec::new(), |s| {
            s.content.iter().map(|(h, b)| (*h, b.clone())).collect()
        })
    }

    /// The texts the mod handed the loader last, as TOML; `None` before it
    /// read any.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.published_texts(), None);
    /// ```
    #[must_use]
    pub fn published_texts(&self) -> Option<String> {
        table::with(None, |s| s.published_texts.clone())
    }

    /// The services the mod published, as `(name, major)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.published().is_empty());
    /// ```
    #[must_use]
    pub fn published(&self) -> Vec<(String, u32)> {
        table::with(Vec::new(), |s| {
            s.published
                .iter()
                .map(|p| (p.name.clone(), p.major))
                .collect()
        })
    }

    /// The content ids the mod declared, `<mod>:<name>`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.content_ids().is_empty());
    /// ```
    #[must_use]
    pub fn content_ids(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.content_ids.clone())
    }

    /// The content renames the mod declared, old GUID to new, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.content_renames().is_empty());
    /// ```
    #[must_use]
    pub fn content_renames(&self) -> Vec<(Guid, Guid)> {
        table::with(None, |s| s.content_renames.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|(from, to)| (Guid(from), Guid(to)))
            .collect()
    }
}

/// A resource the mod serves, as the loader received it.
///
/// # Examples
///
/// See [`Installed::resources`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServedResource {
    /// The resource served.
    pub key: ResourceKey,
    /// The resource it is built from.
    pub base: ResourceKey,
}

/// A change a mod declared to set later, as the fake loader keeps it.
///
/// # Examples
///
/// See [`Installed::live_changes`](crate::test::Installed::live_changes).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct LiveChange {
    /// What it reaches.
    pub target: enshwick_core::content::changes::Target,
    /// What it changes.
    pub field: enshwick_core::content::changes::Field,
    /// Its source.
    pub source: String,
    /// The value in force; `None` until set or after a reset.
    pub value: Option<enshwick_core::content::changes::Value>,
}
