//! The mods the loader loaded, for mods that look after other mods, such as an
//! updater.

use crate::Host;
use crate::abi::{ManifestFlags, ModInfo, Side, Str, UpdateKind, UpdateSource};

/// Where a mod's new releases come from, as its manifest declares with
/// `updates:`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::Updates;
///
/// let safes = Updates::GitHub {
///     repository: "themaiby/enshwick".into(),
///     component: "enshwick-safes".into(),
/// };
/// assert_eq!(safes.to_line(), "github:themaiby/enshwick:enshwick-safes");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
    /// GitHub releases of `repository` (`owner/name`) whose tags name
    /// `component`'s version, such as `<component>-v1.2.0`.
    GitHub {
        /// The repository, `owner/name`.
        repository: String,
        /// The mod's name in the repository's tags and release files.
        component: String,
    },
    /// A JSON document at this `https://` URL that lists the mod's versions.
    Url(String),
}

impl Updates {
    /// The source as one line, as a server advertises it:
    /// `github:<owner/name>:<component>` or `url:<url>`; [`Updates::parse`]
    /// reads it back.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::mods::Updates;
    ///
    /// let safes = Updates::GitHub {
    ///     repository: "themaiby/enshwick".into(),
    ///     component: "enshwick-safes".into(),
    /// };
    /// assert_eq!(safes.to_line(), "github:themaiby/enshwick:enshwick-safes");
    /// assert_eq!(Updates::parse(&safes.to_line()), Some(safes));
    /// ```
    #[must_use]
    pub fn to_line(&self) -> String {
        match self {
            Self::GitHub {
                repository,
                component,
            } => format!("github:{repository}:{component}"),
            Self::Url(url) => format!("url:{url}"),
        }
    }

    /// Reads a source written by [`Updates::to_line`]; `None` for anything
    /// else, such as a repository that is not `owner/name` or a URL that is
    /// not `https://`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::mods::Updates;
    ///
    /// assert_eq!(
    ///     Updates::parse("url:https://example.com/latest.json"),
    ///     Some(Updates::Url("https://example.com/latest.json".into()))
    /// );
    /// assert_eq!(Updates::parse("github:no-owner:x"), None);
    /// ```
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let plain = |text: &str| {
            !text.is_empty()
                && text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        if let Some(rest) = line.strip_prefix("github:") {
            let (repository, component) = rest.rsplit_once(':')?;
            let (owner, name) = repository.split_once('/')?;
            return (plain(owner) && plain(name) && plain(component)).then(|| Self::GitHub {
                repository: repository.to_owned(),
                component: component.to_owned(),
            });
        }
        let url = line.strip_prefix("url:")?;
        (url.starts_with("https://") && !url.contains(char::is_whitespace))
            .then(|| Self::Url(url.to_owned()))
    }

    /// Reads an ABI source; `None` for none, an unknown kind or text that is
    /// not UTF-8.
    ///
    /// # Safety
    ///
    /// The source's strings are valid for the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::abi::UpdateSource;
    /// use enshwick_loader_api::mods::Updates;
    ///
    /// let source = UpdateSource::url("https://example.com/latest.json");
    /// // SAFETY: static strings.
    /// let read = unsafe { Updates::from_abi(&source) };
    /// assert_eq!(read, Some(Updates::Url("https://example.com/latest.json".into())));
    /// ```
    #[must_use]
    pub unsafe fn from_abi(source: &UpdateSource) -> Option<Self> {
        // SAFETY: forwarded from the caller.
        let location = unsafe { source.location.as_str() }?.to_owned();
        // SAFETY: as above.
        let component = unsafe { source.component.as_str() }?.to_owned();
        match source.kind {
            UpdateKind::GITHUB => Some(Self::GitHub {
                repository: location,
                component,
            }),
            UpdateKind::URL => Some(Self::Url(location)),
            _ => None,
        }
    }
}

/// Whether `text` is a minisign public key as the second line of a `.pub`
/// file holds it: `RW` and base64, 42 bytes of an Ed25519 key.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::is_author_key;
///
/// assert!(is_author_key("RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"));
/// assert!(!is_author_key("RWnot-a-key"));
/// ```
#[must_use]
pub const fn is_author_key(text: &str) -> bool {
    /// An Ed25519 public key in minisign is 42 bytes, 56 base64 digits.
    const LEN: usize = (2 + 8 + 32) / 3 * 4;
    let text = text.as_bytes();
    if text.len() != LEN {
        return false;
    }
    let mut digits = [0u8; 3];
    let mut i = 0;
    while i < LEN {
        let digit = match text[i] {
            b @ b'A'..=b'Z' => b - b'A',
            b @ b'a'..=b'z' => b - b'a' + 26,
            b @ b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return false,
        };
        if i < digits.len() {
            digits[i] = digit;
        }
        i += 1;
    }
    // The first two bytes name the algorithm, `Ed`.
    let algorithm = [
        (digits[0] << 2) | (digits[1] >> 4),
        ((digits[1] & 0xf) << 4) | (digits[2] >> 2),
    ];
    algorithm[0] == b'E' && algorithm[1] == b'd'
}

/// The author keys as one line, as a server advertises them: the keys
/// separated by spaces; [`parse_keys`] reads it back.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::{keys_line, parse_keys};
///
/// let keys = vec!["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF".to_owned()];
/// assert_eq!(parse_keys(&keys_line(&keys)), keys);
/// ```
#[must_use]
pub fn keys_line(keys: &[String]) -> String {
    keys.join(" ")
}

/// The author keys in a line written by [`keys_line`]; empty when any of
/// them is not a key, or when there are more than two.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::parse_keys;
///
/// assert!(parse_keys("RWnot-a-key").is_empty());
/// assert!(parse_keys("").is_empty());
/// ```
#[must_use]
pub fn parse_keys(line: &str) -> Vec<String> {
    let keys: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
    if keys.len() <= 2 && keys.iter().all(|key| is_author_key(key)) {
        keys
    } else {
        Vec::new()
    }
}

/// The author keys of an ABI listing or manifest, skipping empty and
/// malformed ones.
///
/// # Safety
///
/// The strings are valid for the call.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Str;
/// use enshwick_loader_api::mods::keys_from_abi;
///
/// let key = "RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF";
/// // SAFETY: static strings.
/// let keys = unsafe { keys_from_abi(&[Str::new(key), Str::new("bad")]) };
/// assert_eq!(keys, [key]);
/// ```
#[must_use]
pub unsafe fn keys_from_abi(keys: &[Str]) -> Vec<String> {
    keys.iter()
        // SAFETY: forwarded from the caller.
        .filter_map(|key| unsafe { key.as_str() })
        .filter(|key| is_author_key(key))
        .map(str::to_owned)
        .collect()
}

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

/// Every file the loader is made of, relative to the game's folder: what an
/// update of the loader replaces and no mod may write.
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
///     if let Some(updates) = &entry.updates {
///         enshwick_loader_api::info!("{} {} updates from {updates:?}", entry.id, entry.version);
///     }
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
    /// such as `mods/enshwick_safes.dll`; a staged update of it goes to
    /// `mods/.staged/<file>`. Empty in [`join_missing`] for a mod this client
    /// does not have.
    pub file: String,
    /// Where new releases come from; `None` when the mod declares none.
    pub updates: Option<Updates>,
    /// The minisign public keys (`RW...`) the author signs releases with, as
    /// the manifest declares them with `signed_by:`; empty when it declares
    /// none. In [`join_missing`],
    /// the keys the server advertises for the mod.
    pub signed_by: Vec<String>,
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
            updates: None,
            signed_by: Vec::new(),
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
    /// How messages to players name the mod: `Name (id)`, or the id alone
    /// when the manifest names none.
    ///
    /// # Examples
    ///
    /// ```
    /// for entry in enshwick_loader_api::mods() {
    ///     enshwick_loader_api::info!("{} {}", entry.label(), entry.version);
    /// }
    /// ```
    #[must_use]
    pub fn label(&self) -> String {
        label(&self.name, &self.id)
    }
}

/// How messages to players name mod `id` called `name`: `Name (id)`, or the
/// id alone when the name is empty or the id itself.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::mods::label;
///
/// assert_eq!(label("Safes", "safes"), "Safes (safes)");
/// assert_eq!(label("", "safes"), "safes");
/// assert_eq!(label("safes", "safes"), "safes");
/// ```
#[must_use]
pub fn label(name: &str, id: &str) -> String {
    if name.is_empty() || name == id {
        id.to_owned()
    } else {
        format!("{name} ({id})")
    }
}

/// Every loaded mod, the loader first, in load order.
///
/// Empty before the loader initialises this mod.
///
/// # Examples
///
/// ```
/// let mods = enshwick_loader_api::mods();
/// let updatable = mods.iter().filter(|m| m.updates.is_some()).count();
/// assert!(updatable <= mods.len());
/// ```
#[must_use]
pub fn mods() -> Vec<ModEntry> {
    crate::host().map(|host| host.mods()).unwrap_or_default()
}

/// The server's join mods that the last refused join found missing on this
/// client or in another version: the server's version, `running` false, the
/// client's DLL path when it has the mod and where the server's version
/// comes from. Read it on [`JoinRefused`](crate::events::JoinRefused).
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
    ///         let _ = (entry.id, entry.file, entry.updates);
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
    let updates = unsafe { Updates::from_abi(&info.updates) };
    // SAFETY: as above.
    let signed_by = unsafe { keys_from_abi(&info.signed_by) };
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
        updates,
        signed_by,
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
    fn abi_sources_read_back() {
        let github = UpdateSource::github("themaiby/enshwick", "enshwick-safes");
        // SAFETY: static strings.
        let read = unsafe { Updates::from_abi(&github) };
        assert_eq!(
            read,
            Some(Updates::GitHub {
                repository: "themaiby/enshwick".into(),
                component: "enshwick-safes".into(),
            })
        );
        let url = UpdateSource::url("https://example.com/latest.json");
        // SAFETY: static strings.
        let read = unsafe { Updates::from_abi(&url) };
        assert_eq!(
            read,
            Some(Updates::Url("https://example.com/latest.json".into()))
        );
        // SAFETY: static strings.
        assert_eq!(unsafe { Updates::from_abi(&UpdateSource::NONE) }, None);
    }

    #[test]
    fn sources_round_trip_as_lines() {
        let github = Updates::GitHub {
            repository: "someone/their-mods".into(),
            component: "their_mod.v2".into(),
        };
        assert_eq!(Updates::parse(&github.to_line()), Some(github));
        let url = Updates::Url("https://example.com/a.json".into());
        assert_eq!(Updates::parse(&url.to_line()), Some(url));
        for bad in [
            "",
            "github:",
            "github:owner/name",
            "github:owner/name:",
            "github:a/b/c:x",
            "github:own er/name:x",
            "url:http://example.com",
            "url:https://exa mple.com",
            "ftp:x",
        ] {
            assert_eq!(Updates::parse(bad), None, "{bad}");
        }
    }

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
            updates: UpdateSource::NONE,
            signed_by: [Str::new(KEY), Str::new("RWnot-a-key")],
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
        assert_eq!(entry.updates, None);
        assert_eq!(entry.file, "mods/enshwick_safes.dll");
        assert_eq!(entry.signed_by, [KEY]);
        assert_eq!(entry.label(), "Safes (safes)");
        assert_eq!(entry.authors, ["themaiby", "someone"]);
        assert_eq!(entry.license, "MIT");
    }

    static AUTHORS: [Str; 2] = [Str::new("themaiby"), Str::new("someone")];

    const KEY: &str = "RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF";

    #[test]
    fn author_keys_are_minisign_public_keys() {
        assert!(is_author_key(KEY));
        assert!(is_author_key(
            "RWSVvUJNE6Z0IzsG2I1G/+JlpjWYHLgP8QVhvMlSWCg0AShBnBmVIt4r"
        ));
        for bad in [
            "",
            "RW",
            &KEY[..55],
            &format!("{KEY}A"),
            &format!("{}=", &KEY[..55]),
            &KEY.replace('R', "S"),
            &KEY.replace('j', "-"),
            "untrusted comment: minisign public key",
        ] {
            assert!(!is_author_key(bad), "{bad}");
        }
    }

    #[test]
    fn key_lines_round_trip_and_refuse_bad_keys() {
        let other = "RWSVvUJNE6Z0IzsG2I1G/+JlpjWYHLgP8QVhvMlSWCg0AShBnBmVIt4r";
        let two = vec![KEY.to_owned(), other.to_owned()];
        assert_eq!(parse_keys(&keys_line(&two)), two);
        assert!(keys_line(&two).len() <= 127);
        assert!(parse_keys(&format!("{KEY} RWnot")).is_empty());
        assert!(parse_keys(&format!("{KEY} {KEY} {KEY}")).is_empty());
    }
}
