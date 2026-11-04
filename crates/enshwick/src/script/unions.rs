//! Enums whose variants carry a value, as a script sees them: a variant
//! without one as its name, a variant with one as a table of one key.

use super::Ty;

/// An enum of [`#[derive(Script)]`](macro@crate::Script) whose variants may
/// carry one value each: `"Stackable"` for a unit variant, `{ Times = 1.5 }`
/// for one with a value.
///
/// # Examples
///
/// ```rust,standalone_crate
/// use enshwick::script::{FromScript, IntoScript, Ty, Typed, Value};
///
/// /// How a number changes.
/// #[derive(Clone, Copy, Debug, PartialEq, enshwick::Script)]
/// pub enum Change {
///     /// Kept.
///     Keep,
///     /// Multiplied.
///     Times(f64),
/// }
///
/// let Ty::Union(union) = Change::TY else { unreachable!() };
/// assert_eq!(union.variants()[1].name(), "Times");
/// assert!(matches!(Change::Keep.into_value(), Value::String(s) if s == "Keep"));
/// let doubled = Value::Map(vec![("Times".into(), Value::Number(2.0))]);
/// assert_eq!(Change::from_value(doubled)?, Change::Times(2.0));
/// # Ok::<(), enshwick::script::Error>(())
/// ```
#[derive(Debug)]
pub struct Union {
    name: &'static str,
    doc: &'static str,
    variants: &'static [Variant],
}

impl Union {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(name: &'static str, doc: &'static str, variants: &'static [Variant]) -> Self {
        Self {
            name,
            doc,
            variants,
        }
    }

    /// The Rust type's name.
    ///
    /// # Examples
    ///
    /// See [`Union`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The type's doc comment up to its first section.
    ///
    /// # Examples
    ///
    /// See [`Union`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its variants in order.
    ///
    /// # Examples
    ///
    /// See [`Union`].
    #[must_use]
    pub const fn variants(&self) -> &'static [Variant] {
        self.variants
    }

    /// The variant named `name`.
    ///
    /// # Examples
    ///
    /// See [`Union`].
    #[must_use]
    pub fn variant(&self, name: &str) -> Option<&'static Variant> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// A variant of a [`Union`].
///
/// # Examples
///
/// See [`Union`].
#[derive(Debug)]
pub struct Variant {
    name: &'static str,
    doc: &'static str,
    ty: Option<&'static Ty>,
}

impl Variant {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(name: &'static str, doc: &'static str, ty: Option<&'static Ty>) -> Self {
        Self { name, doc, ty }
    }

    /// The Rust variant's name.
    ///
    /// # Examples
    ///
    /// See [`Union`].
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The variant's doc comment.
    ///
    /// # Examples
    ///
    /// See [`Union`].
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// The type of the value it carries; `None` for a unit variant.
    ///
    /// # Examples
    ///
    /// See [`Union`].
    #[must_use]
    pub const fn ty(&self) -> Option<&'static Ty> {
        self.ty
    }
}
