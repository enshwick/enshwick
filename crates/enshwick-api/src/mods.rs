//! The mods the loader loaded, for mods that look after other mods, such as an
//! updater.

use crate::Host;
use crate::abi::{Header, HostApi, ManifestFlags, ModInfo, Side, Str, UpdateKind, UpdateSource};

/// Where a mod's new releases come from, as its manifest declares with
/// `updates:`.
///
/// # Examples
///
/// ```
/// use enshwick_api::mods::Updates;
///
/// let safes = Updates::GitHub {
///     repository: "themaiby/enshwick".into(),
///     component: "enshwick-safes".into(),
/// };
/// assert_eq!(safes.tag_prefix(), Some("enshwick-safes-v".to_owned()));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
    /// GitHub releases of `repository` (`owner/name`) tagged
    /// `<component>-v<semver>`.
    GitHub {
        /// The repository, `owner/name`.
        repository: String,
        /// The tag prefix before `-v<semver>`.
        component: String,
    },
    /// A JSON document at this `https://` URL that names the latest release.
    Url(String),
}

impl Updates {
    /// The prefix of the tags that are this mod's releases, `<component>-v`;
    /// `None` for a URL source.
    ///
    /// # Examples
    ///
    /// See [`Updates`].
    #[must_use]
    pub fn tag_prefix(&self) -> Option<String> {
        match self {
            Self::GitHub { component, .. } => Some(format!("{component}-v")),
            Self::Url(_) => None,
        }
    }

    /// The source as one line, as a server advertises it:
    /// `github:<owner/name>:<component>` or `url:<url>`; [`Updates::parse`]
    /// reads it back.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::mods::Updates;
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
    /// use enshwick_api::mods::Updates;
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
    /// use enshwick_api::abi::UpdateSource;
    /// use enshwick_api::mods::Updates;
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

/// One loaded mod, or the loader itself, from [`mods`] or [`Host::mods`].
///
/// # Examples
///
/// ```
/// for entry in enshwick_api::mods() {
///     if let Some(updates) = &entry.updates {
///         enshwick_api::info!("{} {} updates from {updates:?}", entry.id, entry.version);
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
}

/// Every loaded mod, the loader first, in load order.
///
/// Empty before the loader initialises this mod and under a loader older
/// than ABI minor 5.
///
/// # Examples
///
/// ```
/// let mods = enshwick_api::mods();
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
/// comes from. Read it on [`JoinMissing`](crate::events::JoinMissing).
///
/// Empty before a refused join and under a loader older than ABI minor 10.
///
/// # Examples
///
/// ```
/// use enshwick_api::events::JoinMissing;
///
/// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
///     load.on::<JoinMissing>(|()| {
///         for entry in enshwick_api::mods::join_missing() {
///             enshwick_api::info!("the server runs {} {}", entry.id, entry.version);
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

impl Host {
    /// The server's join mods the last refused join found missing; see
    /// [`join_missing`]. Empty under a loader older than ABI minor 10.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_api::host() {
    ///     assert!(host.join_missing().iter().all(|entry| !entry.running));
    /// }
    /// ```
    #[must_use]
    pub fn join_missing(&self) -> Vec<ModEntry> {
        let end = core::mem::offset_of!(HostApi, join_missing) + size_of::<usize>();
        if !self.api().header.covers(end) {
            return Vec::new();
        }
        self.listing(self.api().join_missing)
    }

    /// Every loaded mod, the loader first, in load order; empty under a
    /// loader older than ABI minor 5.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick_api::host() {
    ///     for entry in host.mods() {
    ///         let _ = (entry.id, entry.file, entry.updates);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn mods(&self) -> Vec<ModEntry> {
        let end = core::mem::offset_of!(HostApi, mods) + size_of::<usize>();
        if !self.api().header.covers(end) {
            return Vec::new();
        }
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
        let blank = ModInfo {
            header: Header::of::<ModInfo>(),
            id: Str::EMPTY,
            version: Str::EMPTY,
            side: Side(0),
            flags: ManifestFlags::NONE,
            off: 0,
            file: Str::EMPTY,
            updates: UpdateSource::NONE,
        };
        let mut infos = vec![blank; count as usize];
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
    let updates_end = core::mem::offset_of!(ModInfo, updates) + size_of::<UpdateSource>();
    let updates = if info.header.covers(updates_end) {
        // SAFETY: as above.
        unsafe { Updates::from_abi(&info.updates) }
    } else {
        None
    };
    Some(ModEntry {
        id: id.to_owned(),
        version: version.to_owned(),
        side: info.side,
        join: info.flags.contains(ManifestFlags::JOIN),
        running: info.off == 0,
        file: file.to_owned(),
        updates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        // SAFETY: static strings.
        let entry = unsafe { entry(&info) }.unwrap();
        assert!(entry.join);
        assert!(!entry.running);
        assert_eq!(entry.updates, None);
        assert_eq!(entry.file, "mods/enshwick_safes.dll");
    }
}
