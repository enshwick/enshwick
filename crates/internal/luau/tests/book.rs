//! Every Luau example of the book compiles and type-checks against the
//! definitions `cargo enshwick luau` writes, as the Rust ones do as doctests.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn book() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../book/src")
    }

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
        let book = book();
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

    /// The trading post the book's examples depend on, declared as the book
    /// declares it, so its deps modules are the ones `cargo enshwick deps
    /// trading_post --luau` writes.
    mod trading_post {
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
        #[shape(values(BUY, SELL))]
        pub(crate) struct Deal(pub(crate) u32);

        impl Deal {
            pub(crate) const BUY: Self = Self(0);
            pub(crate) const SELL: Self = Self(1);
        }

        /// The price of an item at the trading post, in coins.
        #[enshwick::point(name = "you.trading-post:price", mode = Override, output = u32, refusable)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Price {
            pub(crate) item: u32,
            pub(crate) count: u32,
            pub(crate) player: u64,
            pub(crate) character: enshwick::Entity,
            pub(crate) deal: Deal,
        }

        /// What `count` of `item` sell for, in coins.
        #[enshwick::call(name = "you.trading-post:price-of", output = u32)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct PriceOf {
            pub(crate) item: u32,
            pub(crate) count: u32,
        }

        /// An item sold at the trading post.
        #[enshwick::event(name = "you.trading-post:sold")]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Sold {
            pub(crate) item: u32,
            pub(crate) price: u32,
        }
    }

    /// Where luau-lsp is: `LUAU_LSP`, or `luau-lsp` on the path; `None`
    /// without it.
    fn luau_lsp() -> Option<PathBuf> {
        let tool =
            std::env::var_os("LUAU_LSP").map_or_else(|| PathBuf::from("luau-lsp"), PathBuf::from);
        Command::new(&tool)
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
            .then_some(tool)
    }

    /// Writes the definitions, the trading post's deps modules and every
    /// book block into `dir`, a block as `<chapter>-<line>.luau`; returns
    /// each block's file with its chapter and first line.
    fn lay_out(dir: &Path) -> Vec<(PathBuf, String, usize)> {
        use enshwick::points::Point as _;
        use enshwick::services::{Call as _, ModEvent as _};
        let _fresh = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir.join("deps/trading_post")).unwrap();
        std::fs::write(
            dir.join("enshwick.d.luau"),
            cargo_enshwick::luau::definitions(),
        )
        .unwrap();
        std::fs::write(
            dir.join("enshwick.typed.d.luau"),
            cargo_enshwick::luau::typed_definitions(),
        )
        .unwrap();
        let schema = format!(
            "schema = {}\nmod = \"you.trading-post\"\nversion = \"1.0.0\"\n{}\n{}\n{}",
            enshwick::schema_section::FORMAT,
            trading_post::Price::SCHEMA,
            trading_post::PriceOf::SCHEMA,
            trading_post::Sold::SCHEMA,
        );
        let schema = cargo_enshwick::points::Schema::parse(&schema).unwrap();
        let deps = cargo_enshwick::points::luau_deps(&schema, "trading_post").unwrap();
        for (name, text) in &deps.modules {
            std::fs::write(
                dir.join("deps/trading_post").join(format!("{name}.luau")),
                text,
            )
            .unwrap();
        }
        let book = book();
        let mut blocks = Vec::new();
        for chapter in chapters(&book) {
            let shown = chapter
                .strip_prefix(&book)
                .unwrap_or(&chapter)
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&chapter).unwrap();
            for (line, code) in luau_blocks(&text) {
                let file = dir.join(format!("{}-{line}.luau", shown.replace(['/', '.'], "_")));
                std::fs::write(&file, code).unwrap();
                blocks.push((file, shown.clone(), line));
            }
        }
        blocks
    }

    /// The type errors in luau-lsp's plain output `out`, each moved to its
    /// chapter and line in the book.
    fn type_errors(out: &str, blocks: &[(PathBuf, String, usize)]) -> Vec<String> {
        out.lines()
            .filter(|line| line.contains("TypeError") || line.contains("SyntaxError"))
            .map(|line| {
                let block = blocks.iter().find(|(file, _, _)| {
                    file.file_name()
                        .is_some_and(|name| line.contains(&*name.to_string_lossy()))
                });
                let Some((file, chapter, first)) = block else {
                    return line.to_owned();
                };
                let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
                let rest = name
                    .and_then(|name| line.split_once(&*name).map(|(_, rest)| rest.to_owned()))
                    .unwrap_or_default();
                // `(<line>,<column>)...: ...`, the line counted from 1.
                let at = rest
                    .trim_start_matches(['(', ':'])
                    .split([',', ':', ')'])
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .map_or(*first, |n| first + n - 1);
                let message = rest.split_once(": ").map_or(rest.as_str(), |(_, m)| m);
                format!("{chapter}:{at}: {message}")
            })
            .collect()
    }

    /// Every block type-checks against the definitions this build writes:
    /// a renamed or removed call fails here. Lints such as an unused local
    /// are allowed. Without luau-lsp it is skipped, except in CI, which
    /// installs it.
    #[test]
    fn every_luau_example_of_the_book_type_checks() {
        let Some(tool) = luau_lsp() else {
            assert!(
                std::env::var_os("CI").is_none(),
                "luau-lsp is missing; CI installs it and sets LUAU_LSP"
            );
            return;
        };
        let dir = std::env::temp_dir().join(format!("enshwick-book-luau-{}", std::process::id()));
        let blocks = lay_out(&dir);
        let out = Command::new(tool)
            .arg("analyze")
            .arg("--formatter=plain")
            .arg(format!(
                "--definitions:@enshwick={}",
                dir.join("enshwick.d.luau").display()
            ))
            .arg(format!(
                "--definitions:@typed={}",
                dir.join("enshwick.typed.d.luau").display()
            ))
            .args(blocks.iter().map(|(file, _, _)| file))
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let errors = type_errors(&text, &blocks);
        let _gone = std::fs::remove_dir_all(&dir);
        assert!(
            errors.is_empty(),
            "{} type errors in the book's Luau examples:\n{}",
            errors.len(),
            errors.join("\n")
        );
    }
}
