//! The mods the loader loaded, for mods that look after other mods.

use crate::Host;
use crate::abi::{ManifestFlags, ModInfo, Side, Str};

/// The id the loader lists itself under in [`all`]; no mod may take it.
///
/// # Examples
///
/// ```
/// use enshwick::mods::LOADER_ID;
///
/// let mods = enshwick::mods::all();
/// let others = mods.iter().filter(|m| m.id != LOADER_ID);
/// assert!(others.count() <= mods.len());
/// ```
pub const LOADER_ID: &str = "enshwick";

/// The stub that starts the loader, in the game's folder.
///
/// # Examples
///
/// ```
/// assert!(enshwick::mods::LOADER_FILES.contains(&enshwick::mods::STUB_FILE));
/// ```
pub const STUB_FILE: &str = "powrprof.dll";

/// The loader's DLL relative to the game's folder, as its [`ModEntry::file`]
/// reads.
///
/// # Examples
///
/// ```
/// use enshwick::mods::{LOADER_FILE, LOADER_ID};
///
/// let loader = enshwick::mods::all().into_iter().find(|m| m.id == LOADER_ID);
/// assert!(loader.is_none_or(|m| m.file == LOADER_FILE));
/// ```
pub const LOADER_FILE: &str = "mods/enshwick/enshwick.dll";

/// Every file the loader is made of, relative to the game's folder, which no
/// mod may write.
///
/// # Examples
///
/// ```
/// use enshwick::mods::LOADER_FILES;
///
/// let taken = |file: &str| LOADER_FILES.contains(&file);
/// assert!(taken("powrprof.dll") && !taken("mods/trading_post.dll"));
/// ```
pub const LOADER_FILES: [&str; 2] = [STUB_FILE, LOADER_FILE];

/// A loaded mod's place in load order, counted from 0 after the loader, as
/// [`stats::Origin::Mod`](crate::stats::Origin::Mod) names a mod.
///
/// # Examples
///
/// ```
/// use enshwick::mods::ModIndex;
///
/// if let Some(entry) = ModIndex(0).entry() {
///     enshwick::info!("the first mod in load order is {}", entry.id);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModIndex(pub u32);

impl ModIndex {
    /// The loaded mod at this place, from [`all`]; `None` past the last
    /// mod or before the loader initialises this mod.
    ///
    /// # Examples
    ///
    /// ```
    /// let first = enshwick::mods::ModIndex(0).entry();
    /// assert!(first.is_none_or(|entry| entry.id != enshwick::mods::LOADER_ID));
    /// ```
    #[must_use]
    pub fn entry(self) -> Option<ModEntry> {
        all().into_iter().find(|entry| entry.index == Some(self))
    }
}

impl core::fmt::Display for ModIndex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

/// One loaded mod, or the loader itself, from [`all`].
///
/// # Examples
///
/// ```
/// for entry in enshwick::mods::all() {
///     enshwick::info!("{} {} from {}", entry.id, entry.version, entry.file);
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
    /// such as `mods/trading_post.dll`. Empty in [`join_missing`] for a
    /// mod this client does not have.
    pub file: String,
    /// The name players read; empty when the manifest names none. In
    /// [`join_missing`], the name the server advertises.
    /// [`ModEntry::label`] gives the name, or the id without one.
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
    /// The mod's place in load order in [`all`]; `None` for the loader and
    /// in [`join_missing`] and [`replaced`].
    pub index: Option<ModIndex>,
}

impl Default for ModEntry {
    /// An entry with every field empty and no side, to fill with
    /// `..ModEntry::default()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::mods::ModEntry;
    ///
    /// let post = ModEntry { id: "you.trading-post".into(), ..ModEntry::default() };
    /// assert_eq!(post.label(), "you.trading-post");
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
            index: None,
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
    /// let shown: Vec<String> = enshwick::mods::all().iter().map(|entry| entry.label()).collect();
    /// for entry in enshwick::mods::all() {
    ///     enshwick::info!("{} {}", entry.id, entry.version);
    /// }
    /// # drop(shown);
    /// ```
    #[must_use]
    pub fn label(&self) -> String {
        label(&self.name, &self.id)
    }
}

/// How messages to players name mod `id` called `name`: by its name alone,
/// or by the id when the name is empty, as [`ModEntry::label`] does; the
/// loader's own messages use it.
#[doc(hidden)]
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
/// let mods = enshwick::mods::all();
/// let joining = mods.iter().filter(|m| m.join).count();
/// assert!(joining <= mods.len());
/// ```
#[must_use]
pub fn all() -> Vec<ModEntry> {
    crate::host()
        .map(|host| host.listing(host.api().mods, true))
        .unwrap_or_default()
}

/// Whether the mod `id` is loaded and runs: what an optional dependency's
/// [`deps!`](crate::deps) module asks before it names the mod's content.
///
/// `false` before the loader initialises this mod. Each call reads the
/// loader's listing into a new list, as [`all`] does: ask once, at init, and
/// keep the answer.
///
/// # Examples
///
/// ```
/// if enshwick::mods::is_running("you.lanterns") {
///     enshwick::info!("lanterns light the stalls");
/// }
/// ```
#[must_use]
pub fn is_running(id: &str) -> bool {
    all().iter().any(|entry| entry.id == id && entry.running)
}

/// The server's join mods that the last refused join found missing on this
/// client or in another version.
///
/// Each carries the server's version, `running` false, the client's DLL path
/// when it has the mod, and the name and homepage the server advertises.
///
/// Read it on [`JoinRefused`](crate::events::JoinRefused).
///
/// Empty before a refused join. Once the loader keeps 64 KiB of text servers
/// advertised, an entry with new text is left out.
///
/// # Examples
///
/// ```
/// use enshwick::events::JoinRefused;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on::<JoinRefused>(|()| {
///         for entry in enshwick::mods::join_missing() {
///             enshwick::info!("the server runs {} {}", entry.id, entry.version);
///         }
///     })
/// }
/// ```
#[must_use]
pub fn join_missing() -> Vec<ModEntry> {
    crate::host()
        .map(|host| host.listing(host.api().join_missing, false))
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
/// for old in enshwick::mods::replaced() {
///     enshwick::info!("{} is replaced by {}; delete {}", old.id, old.replaced_by, old.file);
/// }
/// ```
#[must_use]
pub fn replaced() -> Vec<ModEntry> {
    crate::host()
        .map(|host| host.listing(host.api().replaced, false))
        .unwrap_or_default()
}

impl Host {
    /// The entries a listing function of the table writes; `load_order`
    /// numbers them as [`all`] lists them, the loader first.
    fn listing(&self, list: Listing, load_order: bool) -> Vec<ModEntry> {
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
            .enumerate()
            .filter_map(|(at, info)| {
                // SAFETY: `list` filled `count` entries, and the loader keeps
                // every string and author array it lists alive and unchanged
                // for the process, past this call.
                let mut entry = unsafe { entry(info) }?;
                if load_order {
                    entry.index = at
                        .checked_sub(1)
                        .and_then(|place| u32::try_from(place).ok())
                        .map(ModIndex);
                }
                Some(entry)
            })
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

/// An owned copy of one listed mod; `None` when its id, version or file is
/// not UTF-8.
///
/// # Safety
///
/// - Every [`Str`] in `info` is null or points at `len` bytes that stay alive
///   and unchanged for the call.
/// - `info.authors` is null or points at `author_count` initialised [`Str`]s,
///   each meeting the bullet above, alive for the call.
unsafe fn entry(info: &ModInfo) -> Option<ModEntry> {
    // SAFETY: the caller keeps the id's bytes alive and unchanged for the
    // call, and the text is copied before it returns.
    let id = unsafe { info.id.as_str() }?;
    // SAFETY: the caller keeps the version's bytes alive and unchanged for the
    // call, and the text is copied before it returns.
    let version = unsafe { info.version.as_str() }?;
    // SAFETY: the caller keeps the file's bytes alive and unchanged for the
    // call, and the text is copied before it returns.
    let file = unsafe { info.file.as_str() }?;
    // SAFETY: every `Str` this closure is given is one of `info`'s or one of
    // its authors, whose bytes the caller keeps alive and unchanged for the
    // call; the text is copied at once.
    let text = |s: &Str| unsafe { s.as_str() }.unwrap_or_default().to_owned();
    let authors = if info.authors.is_null() {
        &[][..]
    } else {
        // SAFETY: the caller guarantees a non-null `authors` points at
        // `author_count` initialised `Str`s alive for the call.
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
        index: None,
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
            id: Str::new("you.trading-post"),
            version: Str::new("0.1.0"),
            side: Side::BOTH,
            flags: ManifestFlags::JOIN,
            off: 1,
            file: Str::new("mods/trading_post.dll"),
            name: Str::new("Trading Post"),
            description: Str::new("A chest only its owner opens"),
            authors: AUTHORS.as_ptr(),
            author_count: AUTHORS.len(),
            license: Str::new("MIT"),
            homepage: Str::EMPTY,
            replaced_by: Str::EMPTY,
        };
        // SAFETY: every `Str` in `info` and `AUTHORS` points at a static
        // string, alive and unchanged for the process.
        let entry = unsafe { entry(&info) }.unwrap();
        assert!(entry.join);
        assert!(!entry.running);
        assert_eq!(entry.file, "mods/trading_post.dll");
        assert_eq!(entry.label(), "Trading Post");
        assert_eq!(entry.authors, ["themaiby", "someone"]);
        assert_eq!(entry.license, "MIT");
    }

    static AUTHORS: [Str; 2] = [Str::new("themaiby"), Str::new("someone")];
}
