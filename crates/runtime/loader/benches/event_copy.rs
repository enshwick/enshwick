//! What the server's event move costs the loader before the engine moves
//! the streams: one copy pass over 256 records of 32-byte events in one
//! stream, every event of the one type a handler asked for, or none.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick_loader::base::game_events::bench_server_copy;

/// Records in the stream.
const RECORDS: usize = 256;
/// Each event's bytes after its record header.
const PAYLOAD: usize = 32;
/// A record's header: its type hash, its size byte at 6, padding to 8.
const HEADER: usize = 8;
/// The stream layout in the build table's order, as `ecs.event_merge`
/// walks it on client 26430625: the stream array and its count in the
/// world, streams of 0x18 bytes with the bytes used at 0x10, the size byte
/// at 6 of an 8-byte header, records aligned to 8.
const OFFSETS: [usize; 7] = [0, 8, 0x18, 0x10, 6, HEADER, 8];
/// The type the handler asked for.
const WANTED: u32 = 42;

/// A world of one stream whose records are all of `type_hash`; the vectors
/// hold the memory the world points into.
fn world(type_hash: u32) -> (Vec<u64>, Vec<u64>, Vec<u8>) {
    let mut records = vec![0u8; RECORDS * (HEADER + PAYLOAD)];
    for record in records.as_chunks_mut::<{ HEADER + PAYLOAD }>().0 {
        record[..4].copy_from_slice(&type_hash.to_le_bytes());
        record[6] = u8::try_from(PAYLOAD).unwrap_or(u8::MAX);
    }
    let used = u64::try_from(records.len()).unwrap_or(u64::MAX);
    let stream = vec![records.as_ptr() as u64, 0, used];
    let world = vec![stream.as_ptr() as u64, 1];
    (world, stream, records)
}

fn copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_copy");
    for (name, type_hash) in [("selected", WANTED), ("unmatched", WANTED + 1)] {
        let (mut world, _stream, _records) = world(type_hash);
        group.bench_function(format!("server_{name}_256_records"), |b| {
            b.iter(|| {
                // SAFETY: `world` holds one stream laid out as `OFFSETS`
                // says, and nothing else touches it during the call.
                unsafe {
                    bench_server_copy(black_box(world.as_mut_ptr().cast()), OFFSETS, WANTED);
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, copy);
criterion_main!(benches);
