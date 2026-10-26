//! How fast a code pattern is found in a code section: a synthetic 32 MiB
//! section, and the real one of `ENSH_SERVER_EXE` when it is set.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use enshwick::code::Pattern;

/// The size of the synthetic section, about the dedicated server's.
const SECTION: usize = 32 << 20;
/// A pattern with wildcards, as the build tables write them.
const PATTERN: &str = "48 89 5c 24 ?? 57 48 83 ec ?? 8b fa 48 8b d9 e8 ?? ?? ?? ??";

/// Pseudo-random code with the pattern's bytes once, at the end.
fn section() -> Vec<u8> {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut code: Vec<u8> = (0..SECTION)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect();
    let tail = [
        0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x20, 0x8b, 0xfa, 0x48, 0x8b, 0xd9,
        0xe8, 1, 2, 3, 4,
    ];
    let at = SECTION - tail.len();
    code[at..].copy_from_slice(&tail);
    code
}

#[expect(
    clippy::panic,
    reason = "without its pattern the bench measures nothing"
)]
fn scan(c: &mut Criterion) {
    let Ok(pattern) = PATTERN.parse::<Pattern>() else {
        panic!("{PATTERN} does not parse");
    };
    let code = section();
    let mut group = c.benchmark_group("pattern_scan");
    group.throughput(Throughput::Bytes(code.len() as u64));
    group.sample_size(20);
    group.bench_function("synthetic_32mib", |b| {
        b.iter(|| pattern.find_unique(black_box(&code)));
    });
    if let Some(exe) = std::env::var_os("ENSH_SERVER_EXE")
        && let Ok(bytes) = std::fs::read(exe)
    {
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function("server_exe", |b| {
            b.iter(|| pattern.find_all(black_box(&bytes)).count());
        });
    }
    group.finish();
}

criterion_group!(benches, scan);
criterion_main!(benches);
