//! Generates one forwarding stub per POWRPROF.dll export and exports it under
//! the original name.

use std::error::Error;
use std::fmt::Write as _;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=powrprof.exports");
    let exports = std::fs::read_to_string("powrprof.exports")?;
    let names: Vec<&str> = exports
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    let mut stubs = String::new();
    writeln!(
        stubs,
        "pub(crate) const EXPORTS: [&str; {}] = [",
        names.len()
    )?;
    for name in &names {
        writeln!(stubs, "    \"{name}\0\",")?;
    }
    writeln!(stubs, "];")?;
    writeln!(stubs, "core::arch::global_asm!(")?;
    for (i, name) in names.iter().enumerate() {
        writeln!(stubs, "    \".globl enshwick_stub_{name}\",")?;
        writeln!(stubs, "    \"enshwick_stub_{name}:\",")?;
        writeln!(
            stubs,
            "    \"jmp qword ptr [rip + {{table}} + {}]\",",
            i * 8
        )?;
    }
    writeln!(stubs, "    table = sym FORWARDS,")?;
    writeln!(stubs, ");")?;
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("stubs.rs");
    std::fs::write(out, stubs)?;

    if std::env::var("TARGET")?.ends_with("windows-msvc") {
        for name in &names {
            println!("cargo::rustc-cdylib-link-arg=/EXPORT:{name}=enshwick_stub_{name}");
        }
    }
    Ok(())
}
