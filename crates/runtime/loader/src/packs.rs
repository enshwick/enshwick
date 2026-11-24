//! The packs domain: data packs as mods.

#[cfg_attr(
    not(windows),
    expect(
        dead_code,
        reason = "only the Windows build loads packs; tests off it reach discovery and the hash"
    )
)]
pub(crate) mod data_packs;

/// The packs domain's parts: data packs load as mods; their files' hash
/// counts in the join versions, and an off pack's runtime goes.
#[cfg(windows)]
pub(crate) static PARTS: &[&crate::engine::parts::Part] = &[&crate::engine::parts::Part {
    seal: Some(|_| crate::base::join::client::on_pack_hash(data_packs::hash_of)),
    turned: Some(|_, record, off| {
        if off && record.pack {
            data_packs::forget(&record.id);
        }
    }),
    ..crate::engine::parts::part!("data_packs", data_packs)
}];
