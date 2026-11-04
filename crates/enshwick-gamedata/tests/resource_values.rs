//! Every resource of the real archives reads into typed values, writes back
//! to the same bytes, and gives each blob header a block of its own, so a
//! value written in place changes no other header's value. The archives sit
//! beside the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name;
//! without them those tests pass trivially.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use enshwick::resources::{Field, Kind, Registry, Type};
    use enshwick_gamedata::archive::Archive;
    use enshwick_gamedata::reflect::{Image, Layout, Reflection};

    /// The blob blocks a resource's headers reach, keyed by header position:
    /// where each block starts and how many bytes it holds.
    struct Blocks<'r, 'a, 'b> {
        registry: &'r Registry<'a>,
        bytes: &'b [u8],
        found: BTreeMap<usize, (usize, usize)>,
    }

    impl<'a> Blocks<'_, 'a, '_> {
        fn word(&self, at: usize) -> Option<u32> {
            Some(u32::from_le_bytes(
                self.bytes.get(at..at + 4)?.try_into().ok()?,
            ))
        }

        /// Where the `i32` offset at `from`, counted from `from`, points.
        fn target(&self, from: usize) -> Option<usize> {
            from.checked_add_signed(isize::try_from(self.word(from)?.cast_signed()).ok()?)
        }

        fn record(&mut self, header: usize, start: usize, len: usize) -> Option<()> {
            if len > 0 {
                self.bytes.get(start..start.checked_add(len)?)?;
                self.found.insert(header, (start, len));
            }
            Some(())
        }

        /// Follows the headers of the `ty` value at `at` as the codec's
        /// decoder does; `None` where they lead outside the bytes.
        fn walk(&mut self, ty: &Type<'a>, at: usize, depth: usize) -> Option<()> {
            if depth > 64 {
                return None;
            }
            let registry = self.registry;
            let inner = || ty.inner().and_then(|name| registry.named(name));
            match ty.kind() {
                Kind::Typedef => self.walk(inner()?, at, depth + 1),
                Kind::Struct => {
                    let mut chain = vec![ty];
                    while let Some(base) = chain.last()?.base().and_then(|n| registry.named(n)) {
                        if chain.len() > 16 {
                            return None;
                        }
                        chain.push(base);
                    }
                    for field in chain.iter().flat_map(|ty| ty.fields()) {
                        let Some(name) = field.type_name() else {
                            continue;
                        };
                        let offset = usize::try_from(field.offset()).ok()?;
                        self.walk(registry.named(name)?, at + offset, depth + 1)?;
                    }
                    Some(())
                }
                Kind::StaticArray => {
                    let count = usize::try_from(ty.element_count().unwrap_or(0)).ok()?;
                    let stride = usize::try_from(ty.size()).ok()?.checked_div(count);
                    let inner = inner()?;
                    (0..count)
                        .try_for_each(|i| self.walk(inner, at + i * stride.unwrap_or(0), depth + 1))
                }
                Kind::BlobArray => {
                    let count = usize::try_from(self.word(at + 4)?).ok()?;
                    let inner = inner()?;
                    let stride = usize::try_from(inner.size())
                        .ok()?
                        .next_multiple_of(usize::from(inner.align().max(1)));
                    let start = self.target(at)?;
                    // An array whose elements run past the resource holds packed
                    // data the codec keeps as bytes.
                    let fits = count.checked_mul(stride).filter(|len| {
                        *len > 0
                            && start
                                .checked_add(*len)
                                .is_some_and(|end| end <= self.bytes.len())
                    });
                    let Some(len) = fits else {
                        return Some(());
                    };
                    self.record(at, start, len)?;
                    (0..count).try_for_each(|i| self.walk(inner, start + i * stride, depth + 1))
                }
                Kind::BlobString => {
                    let len = usize::try_from(self.word(at + 4)?).ok()?;
                    if len == 0 {
                        return Some(());
                    }
                    let start = self.target(at)?;
                    self.record(at, start, len)
                }
                Kind::BlobOptional => {
                    if self.word(at)? == 0 {
                        return Some(());
                    }
                    let inner = inner()?;
                    let start = self.target(at)?;
                    self.record(at, start, usize::try_from(inner.size()).ok()?)?;
                    self.walk(inner, start, depth + 1)
                }
                Kind::BlobVariant => {
                    let hash = self.word(at)?;
                    if hash == 0 {
                        return Some(());
                    }
                    let held = registry.hashed(hash)?;
                    let start = self.target(at + 4)?;
                    self.record(at, start, usize::try_from(held.size()).ok()?)?;
                    self.walk(held, start, depth + 1)
                }
                _ => Some(()),
            }
        }
    }

    /// The pairs of header positions whose blocks start at the same byte or
    /// overlap, in the `ty` resource `bytes`; `None` when the headers do not
    /// walk.
    fn shared_blocks(
        registry: &Registry<'_>,
        ty: &Type<'_>,
        bytes: &[u8],
    ) -> Option<Vec<(usize, usize)>> {
        let mut blocks = Blocks {
            registry,
            bytes,
            found: BTreeMap::new(),
        };
        blocks.walk(ty, 0, 0)?;
        let mut ranges: Vec<(usize, usize, usize)> = blocks
            .found
            .iter()
            .map(|(header, (start, len))| (*start, start + len, *header))
            .collect();
        ranges.sort_unstable();
        let mut shared = Vec::new();
        let (mut end, mut owner) = (0, 0);
        for (start, until, header) in ranges {
            if start < end {
                shared.push((owner, header));
            }
            if until > end {
                (end, owner) = (until, header);
            }
        }
        Some(shared)
    }

    /// A registry of a struct with two byte arrays, `a` at 0 and `b` at 8.
    fn pair() -> Registry<'static> {
        Registry::new([
            Type::new("demo::uint8", Kind::Uint8, 1, 1),
            Type::new("demo::Bytes", Kind::BlobArray, 8, 4).with_inner("demo::uint8"),
            Type::new("demo::Pair", Kind::Struct, 16, 4).with_fields(vec![
                Field::new("a", 0, "demo::Bytes"),
                Field::new("b", 8, "demo::Bytes"),
            ]),
        ])
    }

    /// A `demo::Pair` whose `b` header points `b_offset` bytes past itself,
    /// with four data bytes after the headers.
    fn pair_bytes(b_offset: i32) -> Vec<u8> {
        [
            &16i32.to_le_bytes()[..],
            &2u32.to_le_bytes(),
            &b_offset.to_le_bytes(),
            &2u32.to_le_bytes(),
            &[7, 9, 1, 2],
        ]
        .concat()
    }

    #[test]
    fn two_array_headers_on_one_block_are_found_shared() {
        let registry = pair();
        let ty = registry.named("demo::Pair").unwrap();
        assert_eq!(
            shared_blocks(&registry, ty, &pair_bytes(8)),
            Some(vec![(0, 8)])
        );
        assert_eq!(
            shared_blocks(&registry, ty, &pair_bytes(9)),
            Some(vec![(0, 8)])
        );
    }

    #[test]
    fn array_headers_on_blocks_of_their_own_share_nothing() {
        let registry = pair();
        let ty = registry.named("demo::Pair").unwrap();
        assert_eq!(shared_blocks(&registry, ty, &pair_bytes(10)), Some(vec![]));
    }

    /// Decodes every resource of the archive beside `var`'s executable,
    /// encodes each over its own bytes, and lists the types that failed with
    /// a count and the first error; then lists the resources whose blob
    /// headers share a block.
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
        let version = archive.version().unwrap();
        let build = version.split('|').next().unwrap();
        assert!(
            build.parse::<u32>().is_ok() && version.contains("game38"),
            "{version}"
        );
        let keys = archive.keys();
        let mut failed: BTreeMap<String, (usize, String)> = BTreeMap::new();
        let mut unknown = 0usize;
        let mut shared: Vec<String> = Vec::new();
        for key in &keys {
            let Some(ty) = registry.hashed(key.type_hash) else {
                unknown += 1;
                continue;
            };
            let name = ty.qualified_name();
            let base = archive.resource(key).unwrap();
            match shared_blocks(&registry, ty, &base) {
                Some(pairs) if pairs.is_empty() => {}
                Some(pairs) => shared.push(format!("{name}: headers at {pairs:?}")),
                None => shared.push(format!("{name}: headers lead outside the resource")),
            }
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
        assert!(
            shared.is_empty(),
            "{} resources share a blob block, so an in-place write changes two values:\n{}",
            shared.len(),
            shared[..shared.len().min(20)].join("\n")
        );
    }

    #[test]
    fn every_server_resource_round_trips_and_shares_no_blob_block() {
        round_trip("ENSH_SERVER_EXE");
    }

    #[test]
    fn every_client_resource_round_trips_and_shares_no_blob_block() {
        round_trip("ENSH_CLIENT_EXE");
    }
}
