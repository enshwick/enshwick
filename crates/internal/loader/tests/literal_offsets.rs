//! Keeps literal offsets into game memory from growing outside `src/pinned/`:
//! an offset comes from reflection or the build table, so a game update
//! carries it. `literal_offsets.txt` lists how many each file still holds.
//!
//! Three spellings count: a `usize` constant spelled in hexadecimal, a number
//! written into the offset argument of a game memory accessor
//! (`read_at::<u32>(base, 0x10)`, `write(slot, at + 4, v)`), and a constant
//! spelled in decimal that such an argument names (`read(slot, SLOT_COUNT)`).

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    /// What a constant or a call that is no offset into game memory carries at
    /// its end, followed by why: a sanity bound, room the loader keeps, a fact
    /// of Windows or of the PE format, a test's own memory.
    const BOUND: &str = "// bound, not a game offset:";

    /// Functions that read or write game memory at a base and an offset, the
    /// offset being their second argument: the loader's `memory` module, the
    /// API crate's `raw` module, and the `field` readers of bytes copied out
    /// of game memory.
    const ACCESSORS: &[&str] = &[
        "read",
        "read_ptr",
        "read_bytes",
        "write",
        "read_at",
        "read_ptr_at",
        "read_bytes_at",
        "read_u32_atomic_at",
        "write_at",
        "write_ptr_at",
        "write_bytes_at",
        "field",
    ];

    /// The paths an accessor is called through; any other path (`fs::write`,
    /// `ptr::read`) is not game memory.
    const ACCESSOR_PATHS: &[&str] = &["", "memory::", "raw::"];

    /// Whether `line` carries the bound marker followed by a reason.
    fn is_marked_bound(line: &str) -> bool {
        line.split_once(BOUND)
            .is_some_and(|(_, why)| !why.trim().is_empty())
    }

    /// Whether `line` declares a `usize` constant, or an array of them, spelled
    /// as a hexadecimal literal, and is not marked a bound with its reason.
    fn is_literal_offset(line: &str) -> bool {
        if is_marked_bound(line) {
            return false;
        }
        let Some((_, ty_and_value)) = usize_const(line) else {
            return false;
        };
        (ty_and_value.starts_with("usize = 0x"))
            || (ty_and_value.starts_with("[usize;") && ty_and_value.contains("= [0x"))
    }

    /// The name and the `<type> = <value>` rest of a constant declared on
    /// `line`.
    fn usize_const(line: &str) -> Option<(&str, &str)> {
        let line = line.trim_start();
        let line = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub(super) "))
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        let rest = line.strip_prefix("const ")?;
        let (name, ty_and_value) = rest.split_once(':')?;
        Some((name.trim(), ty_and_value.trim_start()))
    }

    /// The name of a `usize` constant declared on `line` with a decimal value,
    /// unless the line is marked a bound.
    fn decimal_const(line: &str) -> Option<&str> {
        if is_marked_bound(line) {
            return None;
        }
        let (name, ty_and_value) = usize_const(line)?;
        let value = ty_and_value.strip_prefix("usize = ")?;
        let value = value.trim_end().trim_end_matches(';');
        (value.chars().all(|c| c.is_ascii_digit() || c == '_')
            && value.starts_with(|c: char| c.is_ascii_digit()))
        .then_some(name)
    }

    fn is_ident(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    /// Whether `arg` holds a number that places a field, other than a lone
    /// `0`: `0x10`, `at + 4`, `i * 24`. The size of a primitive element as a
    /// stride (`i * 8`, `4 * i`) and a shift or modulus (`i >> 6`, `j % 64`)
    /// place no field.
    fn holds_number(arg: &str) -> bool {
        if arg.trim() == "0" {
            return false;
        }
        let chars: Vec<char> = arg.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if !chars[i].is_ascii_digit() || (i > 0 && is_ident(chars[i - 1])) {
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            let number: String = chars[start..i].iter().collect();
            let before: String = chars[..start].iter().collect();
            let before = before.trim_end();
            let after: String = chars[i..].iter().collect();
            let after = after.trim_start();
            if before.ends_with(">>") || before.ends_with("<<") || before.ends_with('%') {
                continue;
            }
            let stride = before.ends_with('*') || after.starts_with('*');
            if stride && matches!(number.as_str(), "1" | "2" | "4" | "8") {
                continue;
            }
            return true;
        }
        false
    }

    /// The identifiers in `arg`.
    fn idents(arg: &str) -> impl Iterator<Item = &str> {
        arg.split(|c: char| !is_ident(c))
            .filter(|w| w.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
    }

    /// The top-level arguments of the call whose `(` sits at `open` in
    /// `code`, and the index past its `)`.
    fn call_args(code: &[char], open: usize) -> Option<(Vec<String>, usize)> {
        let mut depth = 0usize;
        let mut args = Vec::new();
        let mut current = String::new();
        for (i, &c) in code.iter().enumerate().skip(open) {
            match c {
                '(' | '[' | '{' => {
                    depth += 1;
                    if depth == 1 {
                        continue;
                    }
                }
                ')' | ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        args.push(std::mem::take(&mut current));
                        return Some((args, i + 1));
                    }
                }
                ',' if depth == 1 => {
                    args.push(std::mem::take(&mut current));
                    continue;
                }
                _ => {}
            }
            current.push(c);
        }
        None
    }

    /// The offset arguments of every accessor call in `text`, each with
    /// whether the call's lines carry the bound marker.
    fn offset_args(text: &str) -> Vec<(String, bool)> {
        let lines: Vec<&str> = text.lines().collect();
        let mut code = Vec::new();
        let mut line_of = Vec::new();
        for (n, line) in lines.iter().enumerate() {
            let code_part = line.split_once("//").map_or(*line, |(c, _)| c);
            for c in code_part.chars().chain(std::iter::once('\n')) {
                code.push(c);
                line_of.push(n);
            }
        }
        let mut found = Vec::new();
        let mut i = 0;
        while i < code.len() {
            if !is_ident(code[i]) || (i > 0 && is_ident(code[i - 1])) {
                i += 1;
                continue;
            }
            let start = i;
            while i < code.len() && is_ident(code[i]) {
                i += 1;
            }
            let name: String = code[start..i].iter().collect();
            if !ACCESSORS.contains(&name.as_str()) {
                continue;
            }
            let before: String = code[..start].iter().collect();
            let before = before.trim_end_matches(' ');
            if before.ends_with('.') || before.ends_with("fn") {
                continue;
            }
            let path = before
                .rsplit(|c: char| !(is_ident(c) || c == ':'))
                .next()
                .unwrap_or("");
            let path = path.strip_prefix("crate::").unwrap_or(path);
            let path = path.strip_prefix("enshwick::").unwrap_or(path);
            if !ACCESSOR_PATHS.contains(&path) {
                continue;
            }
            let mut j = i;
            if code[j..].starts_with(&[':', ':', '<']) {
                let mut angle = 0usize;
                while j < code.len() {
                    match code[j] {
                        '<' => angle += 1,
                        '>' => {
                            angle -= 1;
                            if angle == 0 {
                                j += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            while j < code.len() && code[j].is_whitespace() {
                j += 1;
            }
            if code.get(j) != Some(&'(') {
                continue;
            }
            let Some((args, end)) = call_args(&code, j) else {
                continue;
            };
            // A reference or a string is an argument of a function that only
            // shares an accessor's name, never an offset.
            if let Some(offset) = args.get(1).map(|a| a.trim())
                && !offset.starts_with(['&', '"'])
                && !args[0].trim().starts_with('"')
            {
                let marked = (line_of[start]..=line_of[end - 1]).any(|n| is_marked_bound(lines[n]));
                found.push((offset.to_owned(), marked));
            }
            i = j + 1;
        }
        found
    }

    /// Literal offsets in one file's `text`: hexadecimal `usize` constants,
    /// numbers in accessor offsets, and decimal constants those offsets name.
    fn count_in(text: &str) -> usize {
        findings(text).len()
    }

    /// Each literal offset in `text`, as the constant's name or the offset
    /// argument that holds it.
    fn findings(text: &str) -> Vec<String> {
        let mut found: Vec<String> = text
            .lines()
            .filter(|l| is_literal_offset(l))
            .filter_map(usize_const)
            .map(|(name, _)| format!("const {name}"))
            .collect();
        let decimal: BTreeSet<&str> = text.lines().filter_map(decimal_const).collect();
        let mut named = BTreeSet::new();
        for (offset, marked) in offset_args(text) {
            if marked {
                continue;
            }
            if holds_number(&offset) {
                found.push(format!("offset `{offset}`"));
            }
            for ident in idents(&offset) {
                if let Some(name) = decimal.get(ident) {
                    named.insert(*name);
                }
            }
        }
        found.extend(named.into_iter().map(|name| format!("const {name}")));
        found
    }

    /// Literal offsets per source file under `src`, outside `src/pinned/`.
    fn counts(src: &Path) -> BTreeMap<String, Vec<String>> {
        let mut found = BTreeMap::new();
        let mut dirs = vec![src.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path != src.join("pinned") {
                        dirs.push(path);
                    }
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let count = findings(&std::fs::read_to_string(&path).unwrap());
                if !count.is_empty() {
                    let name = path
                        .strip_prefix(src.parent().unwrap())
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    found.insert(name, count);
                }
            }
        }
        found
    }

    /// The counts `literal_offsets.txt` allows: `<count> <path>` per line.
    fn allowed(list: &str) -> BTreeMap<String, usize> {
        list.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| {
                let (count, path) = l.split_once(' ').unwrap();
                (path.trim().to_owned(), count.parse().unwrap())
            })
            .collect()
    }

    #[test]
    fn a_literal_offset_is_spotted_however_it_is_declared() {
        assert!(is_literal_offset("const LIST_COUNT: usize = 0x40;"));
        assert!(is_literal_offset(
            "    pub(super) const BEDS: [usize; 3] = [0x114, 0x118, 0x11c];"
        ));
        assert!(is_literal_offset("pub(crate) const A: usize = 0x0;"));
        assert!(!is_literal_offset("const PREFIX: usize = 16;"));
        assert!(!is_literal_offset("const MASK: u32 = 0xff;"));
        assert!(!is_literal_offset("let x: usize = 0x40;"));
    }

    #[test]
    fn a_bound_with_its_reason_is_not_counted_and_a_bare_marker_is() {
        assert!(!is_literal_offset(
            "const ENTRY_MAX: usize = 0x1000; // bound, not a game offset: far past any entry"
        ));
        assert!(is_literal_offset(
            "const ENTRY_MAX: usize = 0x1000; // bound, not a game offset:"
        ));
        assert!(is_literal_offset(
            "const ENTRY_MAX: usize = 0x1000; // bound, not a game offset:   "
        ));
        assert!(is_literal_offset(
            "const ENTRY_MAX: usize = 0x1000; // not a game offset: far past any entry"
        ));
    }

    #[test]
    fn a_number_in_an_accessor_offset_counts_once_per_call() {
        assert_eq!(count_in("unsafe { write_at(out, 0x10, 0u32) };"), 1);
        assert_eq!(
            count_in("let v = unsafe { read_at::<u8>(out.as_ptr().cast(), 0xc) } != 0;"),
            1
        );
        assert_eq!(
            count_in("let v: u32 = unsafe { crate::memory::read(slot, at + 4) };"),
            1
        );
        assert_eq!(
            count_in(
                "let v = unsafe {\n    enshwick::raw::read_ptr_at::<u8>(\n        base,\n        \
                 0x28,\n    )\n};"
            ),
            1
        );
    }

    #[test]
    fn a_zero_offset_or_a_number_outside_the_offset_does_not_count() {
        assert_eq!(count_in("unsafe { write_at(obj, 0, 0x7000_usize) };"), 0);
        assert_eq!(
            count_in("unsafe { memory::write(base, AT, 0xdead_u32) };"),
            0
        );
        assert_eq!(count_in("std::fs::write(path, [0xff, 0xfe]).unwrap();"), 0);
        assert_eq!(count_in("let n = input.read(&mut buf[4..])?;"), 0);
        assert_eq!(
            count_in("pub(crate) unsafe fn read(base: usize, offset: usize) {}"),
            0
        );
        assert_eq!(count_in("// read_at(base, 0x10) in a comment"), 0);
        assert_eq!(count_in("read(loader, &ReadBox::new(at, [1; 3], 1)?)"), 0);
        assert_eq!(count_in("let k = field(\"key\", 2);"), 0);
    }

    #[test]
    fn a_primitive_stride_or_a_shift_places_no_field_and_a_record_stride_does() {
        assert_eq!(
            count_in("let w = unsafe { read_at::<u64>(bits, i * 8) };"),
            0
        );
        assert_eq!(
            count_in("let l: u32 = unsafe { read_at(f, at + 4 * i) };"),
            0
        );
        assert_eq!(
            count_in("let m: u64 = unsafe { read_at(b, (i >> 6) * 8) };"),
            0
        );
        assert_eq!(count_in("unsafe { write_at(records, head * 24, r) };"), 1);
        assert_eq!(
            count_in("let v: u8 = unsafe { read(read_ptr(o, 0x18), 0x20) };"),
            2
        );
        assert_eq!(
            count_in("let c: u32 = unsafe { read(b, bucket * 8 + 4) };"),
            1
        );
    }

    #[test]
    fn a_decimal_constant_counts_once_when_an_accessor_offset_names_it() {
        let text = "const SLOT_ITEM: usize = 0;\nconst SLOT_COUNT: usize = 4;\n\
                    const LIMIT: usize = 16;\n\
                    let a: u32 = unsafe { memory::read(slot, SLOT_ITEM) };\n\
                    let b: u16 = unsafe { memory::read(slot, SLOT_COUNT) };\n\
                    unsafe { memory::write(slot, SLOT_COUNT, b) };\n\
                    let c = LIMIT;";
        assert_eq!(count_in(text), 2);
    }

    #[test]
    fn a_marked_call_or_decimal_constant_is_not_counted() {
        assert_eq!(
            count_in(
                "unsafe { write_at(buf, 8, 1u32) }; // bound, not a game offset: the test's \
                 own buffer"
            ),
            0
        );
        assert_eq!(
            count_in(
                "const AT: usize = 4; // bound, not a game offset: the test's own buffer\n\
                 let v: u32 = unsafe { memory::read(base, AT) };"
            ),
            0
        );
    }

    #[test]
    fn no_file_holds_more_literal_offsets_than_its_list_allows() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let found = counts(&root.join("src"));
        let list = std::fs::read_to_string(root.join("tests/literal_offsets.txt")).unwrap();
        let allowed = allowed(&list);
        let mut wrong = Vec::new();
        for (path, held) in &found {
            let count = held.len();
            let allows = allowed.get(path).copied().unwrap_or(0);
            if count > allows {
                wrong.push(format!(
                    "{path}: {count} literal offsets, {allows} allowed; take each new one from \
                     reflection or the build table (src/pinned/): {}",
                    held.join(", ")
                ));
            }
        }
        for (path, &allows) in &allowed {
            let count = found.get(path).map_or(0, Vec::len);
            if count < allows {
                wrong.push(format!(
                    "{path}: {count} literal offsets, {allows} allowed; lower its count in \
                     tests/literal_offsets.txt"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
