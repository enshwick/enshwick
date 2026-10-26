//! A mod built with `panic = "abort"` fails to compile.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::{Command, Output};

    /// Compiles a crate that runs the unwind check under `panic`.
    fn compile_with(panic: &str) -> Output {
        let guard = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/declare/unwind.rs");
        let dir =
            std::env::temp_dir().join(format!("enshwick-panic-{panic}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("lib.rs");
        let guard = guard.display();
        std::fs::write(
            &source,
            format!("#[macro_use]\n#[path = r\"{guard}\"]\nmod unwind;\n__require_unwind!();\n"),
        )
        .unwrap();
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let output = Command::new(rustc)
            .args([
                "--edition",
                "2024",
                "--crate-type",
                "lib",
                "--emit",
                "metadata",
            ])
            .arg("-C")
            .arg(format!("panic={panic}"))
            .arg("--out-dir")
            .arg(&dir)
            .arg(&source)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        output
    }

    #[test]
    fn a_mod_built_with_panic_abort_does_not_compile() {
        let output = compile_with("abort");
        assert!(!output.status.success());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(errors.contains("panic = \"unwind\""), "{errors}");
    }

    #[test]
    fn a_mod_built_with_panic_unwind_compiles() {
        let output = compile_with("unwind");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
