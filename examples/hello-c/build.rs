//! Builds `hello.c` into a mod DLL for the Windows test, with clang and no C
//! runtime, so no Windows SDK is needed.

use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), String> {
    let header_dir = "../../crates/enshwick/include";
    println!("cargo::rerun-if-changed=hello.c");
    println!("cargo::rerun-if-changed={header_dir}/enshwick.h");
    println!("cargo::rerun-if-env-changed=CLANG");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").map_err(|e| format!("OUT_DIR: {e}"))?);
    let dll = out.join("hello_c.dll");
    let clang = std::env::var("CLANG").unwrap_or_else(|_| "clang".to_owned());
    let status = Command::new(&clang)
        .args([
            "--target=x86_64-pc-windows-msvc",
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-ffreestanding",
            "-nostdlib",
            "-fuse-ld=lld",
            "-shared",
            "-Wl,/NOENTRY",
            "-I",
            header_dir,
            "hello.c",
            "-o",
        ])
        .arg(&dll)
        .status()
        .map_err(|e| format!("running {clang}, which builds hello.c: {e}"))?;
    if !status.success() {
        return Err(format!("{clang} failed to build hello.c: {status}"));
    }
    println!("cargo::rustc-env=HELLO_C_DLL={}", dll.display());
    Ok(())
}
