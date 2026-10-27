//! Transactions of the world: commits, statuses and undo, as the loader holds them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, Status, TerrainEdit, TerrainMaterialInfo, TransactionInfo};
    use enshwick::terrain::Shape;
    use enshwick::world::{TransactionStatus, World, WorldError};
    use enshwick::{Load, Mod, Off};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Builder;

    impl Mod for Builder {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.builder", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Builder);

    fn rock() -> TerrainMaterialInfo {
        TerrainMaterialInfo {
            header: Header::of::<TerrainMaterialInfo>(),
            id: 9,
            flags: 0,
            hardness: 3,
            reserved: 0,
            item: enshwick::ItemId::new(77),
            health: 500,
        }
    }

    fn started() -> (enshwick::test::Installed, World) {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Builder)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (loader, *WORLD.get().unwrap())
    }

    #[test]
    fn a_transaction_reaches_the_loader_in_order_and_is_applied_on_its_steps() {
        let (loader, world) = started();
        let at = [10.0, 20.0, 30.0];
        assert_eq!(
            world
                .transaction()
                .terrain_dig(at, 2.0, Shape::Round, &[])
                .commit(),
            Err(WorldError::Missing),
            "no world runs yet"
        );
        loader.set_terrain(&[rock()]);

        let transaction = world
            .transaction()
            .terrain_dig(at, 4.0, Shape::Square, &[9])
            .terrain_add(at, 3.0, 1.5, 9, Shape::Round)
            .terrain_flatten(at, 2.0, 9, Shape::Round);
        assert_eq!(transaction.len(), 3);
        let id = transaction.commit().unwrap();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Pending)
        );

        let held = loader.transactions();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].id, id.get());
        let edits: Vec<TerrainEdit> = held[0]
            .changes
            .iter()
            .filter_map(|change| match change {
                enshwick::test::FakeChange::Terrain(edit) => Some(*edit),
                _ => None,
            })
            .collect();
        let kinds: Vec<u32> = edits.iter().map(|edit| edit.kind).collect();
        assert_eq!(
            kinds,
            [TerrainEdit::DIG, TerrainEdit::ADD, TerrainEdit::FLATTEN]
        );
        assert_eq!(edits[0].protect[1], 1 << 1, "material 9 is protected");
        assert!(
            loader.terrain_edits().is_empty(),
            "a transaction is no direct edit"
        );

        loader.run_transactions();
        assert_eq!(
            world.transaction_status(id),
            Some(TransactionStatus::Applied)
        );
    }

    #[test]
    fn an_undo_takes_back_an_applied_transaction_and_drops_a_queued_one() {
        let (loader, world) = started();
        loader.set_terrain(&[rock()]);
        let at = [10.0, 20.0, 30.0];

        let applied = world
            .transaction()
            .terrain_dig(at, 2.0, Shape::Round, &[])
            .commit()
            .unwrap();
        loader.run_transactions();
        world.undo(applied).unwrap();
        assert_eq!(
            world.transaction_status(applied),
            Some(TransactionStatus::Undoing)
        );
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(applied),
            Some(TransactionStatus::Undone)
        );
        assert_eq!(world.undo(applied), Err(WorldError::Refused), "undone once");

        let queued = world
            .transaction()
            .terrain_dig(at, 2.0, Shape::Round, &[])
            .commit()
            .unwrap();
        world.undo(queued).unwrap();
        assert_eq!(
            world.transaction_status(queued),
            Some(TransactionStatus::Undone),
            "nothing was written, so nothing waits"
        );
    }

    #[test]
    fn failures_and_refused_undos_show_in_the_status() {
        let (loader, world) = started();
        loader.set_terrain(&[rock()]);
        let at = [10.0, 20.0, 30.0];

        let far = world
            .transaction()
            .terrain_dig(at, 2.0, Shape::Round, &[])
            .commit()
            .unwrap();
        loader.fail_transaction(far.get(), Status::NOT_LOADED);
        assert_eq!(
            world.transaction_status(far),
            Some(TransactionStatus::Failed(WorldError::NotLoaded))
        );
        assert_eq!(
            world.undo(far),
            Err(WorldError::Refused),
            "it wrote nothing to take back"
        );

        let changed = world
            .transaction()
            .terrain_add(at, 2.0, 1.0, 9, Shape::Round)
            .commit()
            .unwrap();
        loader.run_transactions();
        world.undo(changed).unwrap();
        loader.fail_transaction(changed.get(), Status::REFUSED);
        assert_eq!(
            world.transaction_status(changed),
            Some(TransactionStatus::UndoRefused(WorldError::Refused))
        );
        world.undo(changed).unwrap();
        loader.run_transactions();
        assert_eq!(
            world.transaction_status(changed),
            Some(TransactionStatus::Undone),
            "a refused undo may be asked again"
        );
    }

    #[test]
    fn the_loader_refuses_what_it_cannot_write() {
        let (loader, world) = started();
        loader.set_terrain(&[rock()]);
        let at = [10.0, 20.0, 30.0];

        assert_eq!(world.transaction().commit(), Err(WorldError::Refused));
        let refused = [
            world.transaction().terrain_dig(at, 0.0, Shape::Round, &[]),
            world
                .transaction()
                .terrain_dig([-1.0, 0.0, 0.0], 2.0, Shape::Round, &[]),
            world
                .transaction()
                .terrain_add(at, 2.0, 1.0, 0x80, Shape::Round),
            world
                .transaction()
                .terrain_add(at, 2.0, 17.0, 9, Shape::Round),
            world
                .transaction()
                .terrain_dig(at, 2.0, Shape::Round, &[])
                .terrain_flatten(at, 17.0, 9, Shape::Round),
        ];
        for transaction in refused {
            assert_eq!(transaction.commit(), Err(WorldError::Refused));
        }
        assert!(loader.transactions().is_empty(), "nothing half-queued");
        let accepted = world
            .transaction()
            .terrain_dig(at, 1.0, Shape::Round, &[])
            .commit()
            .unwrap();
        assert_eq!(
            loader.transactions()[0].info.state,
            TransactionInfo::PENDING
        );
        assert!(world.transaction_status(accepted).is_some());
    }

    #[test]
    fn an_edit_past_the_world_s_edge_is_outside_the_world() {
        let loader = enshwick::test::FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2)
            .world_extent([200, 200, 200])
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Builder)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let world = *WORLD.get().unwrap();
        loader.set_terrain(&[rock()]);

        assert!(
            world
                .transaction()
                .terrain_dig([99.0, 10.0, 10.0], 1.0, Shape::Round, &[])
                .commit()
                .is_ok()
        );
        assert_eq!(
            world
                .transaction()
                .terrain_dig([10.0, 10.0, 10.0], 1.0, Shape::Round, &[])
                .terrain_dig([100.0, 10.0, 10.0], 1.0, Shape::Round, &[])
                .commit(),
            Err(WorldError::OutsideWorld)
        );
        assert_eq!(loader.transactions().len(), 1, "nothing half-queued");
    }

    #[test]
    fn the_fake_holds_the_loader_s_brick_cap_and_queue() {
        let (loader, world) = started();
        loader.set_terrain(&[rock()]);
        let wide = |count: u32| {
            (0..count).fold(world.transaction(), |transaction, i| {
                let x = 40.0 + 40.0 * f64::from(i);
                transaction.terrain_dig([x, 40.0, 40.0], 16.0, Shape::Round, &[])
            })
        };
        assert_eq!(
            wide(91).commit(),
            Err(WorldError::Refused),
            "more bricks than the cap"
        );
        wide(50).commit().unwrap();
        assert_eq!(
            wide(50).commit(),
            Err(WorldError::Refused),
            "the queue claims the cap"
        );
        loader.run_transactions();
        wide(50).commit().unwrap();
    }

    #[test]
    fn an_op_with_a_reserved_field_set_is_refused() {
        let (loader, _world) = started();
        loader.set_terrain(&[rock()]);
        let edit = TerrainEdit {
            header: Header::of::<TerrainEdit>(),
            kind: TerrainEdit::DIG,
            shape: TerrainEdit::ROUND,
            flags: 0,
            material: 0,
            position: [10.0; 3],
            radius: 1.0,
            height: 0.0,
            protect: [0; 16],
        };
        let op = enshwick::abi::TransactionOp {
            kind: enshwick::abi::TransactionOp::TERRAIN,
            reserved: 1,
            data: (&raw const edit).cast(),
        };
        let mut id = 0u64;
        // SAFETY: `api` points at the fake's table, which lives for the
        // process.
        let api = unsafe { &*loader.api() };
        let status =
            (api.transaction_commit)(enshwick::abi::ModHandle(1), &raw const op, 1, &raw mut id);
        assert_eq!(status, Status::BAD_ARGS);
    }

    #[test]
    fn the_fake_refuses_an_undo_while_queued_work_holds_the_cap() {
        let (loader, world) = started();
        loader.set_terrain(&[rock()]);
        let wide = |count: u32| {
            (0..count).fold(world.transaction(), |transaction, i| {
                let x = 40.0 + 40.0 * f64::from(i);
                transaction.terrain_dig([x, 40.0, 40.0], 16.0, Shape::Round, &[])
            })
        };
        let first = wide(50).commit().unwrap();
        loader.run_transactions();
        let _second = wide(50).commit().unwrap();
        assert_eq!(world.undo(first), Err(WorldError::Refused));
        loader.run_transactions();
        world.undo(first).unwrap();
    }
}
