//! The mods the loader loaded, for mods that look after other mods.

use crate::Host;
use crate::abi::{ModInfo, Side, Str};

mod script;

pub use script::Mods;

/// The id the loader lists itself under in [`all`]; no mod may take it.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::mods::LOADER_ID;
///
/// let mods = enshwick::mods::all();
/// let others = mods.iter().filter(|m| m.id != LOADER_ID);
/// assert!(others.count() <= mods.len());
/// ```
pub const LOADER_ID: &str = "enshwick";

/// The id the game's own content goes under where a person reads or writes it.
///
/// As a mod's content goes under its mod id, the game's reads
/// `enshrouded:<debug name>`, such as `enshrouded:Tool_Torch01_T0_PK` in a
/// file of disabled recipes. No mod may take it.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::mods::GAME_ID;
///
/// let torch = format!("{GAME_ID}:Tool_Torch01_T0_PK");
/// assert_eq!(torch, "enshrouded:Tool_Torch01_T0_PK");
/// ```
pub const GAME_ID: &str = "enshrouded";

/// The file, relative to the game's folder, that turns developer tools on.
///
/// The loader reads it once at start. Developer tools are mods with
/// `developer: true` and the interface demo; without the file
/// the loader turns a developer mod off before its `init`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::mods::{DEVELOPER_TOOLS, LOADER_ID};
///
/// assert_eq!(DEVELOPER_TOOLS, format!("mods/{LOADER_ID}/developer-tools"));
/// ```
pub const DEVELOPER_TOOLS: &str = "mods/enshwick/developer-tools";

/// Why the loader turns a mod with `developer: true` off before its `init`
/// when [`DEVELOPER_TOOLS`] is missing, as its log and the Mods screen say.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::mods::{DEVELOPER_TOOL_OFF, DEVELOPER_TOOLS};
///
/// assert_eq!(
///     DEVELOPER_TOOL_OFF,
///     format!("a developer tool: it runs only while the file {DEVELOPER_TOOLS} exists")
/// );
/// ```
pub const DEVELOPER_TOOL_OFF: &str =
    "a developer tool: it runs only while the file mods/enshwick/developer-tools exists";

/// The stub that starts the loader, in the game's folder.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// assert!(enshwick::mods::LOADER_FILES.contains(&enshwick::mods::STUB_FILE));
/// ```
pub const STUB_FILE: &str = "powrprof.dll";

/// The loader's DLL relative to the game's folder, as its [`ModEntry::file`]
/// reads.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
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
/// # extern crate enshwick_core as enshwick;
/// use enshwick::mods::LOADER_FILES;
///
/// let taken = |file: &str| LOADER_FILES.contains(&file);
/// assert!(taken("powrprof.dll") && !taken("mods/you.trading-post/trading_post.dll"));
/// ```
pub const LOADER_FILES: [&str; 2] = [STUB_FILE, LOADER_FILE];

/// A loaded mod's place in load order, counted from 0 after the loader, as
/// [`stats::Origin::Mod`](crate::stats::Origin::Mod) names a mod.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::mods::ModIndex;
///
/// if let Some(entry) = ModIndex(0).entry() {
///     enshwick::info!("the first mod in load order is {}", entry.id);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, crate::Script)]
pub struct ModIndex(pub u32);

impl ModIndex {
    /// The loaded mod at this place, from [`all`]; `None` past the last
    /// mod or before the loader initialises this mod.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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
/// # extern crate enshwick_core as enshwick;
/// for entry in enshwick::mods::all() {
///     enshwick::info!("{} {} from {}", entry.id, entry.version, entry.file);
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct ModEntry {
    /// The mod id; `enshwick` for the loader.
    pub id: String,
    /// The mod's SemVer version.
    pub version: String,
    /// Where the mod runs, [`Side::SERVER`] or [`Side::BOTH`]; a
    /// [`Side::BOTH`] mod counts in the join check.
    pub side: Side,
    /// Whether the mod runs: not refused at init and not faulted since.
    pub running: bool,
    /// The DLL's path relative to the game's folder, with `/` separators,
    /// such as `mods/you.trading-post/trading_post.dll`. Empty in [`join_missing`] for a
    /// mod this client does not have.
    pub file: String,
    /// The name players read; empty when the manifest names none, and
    /// always in [`join_missing`]. [`ModEntry::label`] gives the name, or the id without one.
    pub name: String,
    /// One sentence on what the mod does; empty for none.
    pub description: String,
    /// The authors' names.
    pub authors: Vec<String>,
    /// An SPDX licence expression; empty for none.
    pub license: String,
    /// Where players read about the mod; empty for none.
    pub homepage: String,
    /// The mod's place in load order in [`all`]; `None` for the loader and
    /// in [`join_missing`].
    pub index: Option<ModIndex>,
}

impl Default for ModEntry {
    /// An entry with every field empty and no side, to fill with
    /// `..ModEntry::default()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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
            running: false,
            file: String::new(),
            name: String::new(),
            description: String::new(),
            authors: Vec::new(),
            license: String::new(),
            homepage: String::new(),
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
    /// # extern crate enshwick_core as enshwick;
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
/// # extern crate enshwick_core as enshwick;
/// let mods = enshwick::mods::all();
/// let joining = mods.iter().filter(|m| m.side == enshwick::abi::Side::BOTH).count();
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
/// # extern crate enshwick_core as enshwick;
/// if enshwick::mods::is_running("you.lanterns") {
///     enshwick::info!("lanterns light the stalls");
/// }
/// ```
#[must_use]
pub fn is_running(id: &str) -> bool {
    all().iter().any(|entry| entry.id == id && entry.running)
}

/// The server's [`Side::BOTH`] mods that the last refused join found missing
/// on this client or in another version.
///
/// Each carries the id and version the server advertises, `running` false
/// and the client's DLL path when it has the mod; name and homepage stay
/// empty, since a server chooses neither.
///
/// Read it on [`JoinRefused`](crate::events::JoinRefused).
///
/// Empty before a refused join. Once the loader keeps 64 KiB of text servers
/// advertised, an entry with new text is left out.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
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

impl Host {
    /// The entries a listing function of the table writes; `load_order`
    /// numbers them as [`all`] lists them, the loader first.
    pub(crate) fn listing(&self, list: Listing, load_order: bool) -> Vec<ModEntry> {
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

/// The folder in `mods/` that holds the listed `file`, such as `you.post` for
/// `mods/you.post/post.dll`; the loader refuses two mods in one folder.
pub(crate) fn folder(file: &str) -> Option<&str> {
    file.strip_prefix("mods/")?
        .split_once('/')
        .map(|(folder, _)| folder)
}

/// Whether the relative path `relative` stays inside the folder it is joined
/// to: no root, drive prefix or `..`.
#[doc(hidden)]
#[must_use]
pub fn stays_inside(relative: &str) -> bool {
    std::path::Path::new(relative).components().all(|part| {
        matches!(
            part,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    })
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
    Some(ModEntry {
        id: id.to_owned(),
        version: version.to_owned(),
        side: info.side,
        running: info.off == 0,
        file: file.to_owned(),
        name,
        description,
        authors,
        license,
        homepage,
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
            flags: crate::abi::ManifestFlags::NONE,
            off: 1,
            file: Str::new("mods/you.trading-post/trading_post.dll"),
            name: Str::new("Trading Post"),
            description: Str::new("A chest only its owner opens"),
            authors: AUTHORS.as_ptr(),
            author_count: AUTHORS.len(),
            license: Str::new("MIT"),
            homepage: Str::EMPTY,
        };
        // SAFETY: every `Str` in `info` and `AUTHORS` points at a static
        // string, alive and unchanged for the process.
        let entry = unsafe { entry(&info) }.unwrap();
        assert_eq!(entry.side, Side::BOTH);
        assert!(!entry.running);
        assert_eq!(entry.file, "mods/you.trading-post/trading_post.dll");
        assert_eq!(entry.label(), "Trading Post");
        assert_eq!(entry.authors, ["themaiby", "someone"]);
        assert_eq!(entry.license, "MIT");
    }

    #[test]
    fn a_listed_file_names_its_folder_only_inside_one_in_mods() {
        assert_eq!(folder("mods/Post 1.2/post.dll"), Some("Post 1.2"));
        assert_eq!(folder("mods/you.post/sub/post.dll"), Some("you.post"));
        assert_eq!(folder("mods/post.dll"), None);
        assert_eq!(folder("other/you.post/post.dll"), None);
        assert_eq!(folder(""), None);
    }

    static AUTHORS: [Str; 2] = [Str::new("themaiby"), Str::new("someone")];
}
