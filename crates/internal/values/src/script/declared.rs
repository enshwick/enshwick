//! A function a script may call, as definitions declare it.

use super::{Function, Signature};

/// A function of a [`Namespace`](struct.Namespace.html) as definitions declare it.
///
/// # Examples
///
/// See [`Namespace::functions`](struct.Namespace.html#method.functions).
#[derive(Clone, Copy, Debug)]
pub struct Declared {
    /// Its place in its list of functions.
    index: usize,
    name: &'static str,
    doc: &'static str,
    signature: &'static Signature,
}

impl Declared {
    /// The function at `index` of its list.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        index: usize,
        name: &'static str,
        doc: &'static str,
        signature: &'static Signature,
    ) -> Self {
        Self {
            index,
            name,
            doc,
            signature,
        }
    }

    /// The function's name.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`](struct.Namespace.html#method.functions).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Its doc comment up to the first section.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`](struct.Namespace.html#method.functions).
    #[must_use]
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Its parameters and answer.
    ///
    /// # Examples
    ///
    /// See [`Namespace::functions`](struct.Namespace.html#method.functions).
    #[must_use]
    pub const fn signature(&self) -> &'static Signature {
        self.signature
    }

    /// Its place in its list, which `Opened::call` and
    /// [`Handle::call`](super::Handle::call) index.
    #[doc(hidden)]
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

/// `functions` as a script declares them, each with its place in the list.
#[doc(hidden)]
pub fn declare<T: 'static>(functions: impl Iterator<Item = &'static Function<T>>) -> Vec<Declared> {
    functions
        .enumerate()
        .map(|(index, f)| Declared {
            index,
            name: f.name,
            doc: f.doc,
            signature: &f.signature,
        })
        .collect()
}
