//! Builds `hello.c` and `hello.cpp` into mod DLLs for the Windows test with
//! clang-cl and no C or C++ runtime, so no Windows SDK is needed. The compiler is the one
//! cargo-xwin names for the target, `clang-cl` otherwise.

use std::path::PathBuf;
use std::process::Command;

const COMPILER_ENV: &str = "CC_x86_64_pc_windows_msvc";

/// One mod DLL the script builds from one source file.
struct ModDll {
    source: &'static str,
    dll: &'static str,
    /// The variable the tests read the DLL's path from.
    env: &'static str,
    flags: &'static [&'static str],
}

const MODS: [ModDll; 2] = [
    ModDll {
        source: "hello.c",
        dll: "hello_c.dll",
        env: "HELLO_C_DLL",
        flags: &["/std:c11"],
    },
    ModDll {
        source: "hello.cpp",
        dll: "hello_cpp.dll",
        env: "HELLO_CPP_DLL",
        // No runtime type information or exceptions, which need the C++
        // runtime; fields a designated initializer leaves out are zero, as in C.
        flags: &[
            "/std:c++20",
            "/GR-",
            "/EHs-c-",
            "-Wno-missing-field-initializers",
        ],
    },
];

fn main() -> Result<(), String> {
    let env = |name: &str| std::env::var(name).map_err(|error| format!("{name}: {error}"));
    let manifest_dir = PathBuf::from(env("CARGO_MANIFEST_DIR")?);
    let headers = manifest_dir.join("../../crates/enshwick/include");
    for mod_dll in MODS {
        println!(
            "cargo::rerun-if-changed={}",
            manifest_dir.join(mod_dll.source).display()
        );
    }
    println!(
        "cargo::rerun-if-changed={}",
        headers.join("enshwick.h").display()
    );
    println!("cargo::rerun-if-env-changed={COMPILER_ENV}");
    if env("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let out = PathBuf::from(env("OUT_DIR")?);
    let compiler = env(COMPILER_ENV).unwrap_or_else(|_| "clang-cl".to_owned());
    for mod_dll in MODS {
        let dll = out.join(mod_dll.dll);
        let status = Command::new(&compiler)
            // Paths join their option: clang-cl reads `/Users/...` as `/U`.
            .current_dir(&manifest_dir)
            .args([
                "--target=x86_64-pc-windows-msvc",
                "-fuse-ld=lld",
                "/nologo",
                "/O2",
                "/W4",
                "/WX",
                "/GS-",
                "/Zl",
                "/LD",
            ])
            .args(mod_dll.flags)
            .arg(format!("/I{}", headers.display()))
            .arg(format!("/Fo{}/", out.display()))
            .arg(format!("/Fe{}", dll.display()))
            .arg(mod_dll.source)
            .args(["/link", "/NOENTRY", "/NODEFAULTLIB"])
            .status()
            .map_err(|error| {
                format!("running {compiler}, which builds {}: {error}", mod_dll.source)
            })?;
        if !status.success() {
            return Err(format!(
                "{compiler} failed to build {}: {status}",
                mod_dll.source
            ));
        }
        println!("cargo::rustc-env={}={}", mod_dll.env, dll.display());
    }
    Ok(())
}
