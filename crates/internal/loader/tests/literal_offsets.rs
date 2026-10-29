//! Keeps literal offsets into game memory from growing outside `src/pinned/`:
//! an offset comes from reflection or the build table, so a game update
//! carries it. `literal_offsets.txt` lists how many each file still holds.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    /// Whether `line` declares a `usize` constant, or an array of them, spelled
    /// as a hexadecimal literal.
    fn is_literal_offset(line: &str) -> bool {
        let line = line.trim_start();
        let line = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub(super) "))
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        let Some(rest) = line.strip_prefix("const ") else {
            return false;
        };
        let Some((_, ty_and_value)) = rest.split_once(':') else {
            return false;
        };
        let ty_and_value = ty_and_value.trim_start();
        (ty_and_value.starts_with("usize = 0x"))
            || (ty_and_value.starts_with("[usize;") && ty_and_value.contains("= [0x"))
    }

    /// Literal offsets per source file under `src`, outside `src/pinned/`.
    fn counts(src: &Path) -> BTreeMap<String, usize> {
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
                let text = std::fs::read_to_string(&path).unwrap();
                let count = text.lines().filter(|l| is_literal_offset(l)).count();
                if count > 0 {
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
    fn no_file_holds_more_literal_offsets_than_its_list_allows() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let found = counts(&root.join("src"));
        let list = std::fs::read_to_string(root.join("tests/literal_offsets.txt")).unwrap();
        let allowed = allowed(&list);
        let mut wrong = Vec::new();
        for (path, &count) in &found {
            let allows = allowed.get(path).copied().unwrap_or(0);
            if count > allows {
                wrong.push(format!(
                    "{path}: {count} literal offsets, {allows} allowed; take each new one from \
                     reflection or the build table (src/pinned/)"
                ));
            }
        }
        for (path, &allows) in &allowed {
            let count = found.get(path).copied().unwrap_or(0);
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
