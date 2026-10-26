//! DLLs that run beside Enshwick without its API: native mods in `mods/`,
//! which the loader loads and leaves alone, and other mods' proxies in
//! `mods/chain/`, which the `powrprof.dll` stub loads before the loader.

/// The folder, under the game's, whose DLLs the stub loads before the loader.
pub const CHAIN_DIR: &str = "mods/chain";

/// Where a DLL that does not use the API sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Directly in `mods/`, loaded by the loader.
    Mods,
    /// In `mods/chain/`, loaded by the stub.
    Chain,
}

/// A DLL that runs beside Enshwick without its API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtherDll {
    /// Its file name.
    pub file: String,
    /// Where it sits.
    pub origin: Origin,
    /// Whether it is loaded in the game now.
    pub loaded: bool,
}

/// The lines the status summary shows for `others`, an empty list for none.
#[must_use]
pub fn lines(others: &[OtherDll]) -> Vec<String> {
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

/// The file names of `others` from `origin` that are loaded.
#[must_use]
pub fn loaded(others: &[OtherDll], origin: Origin) -> Vec<&str> {
    others
        .iter()
        .filter(|dll| dll.origin == origin && dll.loaded)
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
        assert_eq!(loaded(&others, Origin::Chain), ["powrprof.dll"]);
    }

    #[test]
    fn no_other_dll_gives_no_line() {
        assert!(lines(&[]).is_empty());
    }
}
