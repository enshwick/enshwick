//! The server's join mods a refused join found missing, as the loader lists
//! them to mods: their text kept for the process, within a budget.

use crate::kept_text::KeptText;

/// The most bytes of server-advertised text kept for
/// [`HostApi::join_missing`](enshwick::abi::HostApi::join_missing) over the
/// process: each refused join may bring new text, and none of it is freed.
pub(crate) const ADVERTISED_BYTES: usize = 64 << 10;

/// One of the server's join mods the join check found missing on this
/// client, as the server advertises it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wanted {
    /// The mod id.
    pub(crate) id: String,
    /// The name players read; empty for none.
    pub(crate) name: String,
    /// The server's version.
    pub(crate) version: String,
    /// The mod's homepage; empty for none.
    pub(crate) page: String,
}

/// One of the server's join mods the last refused join found missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Missing {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) version: &'static str,
    pub(crate) file: &'static str,
    pub(crate) page: &'static str,
}

/// `wanted` as listed to mods, the server's text kept in `advertised` and
/// the client's DLL path of each by `file`, and how many were left out
/// because `advertised` had no room for their text.
pub(crate) fn listed(
    advertised: &mut KeptText,
    wanted: &[Wanted],
    mut file: impl FnMut(&str) -> &'static str,
) -> (Vec<Missing>, usize) {
    let mut left_out = 0;
    let entries = wanted
        .iter()
        .filter_map(|wanted| {
            let entry = entry(advertised, wanted, file(&wanted.id));
            left_out += usize::from(entry.is_none());
            entry
        })
        .collect();
    (entries, left_out)
}

/// `wanted` with its text kept in `advertised`; `None` once that has no room.
fn entry(advertised: &mut KeptText, wanted: &Wanted, file: &'static str) -> Option<Missing> {
    Some(Missing {
        id: advertised.keep(&wanted.id)?,
        name: advertised.keep(&wanted.name)?,
        version: advertised.keep(&wanted.version)?,
        file,
        page: advertised.keep(&wanted.page)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted(join: usize) -> Vec<Wanted> {
        (0..4)
            .map(|i| Wanted {
                id: format!("server.mod-{join}-{i}"),
                name: format!("Mod {join} {i}"),
                version: "1.0.0".to_owned(),
                page: format!("https://example.com/{join}/{i}"),
            })
            .collect()
    }

    #[test]
    fn missing_mods_are_listed_with_the_server_s_text() {
        let mut advertised = KeptText::new(ADVERTISED_BYTES);
        let (entries, left_out) = listed(&mut advertised, &wanted(0), |_| "mods/a.dll");
        assert_eq!(left_out, 0);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[1].id, "server.mod-0-1");
        assert_eq!(entries[1].page, "https://example.com/0/1");
        assert_eq!(entries[1].file, "mods/a.dll");
    }

    #[test]
    fn a_server_sending_new_text_on_every_join_is_listed_only_within_the_budget() {
        let mut advertised = KeptText::new(ADVERTISED_BYTES);
        let mut texts = std::collections::BTreeSet::new();
        let mut left_total = 0;
        for join in 0..2000 {
            let (entries, left_out) = listed(&mut advertised, &wanted(join), |_| "");
            texts.extend(
                entries
                    .iter()
                    .flat_map(|e| [e.id, e.name, e.version, e.page]),
            );
            left_total += left_out;
        }
        assert!(left_total > 0, "the budget ran out");
        let kept: usize = texts.iter().map(|text| text.len()).sum();
        assert!(kept <= ADVERTISED_BYTES, "{kept} bytes of text listed");
        let (again, left_out) = listed(&mut advertised, &wanted(0), |_| "");
        assert_eq!(
            (again.len(), left_out),
            (4, 0),
            "text kept before still lists"
        );
    }
}
