//! `enshwick.buffs.add_buff`: a script's own buff type, as a Rust mod's.

#[cfg(test)]
mod tests {
    use enshwick::buffs::BuffTypeId;
    use enshwick::content::Id;
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    #[test]
    fn a_script_adds_buff_types_in_one_patch_and_a_bad_slot_or_a_repeat_raises_an_error() {
        let loader = run(
            FakeHost::new().buffs(&[BuffTypeId(0xf9b3_64f6)]),
            r#"
            local id = enshwick.buffs.add_buff({
                id = "you.luau:lucky",
                like = 0xf9b364f6,
                name = enshwick.content.guid("you.luau:lucky.name"),
                default_life_time = 300,
                slot = "Buff",
                plain = true,
            })
            enshwick.log.info(`added {id}`)
            local ok = pcall(enshwick.buffs.add_buff, {
                id = "you.luau:odd",
                like = 0xf9b364f6,
                slot = { Other = 300 },
            })
            enshwick.log.info(`odd {ok}`)
            enshwick.buffs.add_buff({ id = "you.luau:luckier", like = 0xf9b364f6 })
            local again = pcall(enshwick.buffs.add_buff, { id = "you.luau:lucky", like = 0xf9b364f6 })
            enshwick.log.info(`again {again}`)
            "#,
        );
        let id = BuffTypeId::from_guid(Id::new("you.luau:lucky").guid());
        assert!(
            logged(&loader, &format!("added {}", id.0)),
            "{:?}",
            loader.logs()
        );
        assert!(logged(&loader, "odd false"), "{:?}", loader.logs());
        assert!(logged(&loader, "again false"), "{:?}", loader.logs());
        assert_eq!(
            loader.resources().len(),
            1,
            "one patch of the collection for both types"
        );
    }
}
