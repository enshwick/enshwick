//! DLLs that run beside Enshwick without its API: native mods in `mods/`,
//! which the loader loads and leaves alone, and other mods' proxies in
//! `mods/chain/`, which the `powrprof.dll` stub loads before the loader.

use std::path::{Path, PathBuf};

use enshwick_reflect::{Image, Layout};

/// The folder whose DLLs the stub loads before the loader, as messages name
/// it.
pub(crate) const CHAIN_DIR: &str = "mods/chain";

/// Where the PE header starts, read from the DOS header.
const PE_POINTER: usize = 0x3c;
/// The export directory's address and size, from the optional header of a
/// PE32+ image.
const EXPORT_DIRECTORY: usize = 24 + 112;
/// In the export directory: the count of names, and the address of the list
/// of their addresses.
const NAME_COUNT: u64 = 0x18;
const NAMES: u64 = 0x20;
/// The longest export name read.
const LONGEST_NAME: u64 = 256;

/// The folder under `game_dir` whose DLLs the stub loads, in the form the
/// stub loads them from, so a module lookup by path finds them.
pub(crate) fn chain_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods").join("chain")
}

/// Whether the DLL at `path` exports the Enshwick interface, read from its
/// export table without loading it; `None` when the file does not read as a
/// 64-bit PE image.
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

/// Where a DLL that does not use the API sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// Directly in `mods/`, loaded by the loader.
    Mods,
    /// In `mods/chain/`, loaded by the stub.
    Chain,
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
    ]
    .into_iter()
    .filter_map(|(origin, heading)| {
        let files: Vec<String> = others
            .iter()
            .filter(|dll| dll.origin == origin)
            .map(|dll| {
                if dll.loaded {
                    dll.file.clone()
                } else {
                    format!("{} (did not load)", dll.file)
                }
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
        ];
        assert_eq!(
            lines(&others),
            [
                "native mods in mods/, without Enshwick's API: fps-unlock.dll",
                "loaded from mods/chain/ before the loader: powrprof.dll, dinput8.dll (did not load)",
            ]
        );
        assert_eq!(files(&others, Origin::Chain, true), ["powrprof.dll"]);
        assert_eq!(files(&others, Origin::Chain, false), ["dinput8.dll"]);
    }

    /// A PE32+ file of one section, `.edata`, whose export table names
    /// `names`.
    fn pe_exporting(names: &[&str]) -> Vec<u8> {
        const SECTION_RVA: u32 = 0x1000;
        const SECTION_FILE: usize = 0x200;
        let mut bytes = vec![0u8; SECTION_FILE + 0x200];
        let put = |bytes: &mut Vec<u8>, at: usize, value: &[u8]| {
            bytes[at..at + value.len()].copy_from_slice(value);
        };
        put(&mut bytes, 0, b"MZ");
        put(&mut bytes, PE_POINTER, &0x40u32.to_le_bytes());
        put(&mut bytes, 0x40, b"PE\0\0");
        put(&mut bytes, 0x44, &0x8664u16.to_le_bytes());
        put(&mut bytes, 0x46, &1u16.to_le_bytes());
        put(&mut bytes, 0x54, &0xf0u16.to_le_bytes());
        let optional = 0x58;
        put(&mut bytes, optional, &0x20bu16.to_le_bytes());
        put(&mut bytes, optional + 108, &16u32.to_le_bytes());
        put(
            &mut bytes,
            0x40 + EXPORT_DIRECTORY,
            &SECTION_RVA.to_le_bytes(),
        );
        put(
            &mut bytes,
            0x40 + EXPORT_DIRECTORY + 4,
            &0x100u32.to_le_bytes(),
        );
        let section = optional + 0xf0;
        put(&mut bytes, section, b".edata\0\0");
        put(&mut bytes, section + 8, &0x200u32.to_le_bytes());
        put(&mut bytes, section + 12, &SECTION_RVA.to_le_bytes());
        put(&mut bytes, section + 16, &0x200u32.to_le_bytes());
        put(
            &mut bytes,
            section + 20,
            &u32::try_from(SECTION_FILE).unwrap().to_le_bytes(),
        );
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
