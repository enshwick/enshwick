//! The C header of the ABI, `enshwick.h`, written by cbindgen from the ABI
//! module and held to the golden layout file: every size, alignment and
//! field offset there is a static assert at the header's end, so a unit that
//! includes it with another layout fails to compile.
//!
//! The types the header declares are the types the golden file lists, so a
//! new ABI type reaches the header without a change here.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::check::rust_files;
use crate::scratch::Scratch;

/// The package the header describes.
pub(crate) const PACKAGE: &str = "enshwick";
/// The ABI module cbindgen parses, relative to the workspace root.
const ABI_SOURCE: &str = "crates/internal/abi/src/abi.rs";
/// The ABI types the ABI module re-exports from the crate of the value
/// types, which cbindgen parses beside it.
const VALUES_ABI_SOURCE: &str = "crates/internal/values/src/abi.rs";
/// The name of that file's copy beside the ABI module's.
const VALUES_ABI_COPY: &str = "values_abi.rs";
/// The golden layout file.
pub(crate) const LAYOUT: &str = "crates/internal/abi/src/abi/layout.txt";
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
/// stale against the ABI module, or apart from the golden layout in C or C++.
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
    let comments: Vec<String> = rust_in_comments(&fresh)
        .into_iter()
        .chain(callbacks_without_thread(&fresh))
        .collect();
    if !comments.is_empty() {
        return comments;
    }
    let header = root.join(HEADER);
    [
        (Language::C, Packing::Default),
        (Language::Cpp, Packing::Default),
        (Language::C, Packing::One),
    ]
    .into_iter()
    .flat_map(|(language, packing)| compile(&header, language, packing, INCLUDE))
    .collect()
}

/// A unit that only includes the header, whose own asserts hold its layout.
const INCLUDE: &str = "#include \"enshwick.h\"\n";

/// The default packing a check compiles with.
#[derive(Debug, Clone, Copy)]
enum Packing {
    /// The compiler's own.
    Default,
    /// One byte, as `/Zp1` sets it; the header's `#pragma pack` must undo it.
    One,
}

/// The header cbindgen writes for the ABI module.
fn generate(root: &Path) -> Result<String, String> {
    let layout =
        std::fs::read_to_string(root.join(LAYOUT)).map_err(|error| format!("{LAYOUT}: {error}"))?;
    let shapes = parse_layout(&layout)?;
    let config = cbindgen::Config::from_file(root.join(CONFIG))
        .map_err(|error| format!("{CONFIG}: {error}"))?;
    let crate_root = Scratch::new("header-abi")?;
    let copied = copy_abi(root, &crate_root);
    let mut builder = cbindgen::Builder::new()
        .with_config(config)
        .with_item_prefix(PREFIX)
        .with_src(crate_root.join("lib.rs"))
        .with_src(crate_root.join(VALUES_ABI_COPY));
    for shape in &shapes {
        builder = builder.include_item(&shape.name);
    }
    // Reached only through an `Option`, which cbindgen does not follow.
    for callback in callback_types(root)? {
        builder = builder.include_item(&callback);
    }
    let bindings = copied.and_then(|()| {
        builder
            .generate()
            .map_err(|error| format!("cbindgen: {error}"))
    });
    drop(crate_root);
    let bindings = bindings?;
    let mut bytes = Vec::new();
    bindings.write(&mut bytes);
    let header =
        String::from_utf8(bytes).map_err(|error| format!("cbindgen wrote no UTF-8: {error}"))?;
    let mut types: Vec<String> = shapes.iter().map(|shape| shape.name.clone()).collect();
    types.extend(callback_types(root)?);
    let fields = shapes.iter().flat_map(|shape| {
        shape
            .fields
            .iter()
            .map(move |(field, _)| format!("{}::{field}", shape.name))
    });
    let known: Vec<String> = types.iter().cloned().chain(fields).collect();
    let header = cpp_str_pointer(&c_paths(&tidy(&header), &known));
    let asserts = asserts(&shapes, &header);
    let at = header
        .rfind(TRAILER)
        .ok_or_else(|| format!("{CONFIG}: the trailer has no `{TRAILER}`"))?;
    Ok(format!("{}{asserts}\n{}", &header[..at], &header[at..]))
}

/// `header` with `EnshwickStr`'s pointer typed `const char *` in C++, the same
/// layout, so `ENSHWICK_STR_INIT` needs no cast there and stays a constant
/// expression.
fn cpp_str_pointer(header: &str) -> String {
    let c = "  const uint8_t *ptr;";
    let open = format!("typedef struct {PREFIX}Str {{\n{c}\n");
    header.replacen(
        &open,
        &format!(
            "typedef struct {PREFIX}Str {{\n#ifdef __cplusplus\n  const char *ptr;\n#else\n{c}\n#endif\n"
        ),
        1,
    )
}

/// The header as C authors read it: rustdoc's `# Examples` sections, which
/// show Rust, left out, links reduced to their text, and constants spelled
/// `ENSHWICK_NAME` rather than `EnshwickNAME`.
fn tidy(header: &str) -> String {
    let header = nullable_callbacks(header);
    let mut lines: Vec<String> = Vec::new();
    let mut in_examples = false;
    for line in header.lines() {
        let comment = line.trim_start().strip_prefix("//").map(str::trim);
        if in_examples && comment.is_some() {
            continue;
        }
        // A rustdoc link definition, `[text]: path`, whose links are plain text here.
        if comment.is_some_and(|text| {
            text.starts_with('[')
                && text
                    .split_once("]: ")
                    .is_some_and(|(_, target)| !target.contains(' '))
        }) {
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

/// The folder of the ABI module's submodules.
fn abi_modules(root: &Path) -> PathBuf {
    root.join(ABI_SOURCE).with_extension("")
}

/// The Rust files under `modules`, a domain's folder (`events/knowledge.rs`)
/// included, in path order.
fn abi_files(modules: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    rust_files(modules, &mut files)?;
    files.sort();
    Ok(files)
}

/// The callback types of the ABI module, every `pub type <Name>Fn`.
fn callback_types(root: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for path in abi_files(&abi_modules(root))? {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        found.extend(text.lines().filter_map(|line| {
            let name = line.strip_prefix("pub type ")?.split([' ', '=']).next()?;
            name.ends_with("Fn").then(|| name.to_owned())
        }));
    }
    found.sort();
    Ok(found)
}

/// `header` with every `Option` of a callback type as the callback type
/// itself: cbindgen writes `Option<ActionFn>` as an opaque struct
/// `EnshwickOption_ActionFn`, while a C function pointer is nullable already
/// and has the same layout as Rust's `Option` of one.
fn nullable_callbacks(header: &str) -> String {
    let opaque = format!("typedef struct {PREFIX}Option_");
    let mut out = String::with_capacity(header.len());
    let mut skip_blank = false;
    for line in header.lines() {
        if line.starts_with(&opaque) {
            skip_blank = true;
            continue;
        }
        if skip_blank && line.is_empty() {
            skip_blank = false;
            continue;
        }
        skip_blank = false;
        out.push_str(line);
        out.push('\n');
    }
    let out = out
        .replace(&format!("struct {PREFIX}Option_"), PREFIX)
        .replace(&format!("{PREFIX}Option_"), PREFIX);
    structs_after_callbacks(&out)
}

/// Where cbindgen's trailer starts, after every declaration: the end of the
/// header's packing, which the layout asserts come right before.
const TRAILER: &str = "#pragma pack(pop)";

/// `header` with every struct that names a callback type moved, with its
/// comment, after the callback types: cbindgen orders by what it sees a
/// struct use, and it sees no use through an `Option`.
fn structs_after_callbacks(header: &str) -> String {
    let lines: Vec<&str> = header.lines().collect();
    let names_callback = |line: &str| {
        line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|word| word.starts_with(PREFIX) && word.ends_with("Fn") && word != PREFIX)
    };
    let mut moved: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let Some(name) = lines[at]
            .strip_prefix("typedef struct ")
            .and_then(|rest| rest.strip_suffix(" {"))
        else {
            at += 1;
            continue;
        };
        let close = format!("}} {name};");
        let Some(end) = lines[at..].iter().position(|line| *line == close) else {
            at += 1;
            continue;
        };
        let end = at + end;
        if lines[at + 1..end].iter().any(|line| names_callback(line)) {
            let mut start = at;
            while start > 0 && lines[start - 1].trim_start().starts_with("//") {
                start -= 1;
            }
            moved.push((start, end));
        }
        at = end + 1;
    }
    let Some(trailer) = lines.iter().position(|line| *line == TRAILER) else {
        return header.to_owned();
    };
    let inside = |index: usize| {
        moved
            .iter()
            .any(|&(start, end)| (start..=end).contains(&index))
    };
    let mut out = String::with_capacity(header.len());
    for (index, line) in lines.iter().enumerate() {
        if index == trailer {
            for &(start, end) in &moved {
                for kept in &lines[start..=end] {
                    out.push_str(kept);
                    out.push('\n');
                }
                out.push('\n');
            }
        }
        if !inside(index) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
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

/// `header` with the Rust paths in its comments' code spans spelled as C
/// names, for each of `types`: `Status::OK` as `EnshwickStatus_OK`,
/// `HostApi::log` as `EnshwickHostApi.log`, `HookSpec` as `EnshwickHookSpec`,
/// and `u32::MAX` as `UINT32_MAX`.
fn c_paths(header: &str, types: &[String]) -> String {
    let mut out = String::with_capacity(header.len());
    for line in header.lines() {
        if line.trim_start().starts_with("//") {
            for (index, part) in line.split('`').enumerate() {
                if index > 0 {
                    out.push('`');
                }
                if index % 2 == 1 {
                    out.push_str(&c_span(part, types));
                } else {
                    out.push_str(part);
                }
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// One code span of a comment with each Rust path in it spelled in C.
fn c_span(span: &str, types: &[String]) -> String {
    if let Some(name) = span
        .strip_prefix("size_of::<")
        .and_then(|rest| rest.strip_suffix(">()"))
    {
        return format!("sizeof({})", c_path(name, types));
    }
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(span.len());
    let mut rest = span;
    while let Some(start) = rest.find(|c: char| c.is_ascii_alphabetic() || c == '_') {
        if rest[..start].ends_with(word) {
            out.push_str(&rest[..=start]);
            rest = &rest[start + 1..];
            continue;
        }
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let mut end = rest.find(|c: char| !word(c)).unwrap_or(rest.len());
        while rest[end..].starts_with("::")
            && rest[end + 2..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        {
            end += 2 + rest[end + 2..]
                .find(|c: char| !word(c))
                .unwrap_or(rest.len() - end - 2);
        }
        out.push_str(&c_path(&rest[..end], types));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// A Rust path of one or two segments as C names it, or as it is; `known`
/// holds each type and each field as `Type::field`, so a method, which C
/// lacks, stays a Rust path that [`rust_in_comments`] names.
fn c_path(path: &str, known: &[String]) -> String {
    let known = |name: &str| known.iter().any(|known| known == name);
    let shouted = |name: &str| {
        name.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    };
    match path.split_once("::") {
        None if known(path) => format!("{PREFIX}{path}"),
        Some((owner, item)) if known(owner) && shouted(item) => format!("{PREFIX}{owner}_{item}"),
        Some((owner, item)) if known(path) => format!("{PREFIX}{owner}.{item}"),
        Some(("usize", "MAX")) => "SIZE_MAX".to_owned(),
        Some((
            integer @ ("u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64"),
            bound @ ("MAX" | "MIN"),
        )) => {
            let (sign, bits) = integer.split_at(1);
            format!("{}INT{bits}_{bound}", if sign == "u" { "U" } else { "" })
        }
        _ => path.to_owned(),
    }
}

/// The comment lines of `header` that still show Rust: a path with `::` or a
/// macro call.
fn rust_in_comments(header: &str) -> Vec<String> {
    header
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            line.trim_start().starts_with("//") && (line.contains("::") || line.contains("!`"))
        })
        .map(|(index, line)| {
            format!(
                "{HEADER}:{}: a comment shows Rust, not C: {}",
                index + 1,
                line.trim()
            )
        })
        .collect()
}

/// The callback types of `header` whose comment names no thread, which
/// every callback states.
fn callbacks_without_thread(header: &str) -> Vec<String> {
    let mut comment = String::new();
    let mut problems = Vec::new();
    for line in header.lines() {
        if let Some(text) = line.trim_start().strip_prefix("//") {
            comment.push_str(text);
            continue;
        }
        let callback = line
            .strip_prefix("typedef ")
            .and_then(|rest| rest.split_once(&format!("(*{PREFIX}")))
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(name, _)| name)
            .filter(|name| name.ends_with("Fn"));
        if let Some(name) = callback
            && !comment.contains("thread")
        {
            problems.push(format!(
                "{HEADER}: the comment of {PREFIX}{name} names no thread it runs on"
            ));
        }
        comment.clear();
    }
    problems
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

/// `source` with the brackets of its `cbindgen:` annotation lines
/// unescaped: the source escapes them (`field-names=\[bytes\]`) so rustdoc
/// reads no link.
fn cbindgen_annotations(source: &str) -> String {
    source
        .split_inclusive('\n')
        .map(|line| {
            if line.trim_start().starts_with("/// cbindgen:") {
                line.replace("\\[", "[").replace("\\]", "]")
            } else {
                line.to_owned()
            }
        })
        .collect()
}

/// Copies the ABI module into `to` as a crate of its own, `abi.rs` as
/// `lib.rs` beside the value types' ABI file and its submodules, each at its
/// path in the module's folder: cbindgen looks for the submodules of a
/// single source file next to it, and those of `events.rs` in `events/`,
/// and the rest of the API crate stays out of the header.
fn copy_abi(root: &Path, to: &Path) -> Result<(), String> {
    let copy = |from: &Path, name: &Path| {
        let into = to.join(name);
        into.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::read_to_string(from))
            .and_then(|text| std::fs::write(&into, cbindgen_annotations(&text)))
            .map_err(|error| format!("{}: {error}", from.display()))
    };
    copy(&root.join(ABI_SOURCE), Path::new("lib.rs"))?;
    copy(&root.join(VALUES_ABI_SOURCE), Path::new(VALUES_ABI_COPY))?;
    let modules = abi_modules(root);
    for path in abi_files(&modules)? {
        let name = path
            .strip_prefix(&modules)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        copy(&path, name)?;
    }
    Ok(())
}

/// The types of the golden file, after its `abi <major>.<minor>` line.
fn parse_layout(text: &str) -> Result<Vec<Shape>, String> {
    let mut shapes: Vec<Shape> = Vec::new();
    for (number, line) in text.lines().enumerate().skip(1) {
        let bad = || format!("{LAYOUT}:{}: unreadable line {line:?}", number + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        if line.starts_with("const ") {
            continue;
        }
        if let Some(field) = line.strip_prefix("    ") {
            let (place, _type) = field.split_once(" type ").ok_or_else(bad)?;
            let words: Vec<&str> = place.split_whitespace().collect();
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
            let ([name, "size", size, "align", align]
            | [name, "size", size, "align", align, "born", _]) = words[..]
            else {
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

/// The fields of `HostApi` in the golden file `text`, in the table's order.
pub(crate) fn host_api_fields(text: &str) -> Result<Vec<String>, String> {
    parse_layout(text)?
        .into_iter()
        .find(|shape| shape.name == "HostApi")
        .map(|host| host.fields.into_iter().map(|(name, _)| name).collect())
        .ok_or_else(|| format!("{LAYOUT}: no HostApi"))
}

/// The header's closing asserts, in C and C++ alike, of every size, alignment
/// and field offset of `shapes`, whose structs `header` declares.
///
/// A transparent newtype is a `typedef` of its integer in C, so its single
/// field `0` has no name there and only its size and alignment hold.
fn asserts(shapes: &[Shape], header: &str) -> String {
    let mut c =
        "// The layout the loader was built with, from its golden layout file.\n".to_owned();
    for shape in shapes {
        let Shape {
            name, size, align, ..
        } = shape;
        let _ = writeln!(
            c,
            "ENSHWICK_STATIC_ASSERT(sizeof({PREFIX}{name}) == {size}, \"size of {name}\");\n\
             ENSHWICK_STATIC_ASSERT(ENSHWICK_ALIGNOF({PREFIX}{name}) == {align}, \"alignment of {name}\");"
        );
        let body = struct_body(header, name);
        for (field, offset) in shape.fields.iter().filter(|(field, _)| field != "0") {
            let member = c_member(&body, field);
            let _ = writeln!(
                c,
                "ENSHWICK_STATIC_ASSERT(offsetof({PREFIX}{name}, {member}) == {offset}, \"offset of {name}.{field}\");"
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

/// The language a check compiles the header as.
#[derive(Debug, Clone, Copy)]
enum Language {
    /// C11.
    C,
    /// C++20, which mods in C++ include the header from.
    Cpp,
}

/// Compiles `source` against `header` for the Windows target with clang as
/// `language` under `packing`; a failing layout assert names its type and
/// field.
fn compile(header: &Path, language: Language, packing: Packing, source: &str) -> Vec<String> {
    let dir = match Scratch::new("header-layout") {
        Ok(dir) => dir,
        Err(error) => return vec![error],
    };
    let (file_name, standard, language_name) = match language {
        Language::C => ("layout.c", "-std=c11", "C"),
        Language::Cpp => ("layout.cpp", "-std=c++20", "C++"),
    };
    let (pack, against) = match packing {
        Packing::Default => (None, language_name.to_owned()),
        Packing::One => (
            Some("-fpack-struct=1"),
            format!("{language_name} packed to one byte"),
        ),
    };
    let file = dir.join(file_name);
    if let Err(error) = std::fs::write(&file, source) {
        return vec![format!("{}: {error}", file.display())];
    }
    let include = header.parent().unwrap_or(Path::new("."));
    let output = Command::new("clang")
        .env("PATH", crate::tool_path())
        .args([
            &format!("--target={C_TARGET}"),
            standard,
            "-ffreestanding",
            "-fsyntax-only",
            "-Werror",
        ])
        .args(pack)
        .arg("-I")
        .arg(include)
        .arg(&file)
        .output();
    drop(dir);
    match output {
        Ok(output) if output.status.success() => Vec::new(),
        Ok(output) => failures(
            &against,
            &output.status.to_string(),
            &String::from_utf8_lossy(&output.stderr),
        ),
        Err(error) => vec![format!(
            "running clang, which checks {HEADER} against {LAYOUT} in {against}: {error}; install LLVM's clang and put its bin folder on PATH"
        )],
    }
}

/// The problems of a clang run in `against` that ended with `status`: each
/// error line of `stderr`, or one line naming `status` when it has none.
fn failures(against: &str, status: &str, stderr: &str) -> Vec<String> {
    let errors: Vec<String> = stderr
        .lines()
        .filter(|line| line.contains("error:"))
        .map(|line| format!("{HEADER} against {LAYOUT} in {against}: {line}"))
        .collect();
    if errors.is_empty() {
        vec![format!(
            "{HEADER} against {LAYOUT} in {against}: clang failed with {status} and named no error"
        )]
    } else {
        errors
    }
}

#[cfg(test)]
mod tests;
