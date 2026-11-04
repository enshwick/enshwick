//! The type registry across the ABI: the entries the loader lists, and the
//! types the safe layer reads back from them.

use super::{Attribute, Constant, Field, Kind, Registry, Type};
use crate::abi::{Status, Str, TypeAttribute, TypeConstant, TypeEntry, TypeField};
use crate::{Host, Off};

/// Every type as ABI entries, built once and kept for the process, for the
/// loader's and the fake host's [`HostApi::types`](crate::abi::HostApi::types).
#[doc(hidden)]
#[derive(Debug)]
pub struct TypeTable(&'static [TypeEntry]);

// SAFETY: the entries and every string and array they point at are leaked by
// `TypeTable::new` and never written again, so any thread may read them.
unsafe impl Send for TypeTable {}
// SAFETY: `TypeTable::new` leaks the entries and every string and array they
// point at and never writes them again, so shared reads see fixed bytes.
unsafe impl Sync for TypeTable {}

impl TypeTable {
    /// The entries of `types`, kept for the process.
    #[must_use]
    pub fn new<'t>(types: impl IntoIterator<Item = &'t Type<'static>>) -> Self {
        let entries: Vec<TypeEntry> = types.into_iter().map(entry).collect();
        Self(Box::leak(entries.into_boxed_slice()))
    }

    /// The entries, valid for the process.
    #[must_use]
    pub const fn entries(&self) -> &'static [TypeEntry] {
        self.0
    }
}

/// `items` leaked for the process, as an array pointer and its length; null
/// for none.
fn leak<T>(items: Vec<T>) -> (*const T, u32) {
    if items.is_empty() {
        return (core::ptr::null(), 0);
    }
    let len = u32::try_from(items.len()).unwrap_or(u32::MAX);
    (Box::leak(items.into_boxed_slice()).as_ptr(), len)
}

fn attributes(list: &[Attribute<'static>]) -> (*const TypeAttribute, u32) {
    leak(
        list.iter()
            .map(|a| TypeAttribute {
                name: Str::new(a.name()),
                value_type: Str::new(a.value_type().unwrap_or("")),
                value: Str::new(a.value()),
            })
            .collect(),
    )
}

fn entry(ty: &Type<'static>) -> TypeEntry {
    let (fields, field_count) = leak(
        ty.fields()
            .iter()
            .map(|f| {
                let (attributes, attribute_count) = attributes(f.attributes());
                TypeField {
                    name: Str::new(f.name()),
                    type_name: Str::new(f.type_name().unwrap_or("")),
                    offset: f.offset(),
                    attribute_count,
                    attributes,
                }
            })
            .collect(),
    );
    let (constants, constant_count) = leak(
        ty.constants()
            .iter()
            .map(|c| TypeConstant {
                name: Str::new(c.name()),
                value: c.value(),
            })
            .collect(),
    );
    let (attributes, attribute_count) = attributes(ty.attributes());
    TypeEntry {
        name: Str::new(ty.name()),
        impact_name: Str::new(ty.impact_name()),
        qualified_name: Str::new(ty.qualified_name()),
        inner: Str::new(ty.base().or_else(|| ty.inner()).unwrap_or("")),
        internal_hash: ty.internal_hash(),
        size: ty.size(),
        align: ty.align(),
        element_align: ty.element_align(),
        kind: ty.kind().byte(),
        flags: ty.flags(),
        count: ty.field_count(),
        field_count,
        constant_count,
        attribute_count,
        fields,
        constants,
        attributes,
        ..TypeEntry::EMPTY
    }
}

/// Proof that the strings and arrays read through it are the loader's,
/// valid and unchanged for the life of the process, as
/// [`HostApi::types`](crate::abi::HostApi::types) promises of its entries.
#[derive(Clone, Copy)]
struct Kept(());

impl Kept {
    /// # Safety
    ///
    /// Every string and array read through the token is valid and unchanged
    /// for the life of the process.
    const unsafe fn new() -> Self {
        Self(())
    }

    /// The text of `text`, empty when it is not UTF-8.
    #[expect(
        clippy::unused_self,
        reason = "the token is the proof the read is sound"
    )]
    fn text(self, text: Str) -> &'static str {
        // SAFETY: `Kept::new`'s caller guarantees every string read through
        // the token valid and unchanged for the life of the process.
        unsafe { text.as_str() }.unwrap_or("")
    }

    /// The `len` items at `items`; none for null.
    #[expect(
        clippy::unused_self,
        reason = "the token is the proof the read is sound"
    )]
    fn array<T>(self, items: *const T, len: u32) -> &'static [T] {
        if items.is_null() || len == 0 {
            return &[];
        }
        // SAFETY: `items` is not null, and `Kept::new`'s caller guarantees
        // every array read through the token valid for its `len` items for
        // the life of the process.
        unsafe { core::slice::from_raw_parts(items, len as usize) }
    }

    fn attributes(self, items: *const TypeAttribute, len: u32) -> Vec<Attribute<'static>> {
        self.array(items, len)
            .iter()
            .map(|a| {
                let attribute = Attribute::new(self.text(a.name), self.text(a.value));
                match self.text(a.value_type) {
                    "" => attribute,
                    value_type => attribute.with_value_type(value_type),
                }
            })
            .collect()
    }

    /// The type an entry describes.
    fn entry(self, entry: &TypeEntry) -> Type<'static> {
        let kind = Kind::from_byte(entry.kind);
        let mut ty = Type::new(
            self.text(entry.qualified_name),
            kind,
            entry.size,
            entry.align,
        )
        .with_names(self.text(entry.name), self.text(entry.impact_name))
        .with_layout(entry.element_align, entry.flags, entry.internal_hash)
        .with_attributes(self.attributes(entry.attributes, entry.attribute_count))
        .with_count(entry.count);
        let inner = self.text(entry.inner);
        if !inner.is_empty() {
            ty = ty.with_inner(inner);
        }
        if kind == Kind::Struct {
            ty = ty.with_fields(
                self.array(entry.fields, entry.field_count)
                    .iter()
                    .map(|f| {
                        let field = match self.text(f.type_name) {
                            "" => Field::new(self.text(f.name), f.offset, "").untyped(),
                            type_name => Field::new(self.text(f.name), f.offset, type_name),
                        };
                        field.with_attributes(self.attributes(f.attributes, f.attribute_count))
                    })
                    .collect(),
            );
        }
        if kind == Kind::Enum {
            ty = ty.with_constants(
                self.array(entry.constants, entry.constant_count)
                    .iter()
                    .map(|c| Constant::new(self.text(c.name), c.value))
                    .collect(),
            );
        }
        ty
    }
}

/// The running game's types, kept per mod life after the first call.
static GAME: crate::fresh::Fresh<Registry<'static>> = crate::fresh::Fresh::new();

impl Registry<'static> {
    /// The running game's type registry: every reflected type, read from the
    /// loader at the first call and kept.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game and when the game holds no reflection data.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::Registry;
    ///
    /// if let Ok(registry) = Registry::game() {
    ///     let table = registry.named("demo::BalancingTable");
    ///     assert!(table.is_none_or(|ty| ty.size() > 0));
    /// }
    /// ```
    pub fn game() -> Result<&'static Self, Off> {
        if let Some(registry) = GAME.get() {
            return Ok(registry);
        }
        let host = crate::host().ok_or_else(|| Off::new("types: outside the game"))?;
        let entries = host.type_entries()?;
        // SAFETY: `HostApi::types` promises every string and array of its
        // entries valid and unchanged for the life of the process.
        let kept = unsafe { Kept::new() };
        let types: Vec<Type<'static>> = entries.iter().map(|e| kept.entry(e)).collect();
        Ok(GAME.get_or_init(|| Self::new(types)))
    }
}

impl Host {
    /// Every entry of [`HostApi::types`](crate::abi::HostApi::types).
    fn type_entries(&self) -> Result<Vec<TypeEntry>, Off> {
        let api = self.api();
        let size =
            u32::try_from(size_of::<TypeEntry>()).map_err(|_| Off::new("types: entry size"))?;
        let mut count = 0u32;
        let status = (api.types)(
            self.handle(),
            core::ptr::null_mut(),
            size,
            0,
            &raw mut count,
        );
        crate::load::ok(status, (), || "types".to_owned())?;
        let mut entries = vec![TypeEntry::EMPTY; count as usize];
        let status = (api.types)(
            self.handle(),
            entries.as_mut_ptr(),
            size,
            count,
            &raw mut count,
        );
        crate::load::ok(status, (), || "types".to_owned())?;
        entries.truncate(count as usize);
        Ok(entries)
    }

    /// The keys of the game's own resources of type `ty`, or of every
    /// resource for `None`, in the order their bytes lie in the archive.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game's archive does not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::Registry;
    ///
    /// if let (Some(host), Ok(registry)) = (enshwick::host(), Registry::game()) {
    ///     if let Some(table) = registry.named("demo::BalancingTable") {
    ///         let keys = host.resource_keys(Some(table)).unwrap_or_default();
    ///         assert!(keys.iter().all(|key| key.type_hash == table.qualified_hash()));
    ///     }
    /// }
    /// ```
    pub fn resource_keys(
        &self,
        ty: Option<&Type<'_>>,
    ) -> Result<Vec<crate::abi::ResourceKey>, Off> {
        let api = self.api();
        let hash = ty.map_or(0, Type::qualified_hash);
        let mut count = 0u32;
        let status = (api.resource_keys)(
            self.handle(),
            hash,
            core::ptr::null_mut(),
            0,
            &raw mut count,
        );
        crate::load::ok(status, (), || "resource keys".to_owned())?;
        let empty = crate::abi::ResourceKey {
            guid: [0; 16],
            type_hash: 0,
            part: 0,
        };
        let mut keys = vec![empty; count as usize];
        let status = (api.resource_keys)(
            self.handle(),
            hash,
            keys.as_mut_ptr(),
            count,
            &raw mut count,
        );
        crate::load::ok(status, (), || "resource keys".to_owned())?;
        keys.truncate(count as usize);
        Ok(keys)
    }

    /// The hashes of the game's own contents, in the archive's order;
    /// [`Host::game_content`] reads each.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game's archive does not read.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick::host() {
    ///     let hashes = host.content_hashes().unwrap_or_default();
    ///     assert!(hashes.first().is_none_or(|hash| host.game_content(*hash).is_some()));
    /// }
    /// ```
    pub fn content_hashes(&self) -> Result<Vec<[u32; 4]>, Off> {
        let api = self.api();
        let mut count = 0u32;
        let status = (api.content_hashes)(self.handle(), core::ptr::null_mut(), 0, &raw mut count);
        crate::load::ok(status, (), || "content hashes".to_owned())?;
        let mut hashes = vec![[0u32; 4]; count as usize];
        let status =
            (api.content_hashes)(self.handle(), hashes.as_mut_ptr(), count, &raw mut count);
        crate::load::ok(status, (), || "content hashes".to_owned())?;
        hashes.truncate(count as usize);
        Ok(hashes)
    }

    /// The bytes of the game's own content `hash`, from its archive; `None`
    /// for a hash the archive lacks.
    ///
    /// # Examples
    ///
    /// See [`Host::content_hashes`].
    #[must_use]
    pub fn game_content(&self, hash: [u32; 4]) -> Option<Vec<u8>> {
        let api = self.api();
        let mut buffer = vec![0u8; usize::try_from(hash[0]).ok()?];
        let mut len = 0u32;
        let status = (api.content_read)(
            self.handle(),
            &raw const hash,
            buffer.as_mut_ptr(),
            u32::try_from(buffer.len()).ok()?,
            &raw mut len,
        );
        (status == Status::OK).then(|| {
            buffer.truncate(len as usize);
            buffer
        })
    }

    /// Serves `bytes` as game content, such as a texture's pixels, kept for
    /// the life of the process, and returns the hash a resource names it by;
    /// the same bytes give the same hash.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game and when the loader refuses the content.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick::host() {
    ///     let hash = host.serve_content(&[0, 0, 0, 255])?;
    ///     assert_eq!(hash[0], 4, "a hash starts with its size");
    /// }
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    pub fn serve_content(&self, bytes: &[u8]) -> Result<[u32; 4], Off> {
        let api = self.api();
        let len = u32::try_from(bytes.len()).map_err(|_| Off::new("content past 4 GiB"))?;
        let mut out = [0u32; 4];
        let status = (api.content_serve)(self.handle(), bytes.as_ptr(), len, &raw mut out);
        crate::load::ok(status, out, || "serve content".to_owned())
    }

    /// The value of the game's own resource `key`, read from its archive by
    /// the type its key names.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, for a key the archive lacks, and when the
    /// resource does not read as its type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::Registry;
    ///
    /// if let (Some(host), Ok(registry)) = (enshwick::host(), Registry::game()) {
    ///     if let Some(table) = registry.named("demo::BalancingTable") {
    ///         for key in host.resource_keys(Some(table)).unwrap_or_default() {
    ///             let _value = host.game_value(key);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn game_value(&self, key: crate::abi::ResourceKey) -> Result<super::Value, Off> {
        let registry = Registry::game()?;
        let ty = registry.hashed(key.type_hash).ok_or_else(|| {
            Off::new(format!(
                "resource type {:#010x} is not a type of this build",
                key.type_hash
            ))
        })?;
        let bytes = self.game_resource(key).ok_or_else(|| {
            Off::new(format!(
                "resource {}: not in the archive",
                crate::content::Guid(key.guid)
            ))
        })?;
        Ok(registry.decode(ty.qualified_name(), &bytes)?)
    }
}

/// Serves `value` as the new resource `key`, of the type its key names; the
/// loader builds it from any vanilla resource of that type, whose bytes the
/// value replaces.
pub(crate) fn serve_new(
    host: Host,
    key: crate::abi::ResourceKey,
    value: &super::Value,
) -> Result<(), Off> {
    let registry = Registry::game()?;
    let ty = registry.hashed(key.type_hash).ok_or_else(|| {
        Off::new(format!(
            "resource type {:#010x} is not a type of this build",
            key.type_hash
        ))
    })?;
    let name = ty.qualified_name();
    // The loader opens a base in the game's database; any resource of the
    // type serves, as the value's bytes replace it whole.
    let base = host
        .resource_keys(Some(ty))?
        .into_iter()
        .next()
        .ok_or_else(|| Off::new(format!("the game has no {name} to build a new one from")))?;
    let bytes = registry.encode(name, value, None)?;
    crate::content::serve(host, key, base, move |_| Some(bytes.clone()))
}

impl crate::Load<'_> {
    /// Serves `value` as the new resource `key`, of the type its key names,
    /// with no vanilla resource as its base: every field the value leaves
    /// out is zero. Declare the key's GUID with [`Load::declare`](crate::Load::declare).
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, for a type the build lacks or has no
    /// resource of, a value that does not fit the type, and when the loader
    /// refuses the serve, such as after init.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Id, id};
    /// use enshwick::resources::Value;
    ///
    /// const TABLE: Id = id!("balance");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let guid = load.declare(TABLE)?;
    ///     let value = Value::Struct(vec![("damage".into(), Value::Float(2.0))]);
    ///     load.serve_new(guid.key_by_hash(enshwick::resources::fnv1a32(b"demo::Table")), &value)
    /// }
    /// ```
    pub fn serve_new(
        &mut self,
        key: crate::abi::ResourceKey,
        value: &super::Value,
    ) -> Result<(), Off> {
        serve_new(self.host(), key, value)
    }
}

/// Declares the content id `id`, `<mod>:<name>`, for the mod `host` serves,
/// and returns its GUID.
pub(crate) fn declare(host: Host, id: &str) -> Result<crate::content::Guid, Off> {
    let mut out = [0u8; 16];
    let status = (host.api().declare_content)(host.handle(), Str::new(id), &raw mut out);
    crate::load::ok(status, crate::content::Guid(out), || {
        format!("content id {id}")
    })
}
