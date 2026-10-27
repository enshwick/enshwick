//! Builds `hello.c` into a mod DLL for the Windows test with clang-cl and no
//! C runtime, so no Windows SDK is needed. The compiler is the one
//! cargo-xwin names for the target, `clang-cl` otherwise.

use std::path::PathBuf;
use std::process::Command;

const COMPILER_ENV: &str = "CC_x86_64_pc_windows_msvc";

fn main() -> Result<(), String> {
    let env = |name: &str| std::env::var(name).map_err(|error| format!("{name}: {error}"));
    let manifest_dir = PathBuf::from(env("CARGO_MANIFEST_DIR")?);
    let source = manifest_dir.join("hello.c");
    let headers = manifest_dir.join("../../crates/enshwick/include");
    println!("cargo::rerun-if-changed={}", source.display());
    println!(
        "cargo::rerun-if-changed={}",
        headers.join("enshwick.h").display()
    );
    println!("cargo::rerun-if-env-changed={COMPILER_ENV}");
    if env("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let out = PathBuf::from(env("OUT_DIR")?);
    let dll = out.join("hello_c.dll");
    let compiler = env(COMPILER_ENV).unwrap_or_else(|_| "clang-cl".to_owned());
    let status = Command::new(&compiler)
        // Paths join their option: clang-cl reads `/Users/...` as `/U`.
        .current_dir(&manifest_dir)
        .args([
            "--target=x86_64-pc-windows-msvc",
            "-fuse-ld=lld",
            "/nologo",
            "/std:c11",
            "/O2",
            "/W4",
            "/WX",
            "/GS-",
            "/Zl",
            "/LD",
        ])
        .arg(format!("/I{}", headers.display()))
        .arg(format!("/Fo{}/", out.display()))
        .arg(format!("/Fe{}", dll.display()))
        .arg("hello.c")
        .args(["/link", "/NOENTRY", "/NODEFAULTLIB"])
        .status()
        .map_err(|error| format!("running {compiler}, which builds hello.c: {error}"))?;
    if !status.success() {
        return Err(format!("{compiler} failed to build hello.c: {status}"));
    }
    println!("cargo::rustc-env=HELLO_C_DLL={}", dll.display());
    Ok(())
}
