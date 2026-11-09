//! Where Steam installed Enshrouded: the Steam folders of this machine, their
//! libraries from `steamapps/libraryfolders.vdf`, and in each the game's and
//! the dedicated server's `appmanifest_<app>.acf`, which names the folder.

use std::path::{Path, PathBuf};

/// The dedicated server's Steam app.
pub(crate) const SERVER_APP: u32 = 2_278_520;
/// The game's Steam app.
pub(crate) const GAME_APP: u32 = 1_203_620;

/// An install Steam knows of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    /// The folder holding the executable.
    pub(crate) folder: PathBuf,
    /// The Steam app installed there, [`SERVER_APP`] or [`GAME_APP`].
    pub(crate) app: u32,
}

impl Found {
    /// What the app is, as a person names it.
    pub(crate) fn what(&self) -> &'static str {
        if self.app == SERVER_APP {
            "the dedicated server"
        } else {
            "the game"
        }
    }
}

/// A `KeyValues` value: a string, or a block of named values in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Text(String),
    Block(Vec<(String, Value)>),
}

impl Value {
    /// The value named `key` in a block, compared without case as Steam
    /// does; `None` in a string.
    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Block(pairs) => pairs
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value),
            Self::Text(_) => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Block(_) => None,
        }
    }
}

/// Why a `KeyValues` file does not read.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ParseKeyValuesError {
    #[error("a quoted string is not closed")]
    Unclosed,
    #[error("a key is missing its value")]
    NoValue,
    #[error("a `{{` or `}}` stands where a key belongs")]
    BraceForKey,
    #[error("a block is not closed")]
    UnclosedBlock,
    #[error("a `}}` closes no block")]
    StrayClose,
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Open,
    Close,
    Text(String),
}

/// The tokens of Valve's `KeyValues` text: braces, quoted strings with `\\`,
/// `\"`, `\n` and `\t` escapes, and bare words; `//` starts a comment.
fn tokens(text: &str) -> Result<Vec<Token>, ParseKeyValuesError> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '"' => {
                let mut word = String::new();
                loop {
                    match chars.next().ok_or(ParseKeyValuesError::Unclosed)? {
                        '"' => break,
                        '\\' => match chars.next().ok_or(ParseKeyValuesError::Unclosed)? {
                            'n' => word.push('\n'),
                            't' => word.push('\t'),
                            other => word.push(other),
                        },
                        other => word.push(other),
                    }
                }
                out.push(Token::Text(word));
            }
            '/' if chars.peek() == Some(&'/') => while chars.next_if(|&c| c != '\n').is_some() {},
            c if c.is_whitespace() => {}
            c => {
                let mut word = String::from(c);
                while let Some(c) =
                    chars.next_if(|&c| !c.is_whitespace() && !matches!(c, '{' | '}' | '"'))
                {
                    word.push(c);
                }
                out.push(Token::Text(word));
            }
        }
    }
    Ok(out)
}

/// The top-level block of a `KeyValues` file such as `libraryfolders.vdf` or
/// an `appmanifest_<app>.acf`.
pub(crate) fn parse(text: &str) -> Result<Value, ParseKeyValuesError> {
    let mut tokens = tokens(text)?.into_iter();
    let pairs = block(&mut tokens, false)?;
    Ok(Value::Block(pairs))
}

fn block(
    tokens: &mut impl Iterator<Item = Token>,
    nested: bool,
) -> Result<Vec<(String, Value)>, ParseKeyValuesError> {
    let mut pairs = Vec::new();
    loop {
        let key = match tokens.next() {
            None if nested => return Err(ParseKeyValuesError::UnclosedBlock),
            None => return Ok(pairs),
            Some(Token::Close) if nested => return Ok(pairs),
            Some(Token::Close) => return Err(ParseKeyValuesError::StrayClose),
            Some(Token::Open) => return Err(ParseKeyValuesError::BraceForKey),
            Some(Token::Text(key)) => key,
        };
        let value = match tokens.next().ok_or(ParseKeyValuesError::NoValue)? {
            Token::Open => Value::Block(block(tokens, true)?),
            Token::Text(text) => Value::Text(text),
            Token::Close => return Err(ParseKeyValuesError::NoValue),
        };
        pairs.push((key, value));
    }
}

/// The library folders `libraryfolders.vdf` in `steam` lists, `steam`
/// itself first; just `steam` when the file is missing or does not read.
fn libraries(steam: &Path) -> Vec<PathBuf> {
    let listed = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf"))
        .ok()
        .and_then(|text| parse(&text).ok())
        .and_then(|vdf| match vdf.get("libraryfolders") {
            Some(Value::Block(folders)) => Some(
                folders
                    .iter()
                    .filter_map(|(_, folder)| folder.get("path")?.text().map(PathBuf::from))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    std::iter::once(steam.to_owned()).chain(listed).collect()
}

/// The folder of `app` in `library`, from its app manifest, when it exists.
fn installed(library: &Path, app: u32) -> Option<PathBuf> {
    let steamapps = library.join("steamapps");
    let text = std::fs::read_to_string(steamapps.join(format!("appmanifest_{app}.acf"))).ok()?;
    let dir = parse(&text)
        .ok()?
        .get("AppState")?
        .get("installdir")?
        .text()?
        .to_owned();
    let folder = steamapps.join("common").join(dir);
    folder.is_dir().then_some(folder)
}

/// Every server and game install in the libraries of the Steam folders
/// `steams`, servers first, each folder once.
pub(crate) fn find(steams: &[PathBuf]) -> Vec<Found> {
    let mut libraries: Vec<PathBuf> = Vec::new();
    let mut seen = Vec::new();
    for library in steams.iter().flat_map(|steam| libraries_of(steam)) {
        let key = library.canonicalize().unwrap_or_else(|_| library.clone());
        if !seen.contains(&key) {
            seen.push(key);
            libraries.push(library);
        }
    }
    [SERVER_APP, GAME_APP]
        .into_iter()
        .flat_map(|app| {
            libraries.iter().filter_map(move |library| {
                Some(Found {
                    folder: installed(library, app)?,
                    app,
                })
            })
        })
        .collect()
}

fn libraries_of(steam: &Path) -> Vec<PathBuf> {
    if steam.join("steamapps").is_dir() {
        libraries(steam)
    } else {
        Vec::new()
    }
}

/// The Steam folders this machine may have: on Windows the one the
/// registry names and the default one, elsewhere the native, Debian and
/// Flatpak installs on Linux and the macOS one.
pub(crate) fn steam_folders() -> Vec<PathBuf> {
    if cfg!(windows) {
        let registry = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| registry_path(&String::from_utf8_lossy(&out.stdout)));
        let default =
            std::env::var_os("ProgramFiles(x86)").map(|dir| PathBuf::from(dir).join("Steam"));
        registry.into_iter().chain(default).collect()
    } else {
        let Some(home) = std::env::home_dir() else {
            return Vec::new();
        };
        [
            ".steam/steam",
            ".local/share/Steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
            "Library/Application Support/Steam",
        ]
        .iter()
        .map(|dir| home.join(dir))
        .collect()
    }
}

/// The folder in `reg query`'s answer for a `REG_SZ` value.
fn registry_path(answer: &str) -> Option<PathBuf> {
    answer.lines().find_map(|line| {
        let (_, path) = line.split_once("REG_SZ")?;
        let path = path.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A fresh folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-steam-{name}-{}", std::process::id()));
        let _old = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn manifest(app: u32, dir: &str) -> String {
        format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app}\"\n\t\"name\"\t\t\"x\"\n\t\"installdir\"\t\t\"{dir}\"\n}}\n"
        )
    }

    /// A Steam folder at `root/steam` with a second library at
    /// `root/library`, as `libraryfolders.vdf` lists them with Windows
    /// escapes.
    fn steam(root: &Path) -> PathBuf {
        let steam = root.join("steam");
        let library = root.join("library");
        let escaped = |path: &Path| path.display().to_string().replace('\\', "\\\\");
        write(
            &steam.join("steamapps/libraryfolders.vdf"),
            &format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"228980\"\t\t\"1\"\n\t\t}}\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                escaped(&steam),
                escaped(&library)
            ),
        );
        std::fs::create_dir_all(library.join("steamapps")).unwrap();
        steam
    }

    #[test]
    fn the_server_is_found_before_the_game_in_any_library() {
        let scratch = scratch("steam-both");
        let steam = steam(&scratch);
        let library = scratch.join("library");
        write(
            &steam.join("steamapps/appmanifest_1203620.acf"),
            &manifest(GAME_APP, "Enshrouded"),
        );
        std::fs::create_dir_all(steam.join("steamapps/common/Enshrouded")).unwrap();
        write(
            &library.join("steamapps/appmanifest_2278520.acf"),
            &manifest(SERVER_APP, "EnshroudedServer"),
        );
        std::fs::create_dir_all(library.join("steamapps/common/EnshroudedServer")).unwrap();
        let found = find(&[steam.clone(), steam.clone()]);
        assert_eq!(
            found,
            [
                Found {
                    folder: library.join("steamapps/common/EnshroudedServer"),
                    app: SERVER_APP
                },
                Found {
                    folder: steam.join("steamapps/common/Enshrouded"),
                    app: GAME_APP
                },
            ]
        );
        assert_eq!(found[0].what(), "the dedicated server");
    }

    #[test]
    fn a_manifest_whose_folder_is_gone_finds_nothing() {
        let scratch = scratch("steam-gone");
        let steam = steam(&scratch);
        write(
            &steam.join("steamapps/appmanifest_1203620.acf"),
            &manifest(GAME_APP, "Enshrouded"),
        );
        assert_eq!(find(&[steam]), []);
        assert_eq!(find(&[scratch.join("no-steam")]), []);
    }

    #[test]
    fn the_registry_answer_names_the_steam_folder() {
        let answer = "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\r\n    SteamPath    REG_SZ    c:/program files (x86)/steam\r\n\r\n";
        assert_eq!(
            registry_path(answer),
            Some(PathBuf::from("c:/program files (x86)/steam"))
        );
        assert_eq!(registry_path("ERROR: not found"), None);
    }

    #[test]
    fn steam_s_files_read_with_comments_escapes_and_bare_words() {
        let text = "// header\n\"a\" { \"b\" \"C:\\\\Steam\" bare word \"q\" \"say \\\"hi\\\"\" }";
        let vdf = parse(text).unwrap();
        let a = vdf.get("A").unwrap();
        assert_eq!(a.get("b").and_then(Value::text), Some("C:\\Steam"));
        assert_eq!(a.get("bare").and_then(Value::text), Some("word"));
        assert_eq!(a.get("q").and_then(Value::text), Some("say \"hi\""));
    }

    #[test]
    fn broken_files_say_what_is_wrong() {
        assert_eq!(parse("\"a"), Err(ParseKeyValuesError::Unclosed));
        assert_eq!(parse("\"a\""), Err(ParseKeyValuesError::NoValue));
        assert_eq!(parse("\"a\" {"), Err(ParseKeyValuesError::UnclosedBlock));
        assert_eq!(parse("}"), Err(ParseKeyValuesError::StrayClose));
        assert_eq!(parse("{"), Err(ParseKeyValuesError::BraceForKey));
    }

    /// `pairs` written as Steam writes them, every string quoted and escaped.
    fn render(pairs: &[(String, Value)]) -> String {
        let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        pairs
            .iter()
            .map(|(key, value)| match value {
                Value::Text(text) => format!("{}\t\t{}\n", quote(key), quote(text)),
                Value::Block(inner) => format!("{}\n{{\n{}}}\n", quote(key), render(inner)),
            })
            .collect()
    }

    fn value() -> impl Strategy<Value = Value> {
        let text = "[^\n\t]{0,12}".prop_map(Value::Text);
        text.prop_recursive(3, 24, 4, |inner| {
            prop::collection::vec(("[^\n\t]{0,8}", inner), 0..4).prop_map(Value::Block)
        })
    }

    proptest! {
        #[test]
        fn what_steam_writes_reads_back_the_same(
            pairs in prop::collection::vec(("[^\n\t]{0,8}", value()), 0..4)
        ) {
            prop_assert_eq!(parse(&render(&pairs)), Ok(Value::Block(pairs)));
        }

        #[test]
        fn any_text_reads_or_fails_without_a_panic(text in "\\PC{0,64}") {
            let _either = parse(&text);
        }
    }
}
