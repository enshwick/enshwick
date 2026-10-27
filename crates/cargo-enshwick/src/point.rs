//! `cargo enshwick point`: another mod's extension point, written as a
//! definition this mod compiles, from the owner's DLL or schema file.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow};
use cargo_enshwick::points::{Schema, from_dll, rust};

/// The schema texts `from` holds: of a schema file, of a DLL, or of every
/// DLL in a folder and in its `mods/`, each with where it came from.
fn schemas(from: &Path) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let read = |path: &Path| std::fs::read(path).with_context(|| path.display().to_string());
    if from.extension().is_some_and(|e| e == "toml") {
        let text = String::from_utf8(read(from)?).with_context(|| from.display().to_string())?;
        return Ok(vec![(from.to_path_buf(), text)]);
    }
    let mut found = Vec::new();
    for dll in dlls(from)? {
        if let Ok(Some(text)) = from_dll(&read(&dll)?) {
            found.push((dll, text));
        }
    }
    Ok(found)
}

/// The DLLs `from` names: the file itself, or every DLL in the folder and in
/// its `mods/`.
fn dlls(from: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if from.is_file() {
        return Ok(vec![from.to_path_buf()]);
    }
    let mut found = Vec::new();
    for dir in [from.to_path_buf(), from.join("mods")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    if found.is_empty() {
        return Err(anyhow!("no mod DLL in {}", from.display()));
    }
    Ok(found)
}

/// Writes the definition of the point `name`, `<mod id>:<point>`, read from
/// the owner's DLL or schema file in `from`, to `out` or as the result.
///
/// # Errors
///
/// When no DLL there defines the point, or its schema does not read.
pub(crate) fn run(
    name: &str,
    from: &Path,
    out: Option<&Path>,
    luau: bool,
) -> anyhow::Result<String> {
    let (owner, _) = name
        .split_once(':')
        .with_context(|| format!("{name:?} is no point: <mod id>:<point>"))?;
    let mut owners_seen = Vec::new();
    for (dll, text) in schemas(from)? {
        let schema = Schema::parse(&text).with_context(|| dll.display().to_string())?;
        if schema.points().any(|p| p == name) {
            let code = if luau {
                cargo_enshwick::points::luau(&schema, name)?
            } else {
                rust(&schema, name)?
            };
            return match out {
                Some(path) => {
                    std::fs::write(path, &code).with_context(|| path.display().to_string())?;
                    Ok(format!(
                        "wrote {name} of {} {} to {}",
                        schema.owner(),
                        schema.version(),
                        path.display()
                    ))
                }
                None => Ok(code.trim_end().to_owned()),
            };
        }
        owners_seen.push(schema.owner().to_owned());
    }
    if owners_seen.iter().any(|o| o == owner) {
        Err(anyhow!("{owner} defines no point {name}"))
    } else {
        Err(anyhow!(
            "no DLL in {} defines points of {owner}",
            from.display()
        ))
    }
}

/// Writes the Luau module of the mod event `name`, `<mod id>:<event>`, read
/// from the owner's DLL or schema file in `from`, to `out` or as the result.
///
/// # Errors
///
/// When no DLL there defines the event, or its schema does not read.
pub(crate) fn run_event(name: &str, from: &Path, out: Option<&Path>) -> anyhow::Result<String> {
    let (owner, _) = name
        .split_once(':')
        .with_context(|| format!("{name:?} is no event: <mod id>:<event>"))?;
    for (dll, text) in schemas(from)? {
        let schema = Schema::parse(&text).with_context(|| dll.display().to_string())?;
        if schema.events().any(|e| e == name) {
            let code = cargo_enshwick::points::luau_event(&schema, name)?;
            return match out {
                Some(path) => {
                    std::fs::write(path, &code).with_context(|| path.display().to_string())?;
                    Ok(format!(
                        "wrote {name} of {} {} to {}",
                        schema.owner(),
                        schema.version(),
                        path.display()
                    ))
                }
                None => Ok(code.trim_end().to_owned()),
            };
        }
    }
    Err(anyhow!(
        "no DLL in {} defines the event {name} of {owner}",
        from.display()
    ))
}

/// Writes the Rust declaration, or with `luau` the Luau module, of the call
/// `name`, `<mod id>:<call>`, read
/// from the server's DLL or schema file in `from`, to `out` or as the result.
///
/// # Errors
///
/// When no DLL there serves the call, or its schema does not read.
pub(crate) fn run_call(
    name: &str,
    from: &Path,
    out: Option<&Path>,
    luau: bool,
) -> anyhow::Result<String> {
    let (owner, _) = name
        .split_once(':')
        .with_context(|| format!("{name:?} is no call: <mod id>:<call>"))?;
    for (dll, text) in schemas(from)? {
        let schema = Schema::parse(&text).with_context(|| dll.display().to_string())?;
        if schema.calls().any(|c| c == name) {
            let code = if luau {
                cargo_enshwick::points::luau_call(&schema, name)?
            } else {
                cargo_enshwick::points::rust_call(&schema, name)?
            };
            return match out {
                Some(path) => {
                    std::fs::write(path, &code).with_context(|| path.display().to_string())?;
                    Ok(format!(
                        "wrote {name} of {} {} to {}",
                        schema.owner(),
                        schema.version(),
                        path.display()
                    ))
                }
                None => Ok(code.trim_end().to_owned()),
            };
        }
    }
    Err(anyhow!(
        "no DLL in {} serves the call {name} of {owner}",
        from.display()
    ))
}
