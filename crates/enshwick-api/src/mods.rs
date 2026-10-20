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

    /// Reads an ABI source; `None` for none, an unknown kind or text that is
    /// not UTF-8.
    ///
    /// # Safety
    ///
    /// The source's strings are valid for the call.
    unsafe fn from_abi(source: &UpdateSource) -> Option<Self> {
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
    /// `mods/.staged/<file>`.
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

impl Host {
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
        let api = self.api();
        let end = core::mem::offset_of!(HostApi, mods) + size_of::<usize>();
        if !api.header.covers(end) {
            return Vec::new();
        }
        let entry_size = u32::try_from(size_of::<ModInfo>()).unwrap_or(u32::MAX);
        let mut count = 0u32;
        let status = (api.mods)(
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
        let status = (api.mods)(
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
