//! How long reading content out of the game's archive takes: an icon
//! decoded from BC7, and the attribute containers read and unpacked afresh
//! from the archive of the client `ENSH_CLIENT_EXE` names, skipped without it.

use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use enshwick::content::icons::{decode, formats};
use enshwick_gamedata::archive::Archive;
use enshwick_gamedata::reflect::{Image, Layout, Reflection, fnv1a32};

/// An icon's side in pixels.
const SIDE: u32 = 256;
/// The bytes of a BC7 icon of [`SIDE`] pixels: one byte a pixel.
const ICON_BYTES: usize = (SIDE * SIDE) as usize;
/// The resource type the loader reads attribute slots from.
const CONTAINER: &str = "keen::AttributeContainerResource";
/// The resource type the loader reads the terrain palette from.
const VOXEL_WORLD: &str = "keen::VoxelWorldResource";

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
        let keys = archive.keys_of(fnv1a32(CONTAINER.as_bytes()));
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
        voxel_worlds(&mut group, &archive);
    }
    group.finish();
}

/// Reads and decodes every voxel world of the client's archive, as the
/// loader does once at start to find the terrain palette, then builds each
/// again from its base.
fn voxel_worlds(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    archive: &Archive,
) {
    let Some(exe) = std::env::var_os("ENSH_CLIENT_EXE").and_then(|exe| std::fs::read(exe).ok())
    else {
        return;
    };
    let Ok(image) = Image::parse(&exe, Layout::File, None) else {
        return;
    };
    let registry = Reflection::scan(&image).registry();
    let keys = archive.keys_of(fnv1a32(VOXEL_WORLD.as_bytes()));
    let bytes: usize = keys
        .iter()
        .filter_map(|key| archive.resource(key))
        .map(|resource| resource.len())
        .sum();
    group.throughput(Throughput::Bytes(bytes as u64));
    group.bench_function("voxel_worlds", |b| {
        b.iter(|| {
            archive.trim();
            for key in &keys {
                if let Some(bytes) = archive.resource(key) {
                    black_box(registry.decode(VOXEL_WORLD, &bytes).ok());
                }
            }
        });
    });
    // A build of each from its base, as the loader's terrain patch made one
    // with no material to name.
    group.bench_function("voxel_worlds_built", |b| {
        b.iter(|| {
            archive.trim();
            for key in &keys {
                if let Some(bytes) = archive.resource(key)
                    && let Ok(value) = registry.decode(VOXEL_WORLD, &bytes)
                {
                    black_box(registry.encode(VOXEL_WORLD, &value, Some(&bytes)).ok());
                }
            }
        });
    });
}

criterion_group!(benches, read);
criterion_main!(benches);
