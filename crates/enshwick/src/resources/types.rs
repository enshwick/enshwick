//! The game's reflected types as data: what a resource's bytes mean.

use super::Kind;

/// An attribute the engine attaches to a type or a field: a name and its
/// text, such as a field's `tooltip`.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Attribute;
///
/// let tooltip = Attribute::new("tooltip", "Damage of a level 0 weapon");
/// assert_eq!(tooltip.name(), "tooltip");
/// assert_eq!(tooltip.value(), "Damage of a level 0 weapon");
/// assert_eq!(tooltip.value_type(), None);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribute<'a> {
    name: &'a str,
    value_type: Option<&'a str>,
    value: &'a str,
}

impl<'a> Attribute<'a> {
    /// The attribute `name` with the text `value` and no declared value
    /// type.
    ///
    /// # Examples
    ///
    /// See [`Attribute`].
    #[must_use]
    pub const fn new(name: &'a str, value: &'a str) -> Self {
        Self {
            name,
            value_type: None,
            value,
        }
    }

    /// The same attribute, its value declared of the type named
    /// `value_type`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::Attribute;
    ///
    /// let attribute = Attribute::new("converter", "parse").with_value_type("demo::string");
    /// assert_eq!(attribute.value_type(), Some("demo::string"));
    /// ```
    #[must_use]
    pub const fn with_value_type(mut self, value_type: &'a str) -> Self {
        self.value_type = Some(value_type);
        self
    }

    /// The attribute's name.
    ///
    /// # Examples
    ///
    /// See [`Attribute`].
    #[must_use]
    pub const fn name(&self) -> &'a str {
        self.name
    }

    /// The qualified name of the type the engine declares the value of;
    /// `None` for a flag without a value.
    ///
    /// # Examples
    ///
    /// See [`Attribute`].
    #[must_use]
    pub const fn value_type(&self) -> Option<&'a str> {
        self.value_type
    }

    /// The value as the engine's text; empty for a flag.
    ///
    /// # Examples
    ///
    /// See [`Attribute`].
    #[must_use]
    pub const fn value(&self) -> &'a str {
        self.value
    }
}

/// A field of a struct type.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Field;
///
/// let health = Field::new("health", 4, "demo::float32");
/// assert_eq!(health.name(), "health");
/// assert_eq!(health.offset(), 4);
/// assert_eq!(health.type_name(), Some("demo::float32"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field<'a> {
    name: &'a str,
    offset: u32,
    type_name: Option<&'a str>,
    attributes: Vec<Attribute<'a>>,
}

impl<'a> Field<'a> {
    /// The field `name` at `offset` from the start of its struct, of the
    /// type qualified as `type_name`.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub const fn new(name: &'a str, offset: u32, type_name: &'a str) -> Self {
        Self {
            name,
            offset,
            type_name: Some(type_name),
            attributes: Vec::new(),
        }
    }

    /// The same field without a reflected type: its bytes stay as they are.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::Field;
    ///
    /// assert_eq!(Field::new("pad", 0, "demo::uint8").untyped().type_name(), None);
    /// ```
    #[must_use]
    pub fn untyped(mut self) -> Self {
        self.type_name = None;
        self
    }

    /// The same field with `attributes`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Attribute, Field};
    ///
    /// let field = Field::new("damage", 0, "demo::float32")
    ///     .with_attributes(vec![Attribute::new("tooltip", "Damage")]);
    /// assert_eq!(field.attributes()[0].value(), "Damage");
    /// ```
    #[must_use]
    pub fn with_attributes(mut self, attributes: Vec<Attribute<'a>>) -> Self {
        self.attributes = attributes;
        self
    }

    /// The field's name.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub const fn name(&self) -> &'a str {
        self.name
    }

    /// The offset from the start of the struct.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    /// The qualified name of the field's type; `None` when the engine does
    /// not reflect it.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub const fn type_name(&self) -> Option<&'a str> {
        self.type_name
    }

    /// The field's attributes, in the engine's order.
    ///
    /// # Examples
    ///
    /// See [`Field::with_attributes`].
    #[must_use]
    pub fn attributes(&self) -> &[Attribute<'a>] {
        &self.attributes
    }
}

/// A named constant of an enum type; for the enum of a bitmask, the value
/// is the flag's bit index.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Constant;
///
/// let invalid = Constant::new("Invalid", 0);
/// assert_eq!((invalid.name(), invalid.value()), ("Invalid", 0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Constant<'a> {
    name: &'a str,
    value: u64,
}

impl<'a> Constant<'a> {
    /// The constant `name` of `value`.
    ///
    /// # Examples
    ///
    /// See [`Constant`].
    #[must_use]
    pub const fn new(name: &'a str, value: u64) -> Self {
        Self { name, value }
    }

    /// The constant's name, without its type.
    ///
    /// # Examples
    ///
    /// See [`Constant`].
    #[must_use]
    pub const fn name(&self) -> &'a str {
        self.name
    }

    /// Its value; the type's [`size`](Type::size) says how wide the engine
    /// stores it.
    ///
    /// # Examples
    ///
    /// See [`Constant`].
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }
}

/// A reflected type of the game: its names, hashes, layout, fields,
/// constants and attributes, as the engine describes it.
///
/// Built by the loader from the game's own reflection, and by tests with
/// [`Type::new`] and the `with_` methods.
///
/// # Examples
///
/// ```
/// use enshwick::resources::{Field, Kind, Type};
///
/// let health = Type::new("demo::ecs::Health", Kind::Struct, 8, 4)
///     .with_fields(vec![Field::new("value", 0, "demo::float32")]);
/// assert_eq!(health.name(), "Health");
/// assert_eq!(health.namespace(), "demo::ecs");
/// assert_eq!(health.qualified_hash(), enshwick::resources::fnv1a32(b"demo::ecs::Health"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type<'a> {
    name: &'a str,
    impact_name: &'a str,
    qualified_name: &'a str,
    inner: Option<&'a str>,
    internal_hash: u32,
    size: u32,
    align: u16,
    element_align: u16,
    kind: Kind,
    flags: u8,
    count: u32,
    fields: Vec<Field<'a>>,
    constants: Vec<Constant<'a>>,
    attributes: Vec<Attribute<'a>>,
}

impl<'a> Type<'a> {
    /// The type `qualified_name` of `kind`, `size` bytes aligned to `align`;
    /// its name and impact name are the qualified name's last part.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub fn new(qualified_name: &'a str, kind: Kind, size: u32, align: u16) -> Self {
        let name = last_part(qualified_name);
        Self {
            name,
            impact_name: name,
            qualified_name,
            inner: None,
            internal_hash: 0,
            size,
            align,
            element_align: align,
            kind,
            flags: 0,
            count: 0,
            fields: Vec::new(),
            constants: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// The same type under the short `name` and the `impact_name` the
    /// engine's scripts use, such as `ecs.TemplateResource`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Type};
    ///
    /// let ty = Type::new("demo::ecs::TemplateResource", Kind::Struct, 24, 4)
    ///     .with_names("TemplateResource", "ecs.TemplateResource");
    /// assert_eq!(ty.impact_name(), "ecs.TemplateResource");
    /// ```
    #[must_use]
    pub const fn with_names(mut self, name: &'a str, impact_name: &'a str) -> Self {
        self.name = name;
        self.impact_name = impact_name;
        self
    }

    /// The same type over `inner`: a struct's base, an array's or an
    /// optional's element, a typedef's target, a bitmask's flag enum.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Type};
    ///
    /// let ty = Type::new("demo::ItemId", Kind::Typedef, 4, 4).with_inner("demo::uint32");
    /// assert_eq!(ty.inner(), Some("demo::uint32"));
    /// assert_eq!(ty.base(), None);
    /// ```
    #[must_use]
    pub const fn with_inner(mut self, inner: &'a str) -> Self {
        self.inner = Some(inner);
        self
    }

    /// The same type with the engine's element alignment, its flags byte and
    /// its internal hash, as the descriptor holds them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Type};
    ///
    /// let ty = Type::new("demo::Thing", Kind::Struct, 8, 4).with_layout(4, 2, 0x1234);
    /// assert_eq!((ty.element_align(), ty.flags(), ty.internal_hash()), (4, 2, 0x1234));
    /// ```
    #[must_use]
    pub const fn with_layout(mut self, element_align: u16, flags: u8, internal_hash: u32) -> Self {
        self.element_align = element_align;
        self.flags = flags;
        self.internal_hash = internal_hash;
        self
    }

    /// The same static array, of `count` elements.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Type};
    ///
    /// let ty = Type::new("demo::StaticArray<demo::uint8,4>", Kind::StaticArray, 4, 1)
    ///     .with_inner("demo::uint8")
    ///     .with_count(4);
    /// assert_eq!(ty.element_count(), Some(4));
    /// ```
    #[must_use]
    pub const fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }

    /// The same struct with `fields`, in the engine's order.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<Field<'a>>) -> Self {
        self.count = u32::try_from(fields.len()).unwrap_or(u32::MAX);
        self.fields = fields;
        self
    }

    /// The same enum with `constants`, in the engine's order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Constant, Kind, Type};
    ///
    /// let ty = Type::new("demo::Side", Kind::Enum, 1, 1)
    ///     .with_constants(vec![Constant::new("Left", 0), Constant::new("Right", 1)]);
    /// assert_eq!(ty.constants()[1].name(), "Right");
    /// ```
    #[must_use]
    pub fn with_constants(mut self, constants: Vec<Constant<'a>>) -> Self {
        self.count = u32::try_from(constants.len()).unwrap_or(u32::MAX);
        self.constants = constants;
        self
    }

    /// The same type with `attributes`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Attribute, Kind, Type};
    ///
    /// let ty = Type::new("demo::Table", Kind::Struct, 8, 4)
    ///     .with_attributes(vec![Attribute::new("converter", "parse")]);
    /// assert_eq!(ty.attributes().len(), 1);
    /// ```
    #[must_use]
    pub fn with_attributes(mut self, attributes: Vec<Attribute<'a>>) -> Self {
        self.attributes = attributes;
        self
    }

    /// The short name, such as `TemplateResource`.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub const fn name(&self) -> &'a str {
        self.name
    }

    /// The name the engine's scripts use, such as `ecs.TemplateResource`.
    ///
    /// # Examples
    ///
    /// See [`Type::with_names`].
    #[must_use]
    pub const fn impact_name(&self) -> &'a str {
        self.impact_name
    }

    /// The full name, such as `demo::ecs::TemplateResource`, which keys the
    /// type everywhere.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub const fn qualified_name(&self) -> &'a str {
        self.qualified_name
    }

    /// The namespace, such as `demo::ecs`: the qualified name without its
    /// short name; empty when the qualified name does not end in it.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub fn namespace(&self) -> &'a str {
        self.qualified_name
            .strip_suffix(self.name)
            .and_then(|rest| rest.strip_suffix("::"))
            .unwrap_or("")
    }

    /// FNV-1a over the short name; not stored by the engine.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Type, fnv1a32};
    ///
    /// let ty = Type::new("demo::Thing", Kind::Struct, 8, 4);
    /// assert_eq!(ty.name_hash(), fnv1a32(b"Thing"));
    /// ```
    #[must_use]
    pub const fn name_hash(&self) -> u32 {
        fnv1a32(self.name.as_bytes())
    }

    /// FNV-1a over the impact name; not stored by the engine.
    ///
    /// # Examples
    ///
    /// See [`Type::name_hash`], which works the same.
    #[must_use]
    pub const fn impact_hash(&self) -> u32 {
        fnv1a32(self.impact_name.as_bytes())
    }

    /// FNV-1a over the qualified name: the hash resource keys and variants
    /// carry.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub const fn qualified_hash(&self) -> u32 {
        fnv1a32(self.qualified_name.as_bytes())
    }

    /// The second hash the engine's descriptor keeps beside the qualified
    /// one; 0 when the type was built without it.
    ///
    /// # Examples
    ///
    /// See [`Type::with_layout`].
    #[must_use]
    pub const fn internal_hash(&self) -> u32 {
        self.internal_hash
    }

    /// What the type is.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// The size in bytes.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub const fn size(&self) -> u32 {
        self.size
    }

    /// The alignment in bytes.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub const fn align(&self) -> u16 {
        self.align
    }

    /// The engine's alignment of the type as an array element.
    ///
    /// # Examples
    ///
    /// See [`Type::with_layout`].
    #[must_use]
    pub const fn element_align(&self) -> u16 {
        self.element_align
    }

    /// The engine's flags byte of the descriptor.
    ///
    /// # Examples
    ///
    /// See [`Type::with_layout`].
    #[must_use]
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// How many fields or constants the type has, or a static array's
    /// length.
    ///
    /// # Examples
    ///
    /// See [`Type::with_count`].
    #[must_use]
    pub const fn field_count(&self) -> u32 {
        self.count
    }

    /// How many elements a [`Kind::StaticArray`] holds; `None` for every
    /// other kind.
    ///
    /// # Examples
    ///
    /// See [`Type::with_count`].
    #[must_use]
    pub fn element_count(&self) -> Option<u32> {
        (self.kind == Kind::StaticArray).then_some(self.count)
    }

    /// The qualified name of the type this one holds or names; `None` for a
    /// struct, whose inner type is its [`base`](Self::base).
    ///
    /// # Examples
    ///
    /// See [`Type::with_inner`].
    #[must_use]
    pub fn inner(&self) -> Option<&'a str> {
        self.inner.filter(|_| self.kind != Kind::Struct)
    }

    /// The qualified name of the struct this one derives from; `None` for
    /// every other type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Kind, Type};
    ///
    /// let died = Type::new("demo::ecs::DiedEvent", Kind::Struct, 8, 4).with_inner("demo::ecs::Event");
    /// assert_eq!(died.base(), Some("demo::ecs::Event"));
    /// ```
    #[must_use]
    pub fn base(&self) -> Option<&'a str> {
        self.inner.filter(|_| self.kind == Kind::Struct)
    }

    /// A struct's own fields, in the engine's order; its base's fields are
    /// the base type's.
    ///
    /// # Examples
    ///
    /// See [`Type`].
    #[must_use]
    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
    }

    /// An enum's constants, in the engine's order.
    ///
    /// # Examples
    ///
    /// See [`Type::with_constants`].
    #[must_use]
    pub fn constants(&self) -> &[Constant<'a>] {
        &self.constants
    }

    /// The type's attributes, in the engine's order.
    ///
    /// # Examples
    ///
    /// See [`Type::with_attributes`].
    #[must_use]
    pub fn attributes(&self) -> &[Attribute<'a>] {
        &self.attributes
    }
}

/// The part of a qualified name after its last `::` outside angle brackets.
fn last_part(qualified: &str) -> &str {
    let mut depth = 0i32;
    let mut start = 0;
    let bytes = qualified.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b':' if depth == 0 && bytes.get(i + 1) == Some(&b':') => {
                start = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    qualified.get(start..).unwrap_or(qualified)
}

/// FNV-1a over 32 bits, the hash the engine keys type names, resource types
/// and GUID-derived ids with.
///
/// # Examples
///
/// ```
/// use enshwick::resources::fnv1a32;
///
/// assert_eq!(fnv1a32(b""), 0x811c_9dc5);
/// assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
/// ```
#[must_use]
pub const fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u32).wrapping_mul(0x0100_0193);
        i += 1;
    }
    hash
}
