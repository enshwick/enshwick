use super::*;

/// A folder of its own under the system's temporary folder, removed when
/// dropped.
struct Folder(std::path::PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("enshwick-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _gone = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_pack_folder_reads_its_scripts_but_not_their_definitions() {
    let folder = Folder::new("scripts");
    folder.write("pack.toml", "id = \"you.greeter\"\nversion = \"1.0.0\"\n");
    folder.write("main.luau", "print('hi')");
    folder.write("lib/greet.luau", "return 1");
    folder.write("types/enshwick.d.luau", "declare enshwick: {}");
    folder.write("notes.txt", "not a script");
    let files = PackFiles::read_dir(&folder.0).unwrap();
    let scripts: Vec<&str> = files.scripts().map(|(name, _)| name).collect();
    assert_eq!(scripts, ["lib/greet.luau", "main.luau"]);
    assert!(files.get("types/enshwick.d.luau").is_none());
    assert!(files.get("notes.txt").is_none());
}

#[test]
fn script_files_lists_what_read_dir_reads_as_scripts() {
    let folder = Folder::new("listed");
    folder.write("pack.toml", "id = \"you.greeter\"\nversion = \"1.0.0\"\n");
    assert!(script_files(&folder.0).unwrap().is_empty(), "no main.luau");
    folder.write("main.luau", "print('hi')");
    folder.write("lib/greet.luau", "return 1");
    folder.write("types/enshwick.d.luau", "declare enshwick: {}");
    let mut listed = script_files(&folder.0).unwrap();
    listed.sort();
    assert_eq!(
        listed,
        [folder.0.join("lib/greet.luau"), folder.0.join("main.luau")]
    );
    let read = PackFiles::read_dir(&folder.0).unwrap();
    assert_eq!(read.scripts().count(), listed.len());
}

#[test]
fn a_changed_script_changes_the_pack_s_hash() {
    let folder = Folder::new("hash");
    folder.write("pack.toml", "id = \"you.greeter\"\nversion = \"1.0.0\"\n");
    folder.write("main.luau", "print('hi')");
    let before = PackFiles::read_dir(&folder.0).unwrap().hash();
    folder.write("main.luau", "print('hello')");
    assert_ne!(PackFiles::read_dir(&folder.0).unwrap().hash(), before);
}

#[test]
fn scripts_deeper_than_the_bound_are_not_read_and_too_many_are_refused() {
    let folder = Folder::new("deep");
    folder.write("main.luau", "return 1");
    folder.write("a/b/c/d/four.luau", "return 1");
    folder.write("a/b/c/d/e/five.luau", "return 1");
    let files = PackFiles::read_dir(&folder.0).unwrap();
    let scripts: Vec<&str> = files.scripts().map(|(name, _)| name).collect();
    assert_eq!(scripts, ["a/b/c/d/four.luau", "main.luau"]);
    let folder = Folder::new("many");
    for i in 0..MAX_SCRIPTS {
        folder.write(&format!("s{i}.luau"), "return 1");
    }
    folder.write("main.luau", "return 1");
    let error = PackFiles::read_dir(&folder.0).unwrap_err();
    assert!(
        error.to_string().contains(&MAX_SCRIPTS.to_string()),
        "{error}"
    );
}

#[test]
fn a_script_in_the_icons_folder_keeps_the_file_bound() {
    let folder = Folder::new("icon-script");
    folder.write("main.luau", "return 1");
    let script = "-".repeat(usize::try_from(MAX_ICON_FILE).unwrap() + 1);
    folder.write("icons/large.luau", &script);
    let files = PackFiles::read_dir(&folder.0).unwrap();
    assert_eq!(
        files.get("icons/large.luau").map(<[u8]>::len),
        Some(script.len())
    );
}

#[test]
fn icons_past_the_bound_are_refused() {
    let folder = Folder::new("icons");
    for i in 0..MAX_ICONS {
        folder.write(&format!("icons/i{i}.png"), "png");
    }
    let files = PackFiles::read_dir(&folder.0).unwrap();
    assert_eq!(files.names().count(), MAX_ICONS);
    folder.write("icons/one-more.png", "png");
    let error = PackFiles::read_dir(&folder.0).unwrap_err();
    assert!(
        error.to_string().contains(&MAX_ICONS.to_string()),
        "{error}"
    );
}

#[test]
fn an_icon_larger_than_an_icon_may_be_is_refused() {
    let folder = Folder::new("large-icon");
    let large = "x".repeat(usize::try_from(MAX_ICON_FILE).unwrap() + 1);
    folder.write("icons/large.png", &large);
    let error = PackFiles::read_dir(&folder.0).unwrap_err();
    assert!(
        matches!(&error, ReadPackError::TooLarge { file, .. } if file == "icons/large.png"),
        "{error}"
    );
    assert!(
        error.to_string().contains(&MAX_ICON_FILE.to_string()),
        "{error}"
    );
}

#[test]
fn scripts_of_a_pack_without_main_are_not_read() {
    let folder = Folder::new("no-main");
    folder.write("helper.luau", "return 1");
    let files = PackFiles::read_dir(&folder.0).unwrap();
    assert_eq!(files.scripts().count(), 0);
}

#[cfg(unix)]
#[test]
fn linked_folders_are_not_followed() {
    let outside = Folder::new("outside");
    outside.write("stolen.luau", "return 1");
    let folder = Folder::new("links");
    folder.write("main.luau", "return 1");
    std::os::unix::fs::symlink(&outside.0, folder.0.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(&folder.0, folder.0.join("loop")).unwrap();
    let files = PackFiles::read_dir(&folder.0).unwrap();
    let scripts: Vec<&str> = files.scripts().map(|(name, _)| name).collect();
    assert_eq!(scripts, ["main.luau"]);
}
