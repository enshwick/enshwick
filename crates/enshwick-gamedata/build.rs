//! Embeds every table in `tables/`.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let dir = root.join("tables");
    println!("cargo::rerun-if-changed={}", dir.display());
    let mut files: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        })
        .collect();
    files.sort();
    let mut out = String::from("&[\n");
    for file in &files {
        let path = dir.join(file);
        println!("cargo::rerun-if-changed={}", path.display());
        writeln!(
            out,
            "    ({file:?}, include_str!({:?})),",
            path.display().to_string()
        )?;
    }
    out.push(']');
    let target = PathBuf::from(std::env::var("OUT_DIR")?).join("embedded.rs");
    std::fs::write(target, out)?;
    Ok(())
}
