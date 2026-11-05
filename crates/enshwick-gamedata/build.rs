//! Embeds every table in `tables/` and the builds `reports/` names.

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
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(out_dir.join("embedded.rs"), out)?;
    let reports = root.join("reports");
    println!("cargo::rerun-if-changed={}", reports.display());
    let mut builds: Vec<String> = std::fs::read_dir(&reports)?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|name| Some(name.strip_suffix(".toml")?.to_owned()))
        .collect();
    builds.sort();
    let mut out = String::from("&[\n");
    for build in &builds {
        let mut parts = build.split('-');
        let (Some(side), Some(guid), Some(age), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(format!("reports/{build}.toml is not named <side>-<guid>-<age>").into());
        };
        writeln!(out, "    ({side:?}, {guid:?}, {age}),")?;
    }
    out.push(']');
    std::fs::write(out_dir.join("reported.rs"), out)?;
    Ok(())
}
