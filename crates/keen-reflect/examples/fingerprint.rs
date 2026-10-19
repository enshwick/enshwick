//! Prints a binary's build fingerprint and the entry RVAs of named systems:
//! `cargo run -p keen-reflect --example fingerprint -- <exe> [system...]`.

use std::fmt::Write as _;
use std::io::Write as _;

use keen_reflect::{Image, Layout, Reflection};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: fingerprint <exe> [system...]")?;
    let bytes = std::fs::read(&path)?;
    let image = Image::parse(&bytes, Layout::File, None)?;
    let reflection = Reflection::scan(&image);

    let mut report = String::new();
    match image.fingerprint() {
        Some(build) => {
            let guid = build.guid().iter().fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            });
            writeln!(report, "fingerprint {guid}/{}", build.age())?;
        }
        None => writeln!(report, "fingerprint none")?,
    }
    for name in args {
        match reflection.system(&name) {
            Ok(system) => {
                writeln!(
                    report,
                    "{name} {:#x}",
                    system.entry().saturating_sub(image.base())
                )?;
            }
            Err(error) => writeln!(report, "{name}: {error}")?,
        }
    }
    std::io::stdout().write_all(report.as_bytes())?;
    Ok(())
}
