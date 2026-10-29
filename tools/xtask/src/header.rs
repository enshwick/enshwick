//! The C header of the ABI, `enshwick.h`, written by cbindgen from the ABI
//! module and held to the golden layout file: every size, alignment and
//! field offset there is a `_Static_assert` compiled against the header.
//!
//! The types the header declares are the types the golden file lists, so a
//! new ABI type reaches the header without a change here.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The package the header describes.
pub(crate) const PACKAGE: &str = "enshwick";
/// The ABI module cbindgen parses, relative to the workspace root.
const ABI_SOURCE: &str = "crates/enshwick/src/abi.rs";
/// The golden layout file.
const LAYOUT: &str = "crates/enshwick/src/abi/layout.txt";
/// cbindgen's settings.
const CONFIG: &str = "crates/enshwick/cbindgen.toml";
/// The committed header.
const HEADER: &str = "crates/enshwick/include/enshwick.h";
/// What every type and constant of the header starts with, so C code keeps
/// its own names.
const PREFIX: &str = "Enshwick";
/// The target the layout holds on.
const C_TARGET: &str = "x86_64-pc-windows-msvc";

/// One type of the golden file: its size, alignment and fields as
/// `(name, offset)`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shape {
    name: String,
    size: usize,
    align: usize,
    fields: Vec<(String, usize)>,
}

/// `cargo xtask header`: rewrites the committed header.
pub(crate) fn run(root: &Path) -> Result<bool, String> {
    let header = generate(root)?;
    let path = root.join(HEADER);
    std::fs::write(&path, header).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(true)
}

/// The problems with the committed header under `root`, one line each:
/// stale against the ABI module, or apart from the golden layout.
pub(crate) fn check(root: &Path) -> Vec<String> {
    let fresh = match generate(root) {
        Ok(header) => header,
        Err(error) => return vec![error],
    };
    let committed = std::fs::read_to_string(root.join(HEADER)).unwrap_or_default();
    if committed != fresh {
        return vec![format!(
            "{HEADER} is stale: run `cargo xtask header` and commit it"
        )];
    }
    let layout = match std::fs::read_to_string(root.join(LAYOUT)) {
        Ok(text) => text,
        Err(error) => return vec![format!("{LAYOUT}: {error}")],
    };
    match parse_layout(&layout) {
        Ok(shapes) => compile_asserts(&root.join(HEADER), &asserts(&shapes, &fresh)),
        Err(error) => vec![error],
    }
}

/// The header cbindgen writes for the ABI module.
fn generate(root: &Path) -> Result<String, String> {
    let layout =
        std::fs::read_to_string(root.join(LAYOUT)).map_err(|error| format!("{LAYOUT}: {error}"))?;
    let shapes = parse_layout(&layout)?;
    let config = cbindgen::Config::from_file(root.join(CONFIG))
        .map_err(|error| format!("{CONFIG}: {error}"))?;
    let crate_root = scratch_dir("abi")?;
    let copied = copy_abi(root, &crate_root);
    let mut builder = cbindgen::Builder::new()
        .with_config(config)
        .with_item_prefix(PREFIX)
        .with_src(crate_root.join("lib.rs"));
    for shape in &shapes {
        builder = builder.include_item(&shape.name);
    }
    let bindings = copied.and_then(|()| {
        builder
            .generate()
            .map_err(|error| format!("cbindgen: {error}"))
    });
    let _removed = std::fs::remove_dir_all(&crate_root);
    let bindings = bindings?;
    let mut bytes = Vec::new();
    bindings.write(&mut bytes);
    let header =
        String::from_utf8(bytes).map_err(|error| format!("cbindgen wrote no UTF-8: {error}"))?;
    Ok(with_abi_version(&tidy(&header), &layout))
}

/// The ABI version of a golden layout file, as `enshwick::abi::ABI_VERSION`
/// computes it: FNV-1a over its bytes without carriage returns, so a
/// checkout with Windows line endings has the same version. A test of the
/// API crate holds the committed header to its constant.
fn layout_version(layout: &str) -> u32 {
    layout
        .bytes()
        .filter(|&byte| byte != b'\r')
        .fold(0x811c_9dc5, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        })
}

/// `header` with `ENSHWICK_ABI_VERSION` defined after its includes, which
/// cbindgen cannot write since the constant is computed.
fn with_abi_version(header: &str, layout: &str) -> String {
    let define = format!(
        "// The ABI version: a hash of the layout of every ABI type. A loader\n\
         // loads only mods built for its own.\n\
         #define ENSHWICK_ABI_VERSION {:#010x}\n\n",
        layout_version(layout)
    );
    let includes = "#include <stdint.h>\n\n";
    match header.find(includes) {
        Some(at) => {
            let end = at + includes.len();
            format!("{}{define}{}", &header[..end], &header[end..])
        }
        None => format!("{header}\n{define}"),
    }
}

/// The header as C authors read it: rustdoc's `# Examples` sections, which
/// show Rust, left out, links reduced to their text, and constants spelled
/// `ENSHWICK_NAME` rather than `EnshwickNAME`.
fn tidy(header: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut in_examples = false;
    for line in header.lines() {
        let comment = line.trim_start().strip_prefix("//").map(str::trim);
        if in_examples && comment.is_some() {
            continue;
        }
        in_examples = comment == Some("# Examples");
        if in_examples {
            while lines.last().is_some_and(|last| last.trim() == "//") {
                lines.pop();
            }
            continue;
        }
        lines.push(if comment.is_some() {
            plain_links(line)
        } else {
            line.to_owned()
        });
    }
    let mut text = lines.join("\n");
    text.push('\n');
    for name in shouted_constants(&text) {
        text = replace_word(
            &text,
            &format!("{PREFIX}{name}"),
            &format!("ENSHWICK_{name}"),
        );
    }
    text
}

/// The names after the prefix of the constants written in capitals, such as
/// `MAX_ZONES` for `#define EnshwickMAX_ZONES 256`.
fn shouted_constants(header: &str) -> Vec<String> {
    header
        .lines()
        .filter_map(|line| line.strip_prefix("#define ")?.strip_prefix(PREFIX))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|name| {
            name.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
        .map(str::to_owned)
        .collect()
}

/// `text` with every whole-word `from` replaced by `to`.
fn replace_word(text: &str, from: &str, to: &str) -> String {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(from) {
        let before = if at > 0 {
            rest[..at].chars().last()
        } else {
            out.chars().last()
        };
        let after = rest[at + from.len()..].chars().next();
        out.push_str(&rest[..at]);
        if before.is_some_and(word) || after.is_some_and(word) {
            out.push_str(from);
        } else {
            out.push_str(to);
        }
        rest = &rest[at + from.len()..];
    }
    out.push_str(rest);
    out
}

/// `line` with rustdoc's links reduced to their text: ``[`Header`]``,
/// ``[`Header`](super::Header)`` and `[the header](Header)` become
/// `` `Header` ``, `` `Header` `` and `the header`.
fn plain_links(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        let Some(len) = rest[start + 1..].find(']') else {
            break;
        };
        let text = &rest[start + 1..start + 1 + len];
        let after = &rest[start + 1 + len + 1..];
        let target = after
            .strip_prefix('(')
            .and_then(|target| Some(&target[..target.find(')')?]))
            .filter(|target| !target.contains(' '));
        let code = text.len() >= 2 && text.starts_with('`') && text.ends_with('`');
        out.push_str(&rest[..start]);
        match target {
            Some(target) => {
                out.push_str(text);
                rest = &after[target.len() + 2..];
            }
            None if code => {
                out.push_str(text);
                rest = after;
            }
            None => {
                out.push('[');
                rest = &rest[start + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A fresh, empty directory for one run of this task.
fn scratch_dir(purpose: &str) -> Result<PathBuf, String> {
    let dir =
        std::env::temp_dir().join(format!("enshwick-header-{purpose}-{}", std::process::id()));
    let _stale = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    Ok(dir)
}

/// Copies the ABI module into `to` as a crate of its own, `abi.rs` as
/// `lib.rs` beside its submodules: cbindgen looks for the submodules of a
/// single source file next to it, and the rest of the API crate stays out of
/// the header.
fn copy_abi(root: &Path, to: &Path) -> Result<(), String> {
    let source = root.join(ABI_SOURCE);
    let copy = |from: &Path, name: &OsStr| {
        std::fs::copy(from, to.join(name))
            .map(drop)
            .map_err(|error| format!("{}: {error}", from.display()))
    };
    copy(&source, OsStr::new("lib.rs"))?;
    let modules = source.with_extension("");
    let entries =
        std::fs::read_dir(&modules).map_err(|error| format!("{}: {error}", modules.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("{}: {error}", modules.display()))?
            .path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            copy(&path, path.file_name().unwrap_or_default())?;
        }
    }
    Ok(())
}

/// The types of the golden file.
fn parse_layout(text: &str) -> Result<Vec<Shape>, String> {
    let mut shapes: Vec<Shape> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let bad = || format!("{LAYOUT}:{}: unreadable line {line:?}", number + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        if let Some(field) = line.strip_prefix("    ") {
            let words: Vec<&str> = field.split_whitespace().collect();
            let [name, "at", offset, "size", _] = words[..] else {
                return Err(bad());
            };
            let offset = offset.parse().map_err(|_| bad())?;
            shapes
                .last_mut()
                .ok_or_else(bad)?
                .fields
                .push((name.to_owned(), offset));
        } else {
            let [name, "size", size, "align", align] = words[..] else {
                return Err(bad());
            };
            shapes.push(Shape {
                name: name.to_owned(),
                size: size.parse().map_err(|_| bad())?,
                align: align.parse().map_err(|_| bad())?,
                fields: Vec::new(),
            });
        }
    }
    Ok(shapes)
}

/// A C file asserting every size, alignment and field offset of `shapes`
/// against `header`.
///
/// A transparent newtype is a `typedef` of its integer in C, so its single
/// field `0` has no name there and only its size and alignment hold.
fn asserts(shapes: &[Shape], header: &str) -> String {
    let mut c = "#include <stddef.h>\n#include \"enshwick.h\"\n".to_owned();
    for shape in shapes {
        let Shape {
            name, size, align, ..
        } = shape;
        let _ = writeln!(
            c,
            "_Static_assert(sizeof({PREFIX}{name}) == {size}, \"size of {name}\");\n\
             _Static_assert(_Alignof({PREFIX}{name}) == {align}, \"alignment of {name}\");"
        );
        let body = struct_body(header, name);
        for (field, offset) in shape.fields.iter().filter(|(field, _)| field != "0") {
            let member = c_member(&body, field);
            let _ = writeln!(
                c,
                "_Static_assert(offsetof({PREFIX}{name}, {member}) == {offset}, \"offset of {name}.{field}\");"
            );
        }
    }
    c
}

/// The member lines of the struct `name` in `header`, comments left out;
/// empty when the header has no such struct.
fn struct_body<'h>(header: &'h str, name: &str) -> Vec<&'h str> {
    let open = format!("typedef struct {PREFIX}{name} {{");
    let close = format!("}} {PREFIX}{name};");
    header
        .lines()
        .skip_while(|line| *line != open)
        .skip(1)
        .take_while(|line| *line != close)
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect()
}

/// The C name of `field` in a struct `body`: cbindgen appends `_` to a name
/// that is a C or C++ keyword, such as `template` or `new`.
fn c_member(body: &[&str], field: &str) -> String {
    let declares = |member: &str| {
        body.iter().any(|line| {
            line.match_indices(member).any(|(at, _)| {
                let before = line[..at].chars().last();
                let after = line[at + member.len()..].chars().next();
                !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                    && matches!(after, Some(';' | '[' | ')'))
            })
        })
    };
    let escaped = format!("{field}_");
    if !declares(field) && declares(&escaped) {
        escaped
    } else {
        field.to_owned()
    }
}

/// Compiles `source` against `header` for the Windows target with clang; a
/// failing assert names its type and field.
fn compile_asserts(header: &Path, source: &str) -> Vec<String> {
    let dir = match scratch_dir("layout") {
        Ok(dir) => dir,
        Err(error) => return vec![error],
    };
    let file = dir.join("layout.c");
    if let Err(error) = std::fs::write(&file, source) {
        return vec![format!("{}: {error}", file.display())];
    }
    let include = header.parent().unwrap_or(Path::new("."));
    let output = Command::new("clang")
        .args([
            &format!("--target={C_TARGET}"),
            "-std=c11",
            "-ffreestanding",
            "-fsyntax-only",
            "-Werror",
        ])
        .arg("-I")
        .arg(include)
        .arg(&file)
        .output();
    let _removed = std::fs::remove_dir_all(&dir);
    match output {
        Ok(output) if output.status.success() => Vec::new(),
        Ok(output) => String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.contains("error:"))
            .map(|line| format!("{HEADER} against {LAYOUT}: {line}"))
            .collect(),
        Err(error) => vec![format!(
            "running clang, which checks {HEADER} against {LAYOUT}: {error}; install LLVM's clang"
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = "Pair size 8 align 4\n    left at 0 size 4\n    right at 4 size 4\nStatus size 4 align 4\n    0 at 0 size 4\n";

    #[test]
    fn the_layout_file_reads_into_shapes() {
        let shapes = parse_layout(GOLDEN).unwrap();
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].name, "Pair");
        assert_eq!(
            shapes[0].fields,
            [("left".to_owned(), 0), ("right".to_owned(), 4)]
        );
        assert_eq!((shapes[1].size, shapes[1].align), (4, 4));
    }

    #[test]
    fn a_malformed_layout_line_is_named() {
        let error = parse_layout("Pair size 8 align 4\nPair size eight align 4\n").unwrap_err();
        assert!(error.contains(":2:"), "{error}");
    }

    #[test]
    fn the_abi_version_follows_the_includes_as_the_layout_s_hash() {
        let header = "#include <stdint.h>\n\ntypedef int A;\n";
        let with = with_abi_version(header, GOLDEN);
        let define = format!(
            "#define ENSHWICK_ABI_VERSION {:#010x}",
            layout_version(GOLDEN)
        );
        assert!(with.starts_with("#include <stdint.h>\n\n// "), "{with}");
        assert!(
            with.contains(&format!("\n{define}\n\ntypedef int A;\n")),
            "{with}"
        );
        assert_ne!(
            layout_version(GOLDEN),
            layout_version(&GOLDEN.replace('4', "8"))
        );
        assert_eq!(
            layout_version(GOLDEN),
            layout_version(&GOLDEN.replace('\n', "\r\n"))
        );
    }

    #[test]
    fn asserts_cover_sizes_alignments_and_named_fields_only() {
        let header = "typedef struct EnshwickPair {\n  // The left.\n  int left;\n  int right;\n} EnshwickPair;\n";
        let c = asserts(&parse_layout(GOLDEN).unwrap(), header);
        assert!(c.contains("sizeof(EnshwickPair) == 8"));
        assert!(c.contains("_Alignof(EnshwickPair) == 4"));
        assert!(c.contains("offsetof(EnshwickPair, right) == 4"));
        assert!(c.contains("sizeof(EnshwickStatus) == 4"));
        assert!(!c.contains("offsetof(EnshwickStatus"));
    }

    #[test]
    fn a_member_cbindgen_escaped_is_named_as_escaped() {
        let body = vec![
            "  uint32_t template_;",
            "  uint32_t newer;",
            "  Status (*new_)(int a);",
        ];
        assert_eq!(c_member(&body, "template"), "template_");
        assert_eq!(c_member(&body, "new"), "new_");
        assert_eq!(c_member(&body, "newer"), "newer");
    }

    #[test]
    fn rustdoc_links_become_plain_code() {
        assert_eq!(
            plain_links("// the [`Header`] of [`HostApi::migrate`](super::HostApi::migrate)."),
            "// the `Header` of `HostApi::migrate`."
        );
        assert_eq!(plain_links("// a [`dangling"), "// a [`dangling");
        assert_eq!(
            plain_links("// see [actions](HostApi::actions)."),
            "// see actions."
        );
        assert_eq!(
            plain_links("// levels[0] and [x] stay"),
            "// levels[0] and [x] stay"
        );
    }

    #[test]
    fn examples_sections_are_left_out_of_comments() {
        let header =
            "// A header.\n//\n// # Examples\n//\n// ```\n// rust();\n// ```\ntypedef int A;\n";
        assert_eq!(tidy(header), "// A header.\ntypedef int A;\n");
    }

    #[test]
    fn capital_constants_take_the_c_prefix_everywhere() {
        let header = "#define EnshwickMAX_ZONES 4\n#define EnshwickStatus_OK 0\nint zones[EnshwickMAX_ZONES];\nint EnshwickMAX_ZONES_LEFT;\n";
        assert_eq!(
            tidy(header),
            "#define ENSHWICK_MAX_ZONES 4\n#define EnshwickStatus_OK 0\nint zones[ENSHWICK_MAX_ZONES];\nint EnshwickMAX_ZONES_LEFT;\n"
        );
    }
}
