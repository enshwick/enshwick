//! Tests of the helpers' resolution and the query walk over its double.

use enshwick_core::abi::Side;

use super::double::Query;
use super::{Engine, Walk};
use crate::engine::pinned::ecs;

/// A build with only the query walk's entries.
fn walk_only() -> crate::engine::loader::Loader {
    crate::start::tests::pinning_loader(
        Side::CLIENT,
        &[ecs::ITERATOR_INIT, ecs::ITERATOR_NEXT],
        &[
            (ecs::CONTEXT_ROW, 8),
            (ecs::DESCRIPTOR_ENTITY_RECORDS, 0),
            (ecs::ENTITY_RECORD_SIZE, 4),
        ],
    )
}

#[test]
fn a_build_without_an_unrelated_helper_still_walks_and_names_the_helper_it_lacks() {
    let loader = walk_only();
    assert!(Walk::found(&loader).is_ok());
    assert_eq!(
        Engine::found(&loader).map(|_| ()),
        Err(ecs::MARK_CHANGED.id()),
        "the first entry the build lacks, not a guess"
    );
    let bare = crate::start::tests::bare_loader();
    assert_eq!(Walk::found(&bare).map(|_| ()), Err(ecs::ITERATOR_INIT.id()));
}

#[test]
fn a_build_missing_any_walk_or_component_read_entry_names_one_its_features_rows_list() {
    use crate::actions::{interaction_prompts, local_held};
    let symbols = [
        ecs::ITERATOR_INIT,
        ecs::ITERATOR_NEXT,
        ecs::TYPE_INDEX,
        ecs::COMPONENT_READ_RAW,
        ecs::SLOT_ROOT,
    ];
    let constants = [
        (ecs::CONTEXT_ROW, 8),
        (ecs::DESCRIPTOR_ENTITY_RECORDS, 0),
        (ecs::ENTITY_RECORD_SIZE, 4),
        (ecs::CONTEXT_ENTITY_WORLD, 0),
        (ecs::REGISTRY_TYPE_MAP, 0),
        (ecs::REGISTRY_TYPE_MAP_FALLBACK, 0),
    ];
    // Entry `at` of the symbols, then the constants, left out.
    let without = |at: usize| {
        let kept_symbols: Vec<_> = (0..symbols.len())
            .filter(|&of| of != at)
            .map(|of| symbols[of])
            .collect();
        let kept_constants: Vec<_> = (0..constants.len())
            .filter(|&of| symbols.len() + of != at)
            .map(|of| constants[of])
            .collect();
        crate::start::tests::pinning_loader(Side::CLIENT, &kept_symbols, &kept_constants)
    };
    let mut named = 0;
    for at in 0..symbols.len() + constants.len() {
        let loader = without(at);
        if let Err(id) = Walk::found(&loader) {
            named += 1;
            assert!(local_held::TABLE_IDS.contains(&id), "local_held lacks {id}");
            assert!(
                interaction_prompts::TABLE_IDS.contains(&id),
                "interaction_prompts lacks {id}"
            );
        }
        if let Err(id) = super::Components::found(&loader) {
            named += 1;
            assert!(local_held::TABLE_IDS.contains(&id), "local_held lacks {id}");
        }
    }
    assert!(
        named >= symbols.len() + constants.len(),
        "each missing entry is named"
    );
}

#[test]
fn a_walk_takes_each_row_s_entity_only_once_the_query_moved_to_it() {
    let walk = Query::walk();
    let mut double = Query::new(vec![(5, vec![50]), (9, vec![90])], 0);
    let ctx = double.ctx();
    // SAFETY: each call below takes the double's own context, on this thread.
    let entity = || unsafe { walk.entity(ctx) };
    assert_eq!(entity(), 0, "no row is taken before the walk");
    // SAFETY: the double's own context, on this thread.
    let mut query = unsafe { walk.query::<1>(ctx) };
    assert_eq!(query.next(), Some(&[50]));
    assert_eq!(entity(), 5);
    assert_eq!(query.next(), Some(&[90]));
    assert_eq!(entity(), 9);
    assert_eq!(query.next(), None);
    assert_eq!(double.row(), 2);
}

#[test]
fn a_walk_from_where_the_system_left_the_counter_finds_no_row_until_it_is_put_back() {
    let walk = Query::walk();
    let mut double = Query::new(vec![(5, vec![1])], 1);
    let ctx = double.ctx();
    // SAFETY: each call below takes the double's own context, on this thread.
    let first = || unsafe { walk.query::<1>(ctx) }.next().copied();
    assert_eq!(first(), None);
    // SAFETY: the double's own context, on this thread, written by nothing else.
    unsafe { walk.set_row(ctx, 0) };
    assert_eq!(first(), Some([1]));
}
