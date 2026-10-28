//! DLLs that run beside Enshwick without its API: native mods in `mods/`,
//! which the loader loads and leaves alone, other mods' proxies in
//! `mods/chain/`, which the `powrprof.dll` stub loads before the loader, and
//! other mods' proxies the game loads from its own folder.

use std::path::{Path, PathBuf};

use enshwick_gamedata::reflect::{Image, Layout};

/// The folder whose DLLs the stub loads before the loader, as messages name
/// it.
#[cfg(windows)]
pub(crate) const CHAIN_DIR: &str = "mods/chain";

/// The files among `names` that take a system DLL's place, sorted by name:
/// DLLs that `takes_place` says stand in for a system folder's DLL,
/// Enshwick's own stub aside. Windows loads a DLL from the game's folder
/// before the system's copy, so each of them is another mod's proxy,
/// whatever name it took.
pub(crate) fn proxies<'a>(
    names: impl IntoIterator<Item = &'a str>,
    takes_place: impl Fn(&str) -> bool,
) -> Vec<&'a str> {
    let mut found: Vec<&str> = names
        .into_iter()
        .filter(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
                && !name.eq_ignore_ascii_case(enshwick::mods::STUB_FILE)
                && takes_place(name)
        })
        .collect();
    found.sort_by_cached_key(|name| name.to_ascii_lowercase());
    found
}

/// Whether a DLL whose version resource names `company` is Microsoft's own
/// build, such as a runtime a game ships beside itself, rather than a proxy.
pub(crate) fn by_microsoft(company: &str) -> bool {
    company.trim_start().starts_with("Microsoft")
}

/// Other mods' proxies in `game_dir`, as [`proxies`] finds them against the
/// system folder, `%SystemRoot%\System32` for a 64-bit game, leaving out
/// Microsoft's own DLLs; none when either folder does not read.
#[cfg(windows)]
pub(crate) fn beside_game(game_dir: &Path) -> Vec<String> {
    let Some(system) = std::env::var_os("SystemRoot").map(|root| Path::new(&root).join("System32"))
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(game_dir) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    proxies(names.iter().map(String::as_str), |name| {
        system.join(name).is_file()
            && !std::fs::read(game_dir.join(name))
                .ok()
                .and_then(|bytes| company(&bytes))
                .is_some_and(|c| by_microsoft(&c))
    })
    .into_iter()
    .map(str::to_owned)
    .collect()
}

use crate::pe::PE_POINTER;
/// The export directory's address and size, from the optional header of a
/// PE32+ image.
const EXPORT_DIRECTORY: usize = 24 + 112;
/// In the export directory: the count of names, and the address of the list
/// of their addresses.
const NAME_COUNT: u64 = 0x18;
const NAMES: u64 = 0x20;
/// The longest export name read.
const LONGEST_NAME: u64 = 256;
/// The resource directory's address, from the optional header of a PE32+
/// image.
const RESOURCE_DIRECTORY: usize = 24 + 128;
/// The resource type of a version resource, `RT_VERSION`.
const VERSION_TYPE: u32 = 16;
/// The bit of a resource entry's offset that marks a directory.
const SUBDIRECTORY: u32 = 0x8000_0000;
/// More entries than this in one resource directory mean a misread.
const MOST_ENTRIES: u32 = 4096;
/// The most bytes of a version resource read.
const LONGEST_VERSION: u64 = 0x4000;

/// The folder under `game_dir` whose DLLs the stub loads, in the form the
/// stub loads them from, so a module lookup by path finds them.
pub(crate) fn chain_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods").join("chain")
}

/// Whether the DLL at `path` exports the Enshwick interface, read from its
/// export table without loading it; `None` when the file does not read as a
/// 64-bit PE image.
#[cfg(windows)]
pub(crate) fn is_mod_file(path: &Path) -> Option<bool> {
    let bytes = std::fs::read(path).ok()?;
    exports(&bytes, enshwick::abi::exports::ABI)
}

/// Whether the PE file `bytes` exports `name`; `None` when it does not read.
fn exports(bytes: &[u8], name: &str) -> Option<bool> {
    let image = Image::parse(bytes, Layout::File, Some(0)).ok()?;
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let pe = usize::try_from(u32_at(PE_POINTER)?).ok()?;
    let directory = u64::from(u32_at(pe + EXPORT_DIRECTORY)?);
    if directory == 0 {
        return Some(false);
    }
    let read_u32 = |va: u64| -> Option<u64> {
        Some(u64::from(u32::from_le_bytes(
            image.read(va, 4)?.try_into().ok()?,
        )))
    };
    let count = read_u32(directory + NAME_COUNT)?;
    let names = read_u32(directory + NAMES)?;
    for i in 0..count {
        let at = read_u32(names + i * 4)?;
        // A name near its section's end is read with fewer bytes.
        let text = (1..=LONGEST_NAME)
            .rev()
            .step_by(16)
            .find_map(|len| image.read(at, len))
            .unwrap_or_default();
        let found = text.iter().position(|&b| b == 0).map(|end| &text[..end]);
        if found == Some(name.as_bytes()) {
            return Some(true);
        }
    }
    Some(false)
}

/// The company the PE file `bytes` names in its version resource; `None`
/// when it has none or does not read.
fn company(bytes: &[u8]) -> Option<String> {
    let image = Image::parse(bytes, Layout::File, Some(0)).ok()?;
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let pe = usize::try_from(u32_at(PE_POINTER)?).ok()?;
    let root = u64::from(u32_at(pe + RESOURCE_DIRECTORY)?);
    if root == 0 {
        return None;
    }
    let read_u32 =
        |va: u64| -> Option<u32> { Some(u32::from_le_bytes(image.read(va, 4)?.try_into().ok()?)) };
    // The offset of the entry of directory `at` with id `id`, or of its
    // first entry for `None`, from the resource root.
    let entry = |at: u64, id: Option<u32>| -> Option<u32> {
        let counts = read_u32(at + 12)?;
        let (named, ids) = (counts & 0xffff, counts >> 16);
        if named + ids > MOST_ENTRIES {
            return None;
        }
        let pick = match id {
            None if named + ids > 0 => 0,
            None => return None,
            Some(id) => {
                (named..named + ids).find(|&i| read_u32(at + 16 + u64::from(i) * 8) == Some(id))?
            }
        };
        read_u32(at + 16 + u64::from(pick) * 8 + 4)
    };
    let directory = |offset: u32| {
        (offset & SUBDIRECTORY != 0).then(|| root + u64::from(offset & !SUBDIRECTORY))
    };
    let names = directory(entry(root, Some(VERSION_TYPE))?)?;
    let languages = directory(entry(names, None)?)?;
    let data = entry(languages, None)?;
    if data & SUBDIRECTORY != 0 {
        return None;
    }
    let data = root + u64::from(data);
    let blob = u64::from(read_u32(data)?);
    let len = u64::from(read_u32(data + 4)?).min(LONGEST_VERSION);
    version_company(image.read(blob, len)?)
}

/// The `CompanyName` string of a version resource's bytes `blob`.
fn version_company(blob: &[u8]) -> Option<String> {
    let key: Vec<u8> = "CompanyName\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let at = blob
        .windows(key.len())
        .enumerate()
        .find(|&(i, window)| i % 2 == 0 && window == key.as_slice())
        .map(|(i, _)| i)?;
    let value = (at + key.len()).next_multiple_of(4);
    let text: Vec<u16> = blob
        .get(value..)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16(&text).ok()
}

/// Where a DLL that does not use the API sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// Directly in `mods/`, loaded by the loader.
    Mods,
    /// In `mods/chain/`, loaded by the stub.
    Chain,
    /// In the game's folder under a name the game loads, loaded by Windows.
    Game,
}

/// A DLL that runs beside Enshwick without its API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OtherDll {
    /// Its file name.
    pub(crate) file: String,
    /// Where it sits.
    pub(crate) origin: Origin,
    /// Whether it is loaded in the game now.
    pub(crate) loaded: bool,
}

/// The lines the status summary shows for `others`, an empty list for none.
#[must_use]
pub(crate) fn lines(others: &[OtherDll]) -> Vec<String> {
    [
        (Origin::Mods, "native mods in mods/, without Enshwick's API"),
        (Origin::Chain, "loaded from mods/chain/ before the loader"),
        (Origin::Game, "other mods' proxies in the game's folder"),
    ]
    .into_iter()
    .filter_map(|(origin, heading)| {
        let files: Vec<String> = others
            .iter()
            .filter(|dll| dll.origin == origin)
            .map(|dll| match (dll.loaded, dll.origin) {
                (true, _) => dll.file.clone(),
                (false, Origin::Game) => format!("{} (not loaded at start)", dll.file),
                (false, _) => format!("{} (did not load)", dll.file),
            })
            .collect();
        (!files.is_empty()).then(|| format!("{heading}: {}", files.join(", ")))
    })
    .collect()
}

/// The file names of `others` from `origin` that are loaded, or with
/// `loaded` false, that are not.
#[must_use]
pub(crate) fn files(others: &[OtherDll], origin: Origin, loaded: bool) -> Vec<&str> {
    others
        .iter()
        .filter(|dll| dll.origin == origin && dll.loaded == loaded)
        .map(|dll| dll.file.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dll(file: &str, origin: Origin, loaded: bool) -> OtherDll {
        OtherDll {
            file: file.into(),
            origin,
            loaded,
        }
    }

    #[test]
    fn the_summary_names_each_dll_by_where_it_sits() {
        let others = [
            dll("fps-unlock.dll", Origin::Mods, true),
            dll("powrprof.dll", Origin::Chain, true),
            dll("dinput8.dll", Origin::Chain, false),
            dll("dbghelp.dll", Origin::Game, true),
            dll("winmm.dll", Origin::Game, false),
        ];
        assert_eq!(
            lines(&others),
            [
                "native mods in mods/, without Enshwick's API: fps-unlock.dll",
                "loaded from mods/chain/ before the loader: powrprof.dll, dinput8.dll (did not load)",
                "other mods' proxies in the game's folder: dbghelp.dll, winmm.dll (not loaded at start)",
            ]
        );
        assert_eq!(files(&others, Origin::Chain, true), ["powrprof.dll"]);
        assert_eq!(files(&others, Origin::Chain, false), ["dinput8.dll"]);
    }

    #[test]
    fn a_dll_named_as_a_system_one_is_a_proxy_whatever_its_name() {
        let system = ["dinput8.dll", "version.dll", "winhttp.dll", "powrprof.dll"];
        let in_system = |name: &str| system.iter().any(|s| s.eq_ignore_ascii_case(name));
        let game = [
            "Version.dll",
            "steam_api64.dll",
            "powrprof.dll",
            "dinput8.dll",
            "winhttp.txt",
            "enshrouded.exe",
        ];
        assert_eq!(proxies(game, in_system), ["dinput8.dll", "Version.dll"]);
    }

    const SECTION_RVA: u32 = 0x1000;
    const SECTION_FILE: usize = 0x200;

    fn put(bytes: &mut [u8], at: usize, value: &[u8]) {
        bytes[at..at + value.len()].copy_from_slice(value);
    }

    /// A PE32+ file of one section of 0x200 bytes at [`SECTION_RVA`], all
    /// zero, which the data directory at `directory` of the optional header
    /// points at.
    fn pe_with(directory: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; SECTION_FILE + 0x200];
        put(&mut bytes, 0, b"MZ");
        put(&mut bytes, PE_POINTER, &0x40u32.to_le_bytes());
        put(&mut bytes, 0x40, b"PE\0\0");
        put(&mut bytes, 0x44, &0x8664u16.to_le_bytes());
        put(&mut bytes, 0x46, &1u16.to_le_bytes());
        put(&mut bytes, 0x54, &0xf0u16.to_le_bytes());
        let optional = 0x58;
        put(&mut bytes, optional, &0x20bu16.to_le_bytes());
        put(&mut bytes, optional + 108, &16u32.to_le_bytes());
        put(&mut bytes, 0x40 + directory, &SECTION_RVA.to_le_bytes());
        put(&mut bytes, 0x40 + directory + 4, &0x200u32.to_le_bytes());
        let section = optional + 0xf0;
        put(&mut bytes, section, b".data\0\0\0");
        put(&mut bytes, section + 8, &0x200u32.to_le_bytes());
        put(&mut bytes, section + 12, &SECTION_RVA.to_le_bytes());
        put(&mut bytes, section + 16, &0x200u32.to_le_bytes());
        put(
            &mut bytes,
            section + 20,
            &u32::try_from(SECTION_FILE).unwrap().to_le_bytes(),
        );
        bytes
    }

    /// A PE32+ file whose export table names `names`.
    fn pe_exporting(names: &[&str]) -> Vec<u8> {
        let mut bytes = pe_with(EXPORT_DIRECTORY);
        let count = u32::try_from(names.len()).unwrap();
        put(&mut bytes, SECTION_FILE + 0x18, &count.to_le_bytes());
        put(
            &mut bytes,
            SECTION_FILE + 0x20,
            &(SECTION_RVA + 0x40).to_le_bytes(),
        );
        let mut text = 0x80;
        for (i, name) in names.iter().enumerate() {
            let at = SECTION_RVA + u32::try_from(text).unwrap();
            put(&mut bytes, SECTION_FILE + 0x40 + i * 4, &at.to_le_bytes());
            put(&mut bytes, SECTION_FILE + text, name.as_bytes());
            text += name.len() + 1;
        }
        bytes
    }

    /// A PE32+ file whose version resource names `company`.
    fn pe_by(company: &str) -> Vec<u8> {
        let mut bytes = pe_with(RESOURCE_DIRECTORY);
        let at = |offset: usize| SECTION_FILE + offset;
        let one_id = 1u32 << 16;
        let dir = |bytes: &mut Vec<u8>, offset: usize, id: u32, to: u32| {
            put(bytes, at(offset + 12), &one_id.to_le_bytes());
            put(bytes, at(offset + 16), &id.to_le_bytes());
            put(bytes, at(offset + 20), &to.to_le_bytes());
        };
        dir(&mut bytes, 0, VERSION_TYPE, 0x18 | SUBDIRECTORY);
        dir(&mut bytes, 0x18, 1, 0x30 | SUBDIRECTORY);
        dir(&mut bytes, 0x30, 0x409, 0x48);
        let mut blob = vec![0u8; 6];
        blob.extend("CompanyName\0".encode_utf16().flat_map(u16::to_le_bytes));
        blob.resize(blob.len().next_multiple_of(4), 0);
        blob.extend(company.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
        put(&mut bytes, at(0x48), &(SECTION_RVA + 0x60).to_le_bytes());
        put(
            &mut bytes,
            at(0x4c),
            &u32::try_from(blob.len()).unwrap().to_le_bytes(),
        );
        put(&mut bytes, at(0x60), &blob);
        bytes
    }

    #[test]
    fn a_runtime_microsoft_built_is_no_proxy_and_a_mod_s_dll_is() {
        let runtime = company(&pe_by("Microsoft Corporation")).unwrap();
        assert_eq!(runtime, "Microsoft Corporation");
        assert!(by_microsoft(&runtime));
        let loader = company(&pe_by("ThirteenAG")).unwrap();
        assert!(!by_microsoft(&loader));
        assert_eq!(company(&pe_exporting(&["DllMain"])), None, "no resources");
        assert_eq!(company(b"not a dll"), None);
        assert_eq!(version_company(b"no key here"), None);
    }

    #[test]
    fn a_dll_is_a_mod_only_when_its_export_table_names_the_interface() {
        let abi = enshwick::abi::exports::ABI;
        assert_eq!(exports(&pe_exporting(&["DllMain", abi]), abi), Some(true));
        assert_eq!(exports(&pe_exporting(&["DllMain"]), abi), Some(false));
        assert_eq!(exports(&pe_exporting(&[]), abi), Some(false));
        assert_eq!(exports(b"not a dll", abi), None);
    }

    #[test]
    fn the_chain_folder_is_found_by_the_path_the_stub_loads_from() {
        let dir = chain_dir(Path::new("game"));
        assert_eq!(dir, Path::new("game").join("mods").join("chain"));
        let text = dir.to_string_lossy();
        assert!(!text.contains('/') || cfg!(not(windows)), "{text}");
    }

    #[test]
    fn no_other_dll_gives_no_line() {
        assert!(lines(&[]).is_empty());
    }
}
