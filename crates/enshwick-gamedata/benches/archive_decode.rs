//! How long reading content out of the game's archive takes: an icon
//! decoded from BC7, and the attribute containers read and unpacked afresh
//! from the archive of the client `ENSH_CLIENT_EXE` names, skipped without it.

use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use enshwick::content::icons::{decode, formats};
use enshwick_gamedata::archive::Archive;

/// An icon's side in pixels.
const SIDE: u32 = 256;
/// The bytes of a BC7 icon of [`SIDE`] pixels: one byte a pixel.
const ICON_BYTES: usize = (SIDE * SIDE) as usize;
/// The resource type the loader reads attribute slots from.
const CONTAINER: &str = "keen::AttributeContainerResource";

fn read(c: &mut Criterion) {
    let mut group = c.benchmark_group("archive_decode");
    let icon: Vec<u8> = (0..ICON_BYTES).map(|i| i.to_le_bytes()[0]).collect();
    group.throughput(Throughput::Bytes(ICON_BYTES as u64));
    group.bench_function("bc7_icon", |b| {
        b.iter(|| decode(formats::BC7, SIDE, SIDE, black_box(&icon)));
    });
    let archive = std::env::var_os("ENSH_CLIENT_EXE")
        .and_then(|exe| Path::new(&exe).parent().map(Path::to_path_buf))
        .and_then(|dir| Archive::open(&dir, "enshrouded"));
    if let Some(archive) = archive {
        let keys = archive.keys_of(enshwick_gamedata::reflect::fnv1a32(CONTAINER.as_bytes()));
        let bytes: usize = keys
            .iter()
            .filter_map(|key| archive.resource(key))
            .map(|resource| resource.len())
            .sum();
        group.throughput(Throughput::Bytes(bytes as u64));
        group.sample_size(10);
        group.bench_function("attribute_containers", |b| {
            b.iter(|| {
                archive.trim();
                for key in &keys {
                    black_box(archive.resource(key));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, read);
criterion_main!(benches);
