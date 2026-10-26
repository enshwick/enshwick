//! Content: items, archives, resources, texts and what the mod serves.

use crate::abi::Header;
use crate::abi::ItemData;
use crate::abi::ResourceKey;
use crate::abi::Status;

use super::FakeHost;
use super::Installed;
use super::table;

impl FakeHost {
    /// The static data of item `item`, for `item_info`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ItemData;
    ///
    /// let loader = enshwick::test::FakeHost::new().item(0x5e01, 1, ItemData::HAS_PIDE).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn item(mut self, item: u32, max_stack: u16, flags: u32) -> Self {
        self.items.insert(
            item,
            ItemData {
                header: Header::of::<ItemData>(),
                id: item,
                max_stack,
                category: 0,
                rarity: 0,
                flags,
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
    /// let host = enshwick::test::FakeHost::new().item(0x5e01, 1, 0).item_category(0x5e01, 1);
    /// let _loader = host.install();
    /// ```
    #[must_use]
    pub fn item_category(mut self, item: u32, category: u8) -> Self {
        if let Some(data) = self.items.get_mut(&item) {
            data.category = category;
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

    /// Gives the game the text `text` for tag `id` in `language`, for
    /// `game_text`; a tag without a text in a language falls back to
    /// [`Language::EN_US`](crate::i18n::Language::EN_US) as the game's does.
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
        language: crate::i18n::Language,
        text: &'static str,
    ) -> Self {
        self.game_texts.insert((id, language.0), text);
        self
    }

    /// Plays a game whose recipe `recipe` is disabled, as
    /// [`recipes::is_disabled`](crate::content::recipes::is_disabled) asks.
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
    pub fn disabled_recipe(mut self, recipe: crate::content::Guid) -> Self {
        self.disabled_recipes.insert(recipe.id());
        self
    }

    /// Plays a game that has loaded the item `item` as one of `kind`, as
    /// [`RecipeView::kind`](crate::content::recipes::RecipeView::kind) asks;
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
        item: crate::content::Guid,
        kind: crate::content::recipes::RecipeKind,
    ) -> Self {
        self.recipe_kinds.insert(item.id(), kind.into());
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
    pub fn changes(&self) -> Vec<(crate::content::changes::Change, String)> {
        table::with(Vec::new(), |s| s.changes.clone())
    }

    /// The mod's recipe rules in force, from
    /// [`recipes::disable`](crate::content::recipes::disable) less those
    /// [`recipes::enable`](crate::content::recipes::enable) took back: kind
    /// and name.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.recipe_rules().is_empty());
    /// ```
    #[must_use]
    pub fn recipe_rules(&self) -> Vec<(crate::abi::RecipeRuleKind, String)> {
        table::with(Vec::new(), |s| s.recipe_rules.clone())
    }

    /// The served resources the mod asked to rebuild, in order: each
    /// [`Load::allow_rebuild`](crate::Load::allow_rebuild) at init and each
    /// [`rebuild`](crate::content::rebuild) after.
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
        let patches: Vec<_> = table::with(Vec::new(), |s| {
            s.served
                .iter()
                .filter(|s| s.0.key == *key)
                .map(|s| (s.1, s.2))
                .collect()
        });
        if patches.is_empty() {
            return Err(Status::MISSING);
        }
        let mut bytes = base.to_vec();
        for (patch, user) in patches {
            let mut out = core::ptr::null();
            let mut out_len = 0;
            // SAFETY: `bytes` is readable for the call, the outs are valid,
            // and `user` is the pointer the mod served with.
            let status = unsafe {
                patch(
                    bytes.as_ptr(),
                    bytes.len(),
                    core::ptr::with_exposed_provenance_mut(user),
                    &raw mut out,
                    &raw mut out_len,
                )
            };
            if !status.is_ok() {
                return Err(status);
            }
            bytes = if out.is_null() {
                Vec::new()
            } else {
                // SAFETY: on success the patch wrote bytes that live for the
                // process.
                unsafe { core::slice::from_raw_parts(out, out_len) }.to_vec()
            };
        }
        Ok(bytes)
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
