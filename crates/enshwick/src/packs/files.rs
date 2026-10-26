use std::collections::BTreeMap;
use std::path::Path;

use crate::Off;

use super::{DEFINITIONS_SUFFIX, FILES, MAX_FILE, MAX_SCRIPT_DEPTH, MAX_SCRIPTS, SCRIPT_EXTENSION};

/// One problem in a pack: where it is and what is wrong.
///
/// # Examples
///
/// ```
/// use enshwick::packs::{Pack, PackFiles};
///
/// let files = PackFiles::new().with("items.toml", "[[item]]\nid = \"lantern\"\n");
/// let errors = Pack::parse("you.lanterns", &files).unwrap_err();
/// assert_eq!(errors.list()[0].line, 1);
/// assert!(errors.to_string().starts_with("items.toml:1:1: "), "{errors}");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}:{column}: {message}")]
pub struct PackError {
    /// The file, as the pack names it, such as `items.toml`.
    pub file: String,
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
    /// What is wrong.
    pub message: String,
}

/// Every problem [`Pack::parse`](super::Pack::parse) found, in file order.
///
/// # Examples
///
/// See [`PackError`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}", self.lines())]
pub struct ParsePackError(pub(super) Vec<PackError>);

impl ParsePackError {
    /// The problems.
    ///
    /// # Examples
    ///
    /// See [`PackError`].
    #[must_use]
    pub fn list(&self) -> &[PackError] {
        &self.0
    }

    fn lines(&self) -> String {
        self.0
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl From<ParsePackError> for Off {
    fn from(errors: ParsePackError) -> Self {
        Self::new(format!("data pack: {errors}"))
    }
}

/// Why [`PackFiles::read_dir`] could not read a pack's folder.
///
/// # Examples
///
/// ```
/// use enshwick::packs::ReadPackError;
///
/// let large = ReadPackError::TooLarge { file: "items.toml".into() };
/// assert_eq!(large.to_string(), "items.toml: larger than 4194304 bytes");
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadPackError {
    /// A file could not be read.
    #[error("{file}: {source}")]
    Io {
        /// The file, as the pack names it.
        file: String,
        /// Why.
        source: std::io::Error,
    },
    /// A file is larger than [`MAX_FILE`].
    #[error("{file}: larger than {MAX_FILE} bytes")]
    TooLarge {
        /// The file, as the pack names it.
        file: String,
    },
    /// A script sits more than [`MAX_SCRIPT_DEPTH`] folders deep.
    #[error("{file}: more than {MAX_SCRIPT_DEPTH} folders deep")]
    TooDeep {
        /// The script, as the pack names it.
        file: String,
    },
    /// The pack holds more than [`MAX_SCRIPTS`] scripts.
    #[error("more than {MAX_SCRIPTS} scripts")]
    TooManyScripts,
}

/// The files of a pack: their names, such as `items.toml` or
/// `icons/lantern.png`, and their bytes.
///
/// # Examples
///
/// ```
/// use enshwick::packs::PackFiles;
///
/// let files = PackFiles::new().with("texts.toml", "[en]\n");
/// assert_eq!(files.get("texts.toml"), Some(&b"[en]\n"[..]));
/// assert_ne!(files.hash(), PackFiles::new().hash());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackFiles {
    pub(super) files: BTreeMap<String, Vec<u8>>,
}

impl PackFiles {
    /// No files.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The same with the file `name` holding `bytes`.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn with(mut self, name: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.insert(name.to_owned(), bytes.into());
        self
    }

    /// The pack files in the folder `dir`: the files of [`FILES`] it holds,
    /// every PNG under `dir/icons` and every script in it and its folders
    /// but definitions files, each at most [`MAX_FILE`] bytes.
    ///
    /// # Errors
    ///
    /// [`ReadPackError`] naming the file that could not be read, is larger
    /// than [`MAX_FILE`] or a script too deep, or for too many scripts.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::PackFiles;
    ///
    /// let files = PackFiles::read_dir(std::path::Path::new("no such folder"))?;
    /// assert!(files.names().next().is_none());
    /// # Ok::<(), enshwick::packs::ReadPackError>(())
    /// ```
    pub fn read_dir(dir: &Path) -> Result<Self, ReadPackError> {
        let mut files = Self::new();
        let read = |path: &Path, name: &str, files: &mut Self| -> Result<(), ReadPackError> {
            let io = |source| ReadPackError::Io {
                file: name.to_owned(),
                source,
            };
            let meta = std::fs::metadata(path).map_err(io)?;
            if meta.len() > MAX_FILE {
                return Err(ReadPackError::TooLarge {
                    file: name.to_owned(),
                });
            }
            let bytes = std::fs::read(path).map_err(io)?;
            files.files.insert(name.to_owned(), bytes);
            Ok(())
        };
        for name in FILES {
            let path = dir.join(name);
            if path.is_file() {
                read(&path, name, &mut files)?;
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("icons")) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let png = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"));
                if let (true, Some(file)) = (png, path.file_name().and_then(|f| f.to_str())) {
                    read(&path, &format!("icons/{file}"), &mut files)?;
                }
            }
        }
        let mut scripts = Vec::new();
        find_scripts(dir, "", 0, &mut scripts)?;
        if scripts.len() > MAX_SCRIPTS {
            return Err(ReadPackError::TooManyScripts);
        }
        for (path, name) in scripts {
            read(&path, &name, &mut files)?;
        }
        Ok(files)
    }

    /// Every script, by its name relative to the pack's folder, with its
    /// bytes, sorted by name.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::PackFiles;
    ///
    /// let files = PackFiles::new().with("main.luau", "print('hi')").with("items.toml", "");
    /// assert_eq!(files.scripts().map(|(name, _)| name).collect::<Vec<_>>(), ["main.luau"]);
    /// ```
    pub fn scripts(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .filter(|(name, _)| is_script(name))
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
    }

    /// The bytes of the file `name`.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(Vec::as_slice)
    }

    /// The names of the files, sorted.
    ///
    /// # Examples
    ///
    /// See [`PackFiles::read_dir`].
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// A hash of every file's name and bytes, the same for the same files:
    /// FNV-1a 32, as eight hex digits go into the join check.
    ///
    /// # Examples
    ///
    /// See [`PackFiles`].
    #[must_use]
    pub fn hash(&self) -> u32 {
        let mut hash: u32 = 0x811c_9dc5;
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                hash ^= u32::from(b);
                hash = hash.wrapping_mul(0x0100_0193);
            }
        };
        for (name, bytes) in &self.files {
            eat(name.as_bytes());
            eat(&[0]);
            eat(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
            eat(bytes);
        }
        hash
    }

    pub(super) fn text(&self, name: &str) -> Result<Option<&str>, PackError> {
        self.get(name)
            .map(|bytes| {
                core::str::from_utf8(bytes)
                    .map_err(|e| PackError::at(name, "", 0, &format!("not UTF-8: {e}")))
            })
            .transpose()
    }
}

impl PackError {
    /// The error at byte `offset` of `text`, the contents of `file`.
    pub(super) fn at(file: &str, text: &str, offset: usize, message: &str) -> Self {
        let before = text.get(..offset.min(text.len())).unwrap_or("");
        let line = before.matches('\n').count() + 1;
        let column = before
            .rsplit_once('\n')
            .map_or(before, |(_, last)| last)
            .chars()
            .count()
            + 1;
        Self {
            file: file.to_owned(),
            line,
            column,
            message: message.to_owned(),
        }
    }

    /// A TOML error of `file`.
    pub(super) fn of(file: &str, text: &str, error: &toml::de::Error) -> Self {
        let offset = error.span().map_or(0, |span| span.start);
        Self::at(file, text, offset, error.message().trim_end())
    }
}

/// Whether the file `name` is a script.
fn is_script(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext == SCRIPT_EXTENSION)
        && !name.ends_with(DEFINITIONS_SUFFIX)
}

/// Adds every script under `dir`, named from `prefix`, `depth` folders below
/// the pack's folder, to `found`; hidden files and folders are left out.
fn find_scripts(
    dir: &Path,
    prefix: &str,
    depth: usize,
    found: &mut Vec<(std::path::PathBuf, String)>,
) -> Result<(), ReadPackError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.filter_map(Result::ok) {
        let Some(file) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if file.starts_with('.') {
            continue;
        }
        let name = format!("{prefix}{file}");
        let path = entry.path();
        if path.is_dir() {
            find_scripts(&path, &format!("{name}/"), depth + 1, found)?;
        } else if is_script(&name) {
            if depth > MAX_SCRIPT_DEPTH {
                return Err(ReadPackError::TooDeep { file: name });
            }
            found.push((path, name));
            if found.len() > MAX_SCRIPTS {
                return Err(ReadPackError::TooManyScripts);
            }
        }
    }
    Ok(())
}
