//! What the server's step pays for controls that follow game actions: eight
//! players and two linked controls with nothing pressed, the same with one
//! press and its delivery after the step, and the look at no player.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::Entity;
use enshwick::abi::ControlSpec;
use enshwick_loader::linked_controls::{Follow, Players};

const PLAYERS: [Entity; 8] = [
    Entity(11),
    Entity(12),
    Entity(13),
    Entity(14),
    Entity(15),
    Entity(16),
    Entity(17),
    Entity(18),
];

fn follows() -> [Follow; 2] {
    let fires = ControlSpec::PRESS | ControlSpec::RELEASE;
    [
        Follow {
            index: 0,
            bit: 5,
            hold_ms: 500,
            fires,
        },
        Follow {
            index: 1,
            bit: 9,
            hold_ms: 500,
            fires,
        },
    ]
}

fn linked_controls(c: &mut Criterion) {
    let follows = follows();
    let mut group = c.benchmark_group("linked_controls");
    let mut idle = Players::new();
    group.bench_function("step_no_player", |b| {
        b.iter(|| idle.step(black_box(&follows), &[], 0, |_| Some(0)));
    });
    let mut quiet = Players::new();
    let mut now = 0;
    group.bench_function("step_eight_players_nothing_pressed", |b| {
        b.iter(|| {
            now += 50;
            quiet.step(black_box(&follows), black_box(&PLAYERS), now, |_| Some(0));
            quiet.waiting()
        });
    });
    let mut pressing = Players::new();
    let mut down = false;
    group.bench_function("step_eight_players_one_press_and_delivery", |b| {
        b.iter(|| {
            now += 50;
            down = !down;
            let one = if down { 1 << 5 } else { 0 };
            pressing.step(black_box(&follows), black_box(&PLAYERS), now, |entity| {
                Some(if entity == PLAYERS[0] { one } else { 0 })
            });
            pressing.settle(Some).len()
        });
    });
    group.finish();
}

criterion_group!(benches, linked_controls);
criterion_main!(benches);
