//! Every type of a game build, looked up by any of its names or hashes.

use rustc_hash::FxHashMap;

use super::{Field, Type};

/// Every type of a game build, looked up by any of its names or hashes:
/// the type registry the codec reads values with.
///
/// [`Registry::game`] holds the running game's; tests and tools build one
/// from their own types.
///
/// # Examples
///
/// ```
/// use enshwick::resources::{Kind, Registry, Type, fnv1a32};
///
/// let registry = Registry::new([Type::new("demo::uint32", Kind::Uint32, 4, 4)]);
/// assert!(registry.named("demo::uint32").is_some());
/// assert!(registry.hashed(fnv1a32(b"demo::uint32")).is_some());
/// assert!(registry.impact_named("uint32").is_some());
/// assert_eq!(registry.len(), 1);
/// ```
#[derive(Debug, Default)]
pub struct Registry<'a> {
    types: Vec<Type<'a>>,
    by_name: FxHashMap<&'a str, usize>,
    by_hash: FxHashMap<u32, usize>,
    by_impact: FxHashMap<&'a str, usize>,
    by_impact_hash: FxHashMap<u32, usize>,
}

impl<'a> Registry<'a> {
    /// A registry of `types`; of two types with one qualified name the first
    /// stays.
    ///
    /// # Examples
    ///
    /// See [`Registry`].
    pub fn new(types: impl IntoIterator<Item = Type<'a>>) -> Self {
        let mut registry = Self::default();
        for ty in types {
            if registry.by_name.contains_key(ty.qualified_name()) {
                continue;
            }
            let index = registry.types.len();
            registry.by_name.insert(ty.qualified_name(), index);
            registry.by_hash.entry(ty.qualified_hash()).or_insert(index);
            registry.by_impact.entry(ty.impact_name()).or_insert(index);
            registry
                .by_impact_hash
                .entry(ty.impact_hash())
                .or_insert(index);
            registry.types.push(ty);
        }
        registry
    }

    /// The type of qualified name `name`, such as `demo::BalancingTable`.
    ///
    /// # Examples
    ///
    /// See [`Registry`].
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Type<'a>> {
        self.types.get(*self.by_name.get(name)?)
    }

    /// The type whose qualified name hashes to `hash`, as resource keys and
    /// variants name types.
    ///
    /// # Examples
    ///
    /// See [`Registry`].
    #[must_use]
    pub fn hashed(&self, hash: u32) -> Option<&Type<'a>> {
        self.types.get(*self.by_hash.get(&hash)?)
    }

    /// The type of impact name `name`, such as `ecs.TemplateResource`.
    ///
    /// # Examples
    ///
    /// See [`Registry`].
    #[must_use]
    pub fn impact_named(&self, name: &str) -> Option<&Type<'a>> {
        self.types.get(*self.by_impact.get(name)?)
    }

    /// The type whose impact name hashes to `hash`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Registry, Type, fnv1a32};
    ///
    /// let registry = Registry::new([Type::new("demo::uint32", Kind::Uint32, 4, 4)]);
    /// assert!(registry.impact_hashed(fnv1a32(b"uint32")).is_some());
    /// ```
    #[must_use]
    pub fn impact_hashed(&self, hash: u32) -> Option<&Type<'a>> {
        self.types.get(*self.by_impact_hash.get(&hash)?)
    }

    /// Every type, in the order the registry was built from.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Registry, Type};
    ///
    /// let registry = Registry::new([Type::new("demo::uint8", Kind::Uint8, 1, 1)]);
    /// assert_eq!(registry.iter().next().map(|ty| ty.name()), Some("uint8"));
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &Type<'a>> {
        self.types.iter()
    }

    /// How many types the registry holds.
    ///
    /// # Examples
    ///
    /// See [`Registry`].
    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Whether the registry holds no type.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(enshwick::resources::Registry::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// The fields of struct `ty` and of its bases, the base's first, with
    /// each field's offset from the start of `ty`.
    pub(crate) fn all_fields<'r>(&'r self, ty: &'r Type<'a>) -> Vec<&'r Field<'a>> {
        let mut chain = vec![ty];
        let mut current = ty;
        while let Some(base) = current.base().and_then(|name| self.named(name)) {
            if chain.len() >= MAX_BASES || chain.iter().any(|seen| core::ptr::eq(*seen, base)) {
                break;
            }
            chain.push(base);
            current = base;
        }
        chain
            .iter()
            .rev()
            .flat_map(|ty| ty.fields().iter())
            .collect()
    }

    /// Every struct derived from `base`, directly or through other bases; a
    /// chain deeper than the bases [`all_fields`](Self::all_fields) follows
    /// is taken for a loop and does not count.
    pub(crate) fn derived<'r>(&'r self, base: &'r Type<'a>) -> impl Iterator<Item = &'r Type<'a>> {
        self.types.iter().filter(move |ty| {
            let mut current = *ty;
            for _ in 0..MAX_BASES {
                match current.base().and_then(|name| self.named(name)) {
                    Some(next) if core::ptr::eq(next, base) => return true,
                    Some(next) => current = next,
                    None => return false,
                }
            }
            false
        })
    }
}

/// Bases deeper than this are taken for a loop in bad data.
const MAX_BASES: usize = 32;
