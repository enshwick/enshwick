//! Every resource of the real archives reads into typed values and writes
//! back to the same bytes. The archives sit beside the executables
//! `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name; without them the tests pass
//! trivially.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use enshwick_gamedata::archive::Archive;
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

    /// Decodes every resource of the archive beside `var`'s executable,
    /// encodes each over its own bytes, and lists the types that failed with
    /// a count and the first error.
    fn round_trip(var: &str) {
        let Ok(exe) = std::env::var(var) else {
            return;
        };
        let exe = Path::new(&exe);
        let bytes = std::fs::read(exe).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let registry = Reflection::scan(&image).registry();
        let stem = exe.file_stem().unwrap().to_str().unwrap();
        let archive = Archive::open(exe.parent().unwrap(), stem).unwrap();
        let keys = archive.keys();
        let mut failed: BTreeMap<String, (usize, String)> = BTreeMap::new();
        let mut unknown = 0usize;
        for key in &keys {
            let Some(ty) = registry.hashed(key.type_hash) else {
                unknown += 1;
                continue;
            };
            let name = ty.qualified_name();
            let base = archive.resource(key).unwrap();
            let outcome = registry.decode(name, &base).and_then(|value| {
                let again = registry.encode(name, &value, Some(&base))?;
                Ok((again == base).then_some(()).ok_or_else(|| {
                    let at = again.iter().zip(&base).position(|(a, b)| a != b);
                    format!("bytes differ at {at:?}")
                }))
            });
            let error = match outcome {
                Ok(Ok(())) => continue,
                Ok(Err(differ)) => differ,
                Err(error) => error.to_string(),
            };
            let entry = failed.entry(name.to_owned()).or_insert((0, error));
            entry.0 += 1;
        }
        let report: Vec<String> = failed
            .iter()
            .map(|(name, (count, first))| format!("{name}: {count} ({first})"))
            .collect();
        assert!(
            report.is_empty(),
            "{} of {} resources failed, {unknown} of unknown types:\n{}",
            failed.values().map(|(count, _)| count).sum::<usize>(),
            keys.len(),
            report.join("\n")
        );
        assert_eq!(unknown, 0, "resources of a type the executable lacks");
    }

    #[test]
    fn every_server_resource_reads_and_writes_back_unchanged() {
        round_trip("ENSH_SERVER_EXE");
    }

    #[test]
    fn every_client_resource_reads_and_writes_back_unchanged() {
        round_trip("ENSH_CLIENT_EXE");
    }
}
