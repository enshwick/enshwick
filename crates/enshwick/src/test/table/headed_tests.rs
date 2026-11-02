//! The fake reads and writes a mod's headed structs through their header, as
//! the loader does, so a mod of an older minor works on it as in the game.

use std::path::{Path, PathBuf};

/// The helpers that read or write a headed struct only as far as its header
/// says; `declared` reads a spec through `get_headed`, and `list` writes
/// entries `entry_size` apart.
const HELPERS: [&str; 11] = [
    " list(",
    "::list(",
    "get_headed(",
    "get_headed::<",
    "get_headed_list(",
    "put_headed(",
    "put_headed_list(",
    "found_headed(",
    "read_headed(",
    "write_headed(",
    "declared(",
];

/// Every `.rs` file under `dir`.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    found
}

/// The structs of the ABI that implement `Headed`.
fn headed() -> Vec<String> {
    let abi = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/abi");
    sources(&abi)
        .iter()
        .map(|file| std::fs::read_to_string(file).unwrap())
        .flat_map(|source| {
            source
                .split("\nimpl_headed!(")
                .skip(1)
                .filter_map(|rest| rest.split([')', ',']).next().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether `body` hands `param` to a header-aware helper.
fn routed(body: &str, param: &str) -> bool {
    HELPERS.iter().any(|helper| {
        body.match_indices(helper).any(|(at, _)| {
            let args = &body[at + helper.len()..];
            let mut depth = 0_usize;
            let end = args
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '(' => depth += 1,
                        ')' if depth == 0 => return true,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    false
                })
                .map_or(args.len(), |(end, _)| end);
            let args = &args[..end];
            args.split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == param)
        })
    })
}

#[test]
fn every_fake_entry_point_takes_a_mod_s_headed_struct_through_its_header() {
    let headed = headed();
    assert!(headed.len() > 50, "{} headed structs found", headed.len());
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/test");
    let mut checked = 0;
    for file in sources(&fake) {
        let source = std::fs::read_to_string(&file).unwrap();
        for item in source.split("\nfn ").chain(source.split(" fn ")).skip(1) {
            let Some((signature, body)) = item.split_once('{') else {
                continue;
            };
            let body = body.split("\nfn ").next().unwrap_or(body);
            let body = body.split(" fn ").next().unwrap_or(body);
            for param in signature.split(',') {
                let Some((name, ty)) = param.split_once(": ") else {
                    continue;
                };
                let name = name.rsplit(['(', ' ', '\n']).next().unwrap_or_default();
                let ty = ty.split(')').next().unwrap_or_default().trim();
                let pointee = ty
                    .strip_prefix("*const ")
                    .or_else(|| ty.strip_prefix("*mut "))
                    .and_then(|pointee| pointee.rsplit("::").next());
                if pointee.is_none_or(|pointee| !headed.iter().any(|h| h == pointee)) {
                    continue;
                }
                checked += 1;
                assert!(
                    routed(body, name),
                    "{}: `{name}` reaches a mod's headed struct around its header",
                    file.display()
                );
            }
        }
    }
    assert!(checked > 30, "{checked} headed parameters checked");
}
