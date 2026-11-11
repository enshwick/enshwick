//! Embeds every file of `src/texts/`, one language each, so a new language
//! is a new file there and no change to the code.

use std::fmt::Write as _;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("texts");
    println!("cargo::rerun-if-changed={}", dir.display());
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "toml") {
            files.push(path);
        }
    }
    files.sort();
    let mut out = String::from("&[\n");
    for path in files {
        writeln!(out, "    include_str!({:?}),", path.display().to_string())?;
    }
    out.push(']');
    std::fs::write(Path::new(&std::env::var("OUT_DIR")?).join("texts.rs"), out)?;
    Ok(())
}
