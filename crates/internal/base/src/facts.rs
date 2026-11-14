//! Engine facts by enshwick id, which every domain resolves its layouts
//! through; re-exported as `content::EngineFacts`.

use crate::Off;
use crate::abi::Str;

/// Answers engine facts by enshwick id: the running game through `Load`
/// and [`crate::Host`], a tool through the game's reflection; the content
/// layouts resolve through it.
///
/// # Examples
///
/// ```
/// use enshwick::content::EngineFacts;
/// use enshwick::content::items::ItemLayout;
///
/// fn layout(facts: &impl EngineFacts) -> Result<ItemLayout, enshwick::Off> {
///     ItemLayout::resolve(facts)
/// }
/// ```
pub trait EngineFacts {
    /// The byte offset the id names.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks it.
    fn offset(&self, id: &str) -> Result<usize, Off>;

    /// The size of the type the id names.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks it.
    fn size(&self, id: &str) -> Result<usize, Off>;

    /// The hash of the type the id names.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks it.
    fn hash(&self, id: &str) -> Result<u32, Off>;
}

impl EngineFacts for crate::BaseLoad {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        self.host().offset(id)
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        self.host().size(id)
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        self.host().hash(id)
    }
}

impl EngineFacts for crate::Host {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        let mut out = 0u64;
        let status = (self.api().constant)(self.handle(), Str::new(id), &raw mut out);
        let offset = crate::load::ok(status, out, || format!("offset {id}"))?;
        usize::try_from(offset).map_err(|_| Off::new(format!("offset {id} is out of range")))
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        let mut out = 0u32;
        let status = (self.api().type_size)(Str::new(id), &raw mut out);
        let size = crate::load::ok(status, out, || format!("type {id}"))?;
        usize::try_from(size).map_err(|_| Off::new(format!("type {id} is out of range")))
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        let mut out = 0u32;
        let status = (self.api().type_hash)(Str::new(id), &raw mut out);
        crate::load::ok(status, out, || format!("type {id}"))
    }
}
