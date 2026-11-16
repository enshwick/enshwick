//! The game's vanilla template names, for a console that names a template
//! by the name the template collection gives it.

use enshwick_core::__fake::ids;
use enshwick_core::content::workshops::WorkshopLayout;
use enshwick_core::content::{Blob, EngineFacts, Guid, types};

use crate::test::FakeHost;

/// Where a named template's name sits, unless the host already gives one:
/// where [`FakeHost::templates`] lays it out, so both name alike.
const NAME_AT: u64 = super::templates::TEMPLATE_NAME_AT;

impl FakeHost {
    /// Lists `named` in the game's template collection, each with its name,
    /// so `Vanilla::template` finds it by that name in a mod the host runs.
    /// It replaces the collection, and each template of `named` is a name
    /// alone: give templates a mod builds or patches through
    /// [`FakeHost::templates`] instead. Call [`FakeHost::workshops`] first,
    /// which gives the collection's layout; without it nothing is listed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    ///
    /// let wolf = Guid([7; 16]);
    /// let loader = enshwick::test::FakeHost::new()
    ///     .workshops()
    ///     .vanilla_templates(&[(wolf, "Enemy_Wildbeast_Wolf")])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn vanilla_templates(self, named: &[(Guid, &str)]) -> Self {
        let host = match self.offset(ids::TEMPLATE_NAME) {
            Ok(_) => self,
            Err(_) => self.constant(ids::TEMPLATE_NAME, NAME_AT),
        };
        let (Ok(layout), Ok(collection), Ok(template), Ok(name_at), Ok(count)) = (
            WorkshopLayout::resolve(&host),
            host.hash(types::TEMPLATE_COLLECTION_RESOURCE.id()),
            host.hash(types::TEMPLATE_RESOURCE.id()),
            host.offset(ids::TEMPLATE_NAME),
            u32::try_from(named.len()),
        ) else {
            return host;
        };
        let mut list = Blob::new(&vec![0; layout.templates + 8]);
        let guids: Vec<u8> = named.iter().flat_map(|(guid, _)| guid.0).collect();
        let start = list.append(&guids, 4);
        if !list.set_array(layout.templates, start, count) {
            return host;
        }
        let key = enshwick_core::content::templates::TEMPLATE_COLLECTION.key_by_hash(collection);
        let host = host.resource(key, list.into_bytes());
        named.iter().fold(host, |host, (guid, name)| {
            let mut resource = Blob::new(&vec![0; name_at + 8]);
            let text = resource.append(name.as_bytes(), 1);
            let Ok(len) = u32::try_from(name.len()) else {
                return host;
            };
            if !resource.set_array(name_at, text, len) {
                return host;
            }
            host.resource(guid.key_by_hash(template), resource.into_bytes())
        })
    }
}
