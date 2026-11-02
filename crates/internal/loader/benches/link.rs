//! What one message costs through the reliable link and back: queued, cut
//! into packets, read and delivered by the peer, and its ack read again.

use std::hint::black_box;
use std::time::Instant;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use enshwick_loader::wire::Link;

/// A packet of the size a session's MTU leaves, about.
const PACKET: usize = 1200;

fn link(c: &mut Criterion) {
    let mut group = c.benchmark_group("link");
    for size in [64usize, 6000] {
        let message = vec![7u8; size];
        let (Some(mut a), Some(mut b)) = (Link::new(PACKET), Link::new(PACKET)) else {
            return;
        };
        let now = Instant::now();
        group.throughput(Throughput::Bytes(u64::try_from(size).unwrap_or(0)));
        group.bench_function(format!("round_trip_{size}"), |bench| {
            bench.iter(|| {
                let mut delivered = 0;
                let _queued = a.send(black_box(&message));
                a.poll(now, usize::MAX, |packet| {
                    let _read = b.receive(packet, |m| delivered += m.len());
                });
                b.poll(now, usize::MAX, |packet| {
                    let _read = a.receive(packet, |_| {});
                });
                delivered
            });
        });
    }
    group.finish();
}

criterion_group!(benches, link);
criterion_main!(benches);
