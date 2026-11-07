//! Embeds the builds `reports/` names.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
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
