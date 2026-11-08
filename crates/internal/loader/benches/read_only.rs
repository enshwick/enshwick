//! What a typed read with blob fields pays to measure the read-only data it
//! decodes: one `pages::read_only` from a pointer into a read-only
//! allocation of 490 MB, as the game's template set-up data is on server
//! 26430253, once asking Windows and once from the run already found.

use criterion::{Criterion, criterion_group, criterion_main};

/// How far a typed read measures, as the API's typed reader does.
#[cfg(windows)]
const REACH: usize = 1 << 20;

#[cfg(windows)]
fn read_only(c: &mut Criterion) {
    use std::hint::black_box;

    use enshwick_loader::pages;
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READONLY, PAGE_READWRITE, VirtualAlloc,
        VirtualFree, VirtualProtect,
    };

    /// The one region of template set-up data on server 26430253.
    const SIZE: usize = 0x1ea0_0000;
    const PAGE: usize = 0x1000;

    // SAFETY: a fresh reservation touches no memory the process uses; it
    // returns null when Windows refuses.
    let base = unsafe {
        VirtualAlloc(
            core::ptr::null(),
            SIZE,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_READWRITE,
        )
    }
    .cast::<u8>();
    if base.is_null() {
        return;
    }
    // The game's pages are all resident, and the kernel walks them.
    for page in (0..SIZE).step_by(PAGE) {
        // SAFETY: `page` lies inside the `SIZE` read-write bytes just
        // committed at `base`, which nothing else uses.
        unsafe { base.wrapping_add(page).write(1) };
    }
    let mut old = 0;
    // SAFETY: the pages are the bench's own and nothing writes them again.
    unsafe { VirtualProtect(base.cast(), SIZE, PAGE_READONLY, &raw mut old) };
    // A blob pointer on that build lies about a tenth into the region.
    let address = base.addr() + SIZE / 10;

    let mut group = c.benchmark_group("read_only");
    group.sample_size(10);
    group.bench_function("asked_490_mb", |b| {
        b.iter(|| {
            pages::forget();
            black_box(pages::read_only(black_box(address), REACH))
        });
    });
    group.bench_function("found_490_mb", |b| {
        b.iter(|| black_box(pages::read_only(black_box(address), REACH)));
    });
    group.finish();
    // SAFETY: `base` is the bench's own allocation, released once.
    unsafe { VirtualFree(base.cast(), 0, MEM_RELEASE) };
}

#[cfg(not(windows))]
fn read_only(_: &mut Criterion) {}

criterion_group!(benches, read_only);
criterion_main!(benches);
