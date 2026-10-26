//! The mods the loader loaded, for mods that look after other mods.

use crate::Host;
use crate::abi::{ManifestFlags, ModInfo, Side, Str};

/// The id the loader lists itself under in [`mods`]; no mod may take it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::LOADER_ID;
///
/// let mods = enshwick_loader_api::mods();
/// let others = mods.iter().filter(|m| m.id != LOADER_ID);
/// assert!(others.count() <= mods.len());
/// ```
pub const LOADER_ID: &str = "enshwick";

/// The stub that starts the loader, in the game's folder.
///
/// # Examples
///
/// ```
/// assert!(enshwick_loader_api::mods::LOADER_FILES.contains(&enshwick_loader_api::mods::STUB_FILE));
/// ```
pub const STUB_FILE: &str = "powrprof.dll";

/// The loader's DLL relative to the game's folder, as its [`ModEntry::file`]
/// reads.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::{LOADER_FILE, LOADER_ID};
///
/// let loader = enshwick_loader_api::mods().into_iter().find(|m| m.id == LOADER_ID);
/// assert!(loader.is_none_or(|m| m.file == LOADER_FILE));
/// ```
pub const LOADER_FILE: &str = "mods/enshwick/enshwick.dll";

/// Every file the loader is made of, relative to the game's folder, which no
/// mod may write.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::LOADER_FILES;
///
/// let taken = |file: &str| LOADER_FILES.contains(&file);
/// assert!(taken("powrprof.dll") && !taken("mods/enshwick-safes.dll"));
/// ```
pub const LOADER_FILES: [&str; 2] = [STUB_FILE, LOADER_FILE];

/// One loaded mod, or the loader itself, from [`mods`] or [`Host::mods`].
///
/// # Examples
///
/// ```
/// for entry in enshwick_loader_api::mods() {
///     enshwick_loader_api::info!("{} {} from {}", entry.id, entry.version, entry.file);
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModEntry {
    /// The mod id; `enshwick` for the loader.
    pub id: String,
    /// The mod's SemVer version.
    pub version: String,
    /// The sides the mod runs on.
    pub side: Side,
    /// Whether the mod counts in the join check.
    pub join: bool,
    /// Whether the mod runs: not refused at init and not faulted since.
    pub running: bool,
    /// The DLL's path relative to the game's folder, with `/` separators,
    /// such as `mods/enshwick_safes.dll`. Empty in [`join_missing`] for a
    /// mod this client does not have.
    pub file: String,
    /// The name players read; empty when the manifest names none. In
    /// [`join_missing`], the name the server advertises.
    pub name: String,
    /// One sentence on what the mod does; empty for none.
    pub description: String,
    /// The authors' names.
    pub authors: Vec<String>,
    /// An SPDX licence expression; empty for none.
    pub license: String,
    /// Where players read about the mod; empty for none.
    pub homepage: String,
    /// In [`replaced`], the id of the mod that replaces this one; empty
    /// elsewhere.
    pub replaced_by: String,
}

impl Default for ModEntry {
    /// An entry with every field empty and no side, to fill with
    /// `..ModEntry::default()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::mods::ModEntry;
    ///
    /// let safes = ModEntry { id: "safes".into(), ..ModEntry::default() };
    /// assert_eq!(safes.label(), "safes");
    /// ```
    fn default() -> Self {
        Self {
            id: String::new(),
            version: String::new(),
            side: Side(0),
            join: false,
            running: false,
            file: String::new(),
            name: String::new(),
            description: String::new(),
            authors: Vec::new(),
            license: String::new(),
            homepage: String::new(),
            replaced_by: String::new(),
        }
    }
}

impl ModEntry {
    /// How messages to players name the mod: its name, or the id when the
    /// manifest names none.
    ///
    /// # Examples
    ///
    /// ```
    /// let shown: Vec<String> = enshwick_loader_api::mods().iter().map(|entry| entry.label()).collect();
    /// for entry in enshwick_loader_api::mods() {
    ///     enshwick_loader_api::info!("{} {}", entry.id, entry.version);
    /// }
    /// # drop(shown);
    /// ```
    #[must_use]
    pub fn label(&self) -> String {
        label(&self.name, &self.id)
    }
}

/// How messages to players name mod `id` called `name`: by its name alone,
/// or by the id when the name is empty. Logs name mods by id.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::label;
///
/// assert_eq!(label("Safes", "safes"), "Safes");
/// assert_eq!(label("", "safes"), "safes");
/// ```
#[must_use]
pub fn label(name: &str, id: &str) -> String {
    if name.is_empty() { id } else { name }.to_owned()
}

/// Every loaded mod, the loader first, in load order.
///
/// Empty before the loader initialises this mod.
///
/// # Examples
///
/// ```
/// let mods = enshwick_loader_api::mods();
/// let joining = mods.iter().filter(|m| m.join).count();
/// assert!(joining <= mods.len());
/// ```
#[must_use]
pub fn mods() -> Vec<ModEntry> {
    crate::host().map(|host| host.mods()).unwrap_or_default()
}

/// The server's join mods that the last refused join found missing on this
/// client or in another version: the server's version, `running` false, the
/// client's DLL path when it has the mod, and the name and homepage the
/// server advertises. Read it on [`JoinRefused`](crate::events::JoinRefused).
///
/// Empty before a refused join.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::events::JoinRefused;
///
/// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
///     load.on::<JoinRefused>(|()| {
///         for entry in enshwick_loader_api::mods::join_missing() {
///             enshwick_loader_api::info!("the server runs {} {}", entry.id, entry.version);
///         }
///     })
/// }
/// ```
#[must_use]
pub fn join_missing() -> Vec<ModEntry> {
    crate::host()
        .map(|host| host.join_missing())
        .unwrap_or_default()
}

/// The mods in `mods/` that a present mod replaces (see
/// [`Relation::REPLACES`](crate::abi::Relation::REPLACES)): their DLLs the
/// loader never started, each with [`ModEntry::replaced_by`] and its `file`,
/// which is safe to delete.
///
///
/// # Examples
///
/// ```
/// for old in enshwick_loader_api::mods::replaced() {
///     enshwick_loader_api::info!("{} is replaced by {}; delete {}", old.id, old.replaced_by, old.file);
/// }
/// ```
#[must_use]
pub fn replaced() -> Vec<ModEntry> {
    crate::host()
        .map(|host| host.replaced())
        .unwrap_or_default()
}

impl Host {
    /// The mods in `mods/` that a present mod replaces; see [`replaced`].
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_loader_api::host() {
    ///     assert!(host.replaced().iter().all(|old| !old.replaced_by.is_empty()));
    /// }
    /// ```
    #[must_use]
    pub fn replaced(&self) -> Vec<ModEntry> {
        self.listing(self.api().replaced)
    }

    /// The server's join mods the last refused join found missing; see
    /// [`join_missing`].
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_loader_api::host() {
    ///     assert!(host.join_missing().iter().all(|entry| !entry.running));
    /// }
    /// ```
    #[must_use]
    pub fn join_missing(&self) -> Vec<ModEntry> {
        self.listing(self.api().join_missing)
    }

    /// Every loaded mod, the loader first, in load order.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_loader_api::host() {
    ///     for entry in host.mods() {
    ///         let _ = (entry.id, entry.file);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn mods(&self) -> Vec<ModEntry> {
        self.listing(self.api().mods)
    }

    /// The entries a listing function of the table writes.
    fn listing(&self, list: Listing) -> Vec<ModEntry> {
        let entry_size = u32::try_from(size_of::<ModInfo>()).unwrap_or(u32::MAX);
        let mut count = 0u32;
        let status = list(
            self.handle(),
            core::ptr::null_mut(),
            entry_size,
            0,
            &raw mut count,
        );
        if !status.is_ok() {
            return Vec::new();
        }
        let mut infos = vec![ModInfo::EMPTY; count as usize];
        let capacity = u32::try_from(infos.len()).unwrap_or(u32::MAX);
        let status = list(
            self.handle(),
            infos.as_mut_ptr(),
            entry_size,
            capacity,
            &raw mut count,
        );
        if !status.is_ok() {
            return Vec::new();
        }
        infos.truncate(count as usize);
        infos
            .iter()
            // SAFETY: the loader keeps every string it lists for the process.
            .filter_map(|info| unsafe { entry(info) })
            .collect()
    }
}

/// [`HostApi::mods`] and [`HostApi::join_missing`].
type Listing = extern "system" fn(
    me: crate::abi::ModHandle,
    out: *mut ModInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> crate::abi::Status;

/// # Safety
///
/// The info's strings are valid for the call.
unsafe fn entry(info: &ModInfo) -> Option<ModEntry> {
    // SAFETY: forwarded from the caller.
    let id = unsafe { info.id.as_str() }?;
    // SAFETY: as above.
    let version = unsafe { info.version.as_str() }?;
    // SAFETY: as above.
    let file = unsafe { info.file.as_str() }?;
    // SAFETY: as above.
    let text = |s: &Str| unsafe { s.as_str() }.unwrap_or_default().to_owned();
    let authors = if info.authors.is_null() {
        &[][..]
    } else {
        // SAFETY: the loader points at `author_count` strings it keeps.
        unsafe { core::slice::from_raw_parts(info.authors, info.author_count) }
    };
    let (name, description, authors, license, homepage) = (
        text(&info.name),
        text(&info.description),
        authors.iter().map(text).collect(),
        text(&info.license),
        text(&info.homepage),
    );
    let replaced_by = text(&info.replaced_by);
    Some(ModEntry {
        id: id.to_owned(),
        version: version.to_owned(),
        side: info.side,
        join: info.flags.contains(ManifestFlags::JOIN),
        running: info.off == 0,
        file: file.to_owned(),
        name,
        description,
        authors,
        license,
        homepage,
        replaced_by,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Header;

    #[test]
    fn entries_read_back() {
        let info = ModInfo {
            header: Header::of::<ModInfo>(),
            id: Str::new("safes"),
            version: Str::new("0.1.0"),
            side: Side::BOTH,
            flags: ManifestFlags::JOIN,
            off: 1,
            file: Str::new("mods/enshwick_safes.dll"),
            name: Str::new("Safes"),
            description: Str::new("A chest only its owner opens"),
            authors: AUTHORS.as_ptr(),
            author_count: AUTHORS.len(),
            license: Str::new("MIT"),
            homepage: Str::EMPTY,
            replaced_by: Str::EMPTY,
        };
        // SAFETY: static strings.
        let entry = unsafe { entry(&info) }.unwrap();
        assert!(entry.join);
        assert!(!entry.running);
        assert_eq!(entry.file, "mods/enshwick_safes.dll");
        assert_eq!(entry.label(), "Safes");
        assert_eq!(entry.authors, ["themaiby", "someone"]);
        assert_eq!(entry.license, "MIT");
    }

    static AUTHORS: [Str; 2] = [Str::new("themaiby"), Str::new("someone")];
}
