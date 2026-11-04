//! Every Luau example of the book compiles, as the Rust ones do as
//! doctests.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Every Markdown file under `dir`.
    fn chapters(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(chapters(&path));
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
        out
    }

    /// The code of each ```` ```luau ```` block of `text`, with its first
    /// line's number.
    fn luau_blocks(text: &str) -> Vec<(usize, String)> {
        let mut blocks = Vec::new();
        let mut open: Option<(usize, String)> = None;
        for (number, line) in text.lines().enumerate() {
            match (&mut open, line.trim_start()) {
                (None, "```luau") => open = Some((number + 2, String::new())),
                (Some(_), "```") => blocks.extend(open.take()),
                (Some((_, code)), _) => {
                    code.push_str(line);
                    code.push('\n');
                }
                (None, _) => {}
            }
        }
        blocks
    }

    #[test]
    fn every_luau_example_of_the_book_compiles() {
        let book = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../book/src");
        let lua = mlua::Lua::new();
        let mut compiled = 0;
        for chapter in chapters(&book) {
            let text = std::fs::read_to_string(&chapter).unwrap();
            for (line, code) in luau_blocks(&text) {
                let name = format!("{}:{line}", chapter.display());
                if let Err(e) = lua.load(&code).set_name(&name).into_function() {
                    panic!("{name}: {e}");
                }
                compiled += 1;
            }
        }
        assert!(compiled > 0, "no Luau example in {}", book.display());
    }
}
