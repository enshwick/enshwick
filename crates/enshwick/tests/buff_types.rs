//! A buff type of a mod's own: served with every mod's types in one patch of
//! the game's buff collection, found by its id, and naming its icon only on
//! a client that shows served icons; what the game could not take is refused
//! at init and the mod stays on, and no mod adds a type under another's id.

#[cfg(test)]
mod tests {
    use core::time::Duration;
    use std::sync::Mutex;

    use enshwick::abi::{BuffTypeAdd, Header, HostApi, ModHandle, Side, Status, Str};
    use enshwick::buffs::{
        BUFF_COLLECTION, BUFF_COLLECTION_RESOURCE, BuffLayout, BuffSlotType, BuffTypeId, NewBuff,
    };
    use enshwick::content::icons::{Icon, Pixels, Tint};
    use enshwick::content::{Blob, EngineFacts, Guid, Id, game_resource};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};

    const LUCKY: Id = Id::new("you.lucky:lucky");
    const LUCKY_NAME: Id = Id::new("you.lucky:lucky.name");
    const ICON: Id = Id::new("you.lucky:lucky.icon");
    /// An id whose game id the vanilla collection of these tests holds.
    const CLASH: Id = Id::new("you.lucky:clash");
    /// A vanilla food type of client 26430625.
    const FOOD: BuffTypeId = BuffTypeId(0xf9b3_64f6);
    const FOOD_ICON: Guid = Guid::parse("02778393-2031-44cd-b7cd-3bc215acd7af");
    /// A mod loaded before Lucky.
    const EARLIER: ModHandle = ModHandle(77);
    /// The earlier mod's buff type.
    const GLOOMY: Id = Id::new("you.gloomy:gloomy");

    /// What each `add_buff` answered, in order: Lucky, Lucky again, a copy
    /// of a type the game lacks, and an id the game has.
    static ADDED: Mutex<Vec<Result<BuffTypeId, String>>> = Mutex::new(Vec::new());

    struct Lucky;

    impl Mod for Lucky {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let icon = Icon {
                id: ICON,
                like: FOOD_ICON,
                pixels: Pixels::Tint(Tint {
                    colour: [60, 200, 90],
                    amount: 70,
                }),
            };
            let lucky = NewBuff::new(LUCKY, FOOD)
                .name(LUCKY_NAME.guid())
                .icon(icon)
                .default_life_time(Duration::from_mins(5))
                .slot(BuffSlotType::Buff)
                .plain();
            let answers = [
                load.add_buff(lucky.clone()),
                load.add_buff(lucky),
                load.add_buff(NewBuff::new(Id::new("you.lucky:odd"), BuffTypeId(0x99))),
                load.add_buff(NewBuff::new(CLASH, FOOD)),
            ];
            *ADDED.lock().unwrap() = answers
                .into_iter()
                .map(|added| added.map_err(|off| off.to_string()))
                .collect();
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Lucky,
        r#"
        id = "you.lucky"
        side = "both"
        "#
    );

    fn host() -> FakeHost {
        FakeHost::new().buffs(&[FOOD, BuffTypeId::from_guid(CLASH.guid())])
    }

    fn start(host: FakeHost) -> Installed {
        let loader = host.install();
        // SAFETY: the exports of this file's `declare_mod!(Lucky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The new type as the built collection holds it: its bytes, found
    /// through the key list the way the game finds it.
    fn built_lucky(loader: &Installed, layout: &BuffLayout, hash: u32) -> Vec<u8> {
        let key = BUFF_COLLECTION.key_by_hash(hash);
        let base = game_resource(key).expect("the vanilla collection");
        lucky_in(&loader.build_resource(&key, &base).unwrap(), layout)
    }

    /// The new type as the collection `bytes` hold it, found through the key
    /// list the way the game finds it.
    fn lucky_in(bytes: &[u8], layout: &BuffLayout) -> Vec<u8> {
        let built = Blob::new(bytes);
        let id = BuffTypeId::from_guid(LUCKY.guid());
        let (keys, count) = built.array(layout.keys).unwrap();
        let (values, _) = built.array(layout.values).unwrap();
        let (buffs, _) = built.array(layout.buffs).unwrap();
        let i = (0..count as usize)
            .find(|i| built.read_u32(keys + i * 4) == Some(id.0))
            .expect("the map lists the new type");
        let at = buffs + built.read_u32(values + i * 4).unwrap() as usize * layout.size;
        built.bytes()[at..at + layout.size].to_vec()
    }

    #[test]
    fn the_new_type_is_served_under_its_id_with_its_name_and_time() {
        let host = host();
        let layout = BuffLayout::resolve(&host).unwrap();
        let hash = host.hash(BUFF_COLLECTION_RESOURCE.id()).unwrap();
        let loader = start(host);
        let id = BuffTypeId::from_guid(LUCKY.guid());
        assert_eq!(ADDED.lock().unwrap().first(), Some(&Ok(id)));
        let lucky = built_lucky(&loader, &layout, hash);
        assert_eq!(&lucky[layout.id..layout.id + 4], &id.0.to_le_bytes());
        assert_eq!(&lucky[layout.name..layout.name + 16], &LUCKY_NAME.guid().0);
        assert_eq!(lucky[layout.slot], 1);
        let life = i64::from_le_bytes(
            lucky[layout.life_time..layout.life_time + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(life, 300_000_000_000);
        assert_eq!(
            &lucky[layout.icon..layout.icon + 16],
            &[0; 16],
            "the vanilla icon until this client shows served ones"
        );
    }

    #[test]
    fn what_the_game_could_not_take_is_refused_at_init_and_the_mod_stays_on() {
        let loader = start(host());
        let added = ADDED.lock().unwrap().clone();
        let why = |i: usize| added[i].clone().expect_err("refused");
        assert!(why(1).contains("another id"), "{added:?}");
        assert!(why(2).contains("no buff type"), "{added:?}");
        assert!(why(3).contains("another id"), "{added:?}");
        assert_eq!(loader.resources().len(), 1, "one patch of the collection");
    }

    /// A type an earlier mod adds through the table, as the loader takes it.
    fn earlier_type(id: Id, like: BuffTypeId) -> BuffTypeAdd {
        BuffTypeAdd {
            header: Header::of::<BuffTypeAdd>(),
            guid: id.guid(),
            name: Guid::NIL,
            description: Guid::NIL,
            icon: Guid::NIL,
            life_time_ns: 0,
            like,
            changes: 0,
            slot: 0,
            flags: 0,
        }
    }

    #[test]
    fn two_mods_types_are_served_in_one_patch_and_another_mod_s_id_is_refused() {
        let host = host().pack(EARLIER, Side::BOTH);
        let layout = BuffLayout::resolve(&host).unwrap();
        let hash = host.hash(BUFF_COLLECTION_RESOURCE.id()).unwrap();
        let loader = host.install();
        // SAFETY: the installed fake's table, alive while `loader` is.
        let api = unsafe { &*loader.api() };
        let mut id = BuffTypeId(0);
        let gloomy = earlier_type(GLOOMY, FOOD);
        assert_eq!(
            (api.buff_type_add)(EARLIER, &raw const gloomy, &raw mut id),
            Status::REFUSED,
            "a GUID the mod did not declare"
        );
        let (mut guid, gloomy_id) = (Guid::NIL, GLOOMY.to_string());
        assert_eq!(
            (api.declare_content)(EARLIER, Str::new(&gloomy_id), &raw mut guid),
            Status::OK
        );
        assert_eq!(
            (api.buff_type_add)(EARLIER, &raw const gloomy, &raw mut id),
            Status::OK
        );
        let taken = earlier_type(LUCKY, FOOD);
        assert_eq!(
            (api.buff_type_add)(EARLIER, &raw const taken, &raw mut id),
            Status::REFUSED,
            "Lucky's id, which only Lucky declares"
        );
        // SAFETY: the exports of this file's `declare_mod!(Lucky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            (api.buff_type_add)(EARLIER, &raw const taken, &raw mut id),
            Status::TOO_LATE,
            "after init a call is late before its id is looked at"
        );
        let added = ADDED.lock().unwrap().clone();
        assert_eq!(
            added[0],
            Ok(BuffTypeId::from_guid(LUCKY.guid())),
            "Lucky's own type"
        );
        assert_eq!(loader.resources().len(), 1, "one patch for both mods");
        let key = BUFF_COLLECTION.key_by_hash(hash);
        let base = game_resource(key).expect("the vanilla collection");
        let built = Blob::new(&loader.build_resource(&key, &base).unwrap());
        let (keys, count) = built.array(layout.keys).unwrap();
        let listed: Vec<u32> = (0..count as usize)
            .filter_map(|i| built.read_u32(keys + i * 4))
            .collect();
        for id in [GLOOMY.guid(), LUCKY.guid()] {
            assert!(listed.contains(&BuffTypeId::from_guid(id).0), "{listed:x?}");
        }
        assert_eq!(
            count, 4,
            "the two vanilla types, the earlier mod's and Lucky's"
        );
    }

    #[test]
    fn another_mod_declaring_the_mod_s_content_id_is_refused_and_turned_off() {
        /// What the earlier mod's calls answered, in order.
        static ANSWERS: Mutex<Vec<Status>> = Mutex::new(Vec::new());
        /// Lucky's own init, then calls as the earlier mod, as a C mod would
        /// before the mods' registrations close.
        unsafe extern "system" fn earlier_after(api: *const HostApi, me: ModHandle) -> Status {
            // SAFETY: the init export of this file's `declare_mod!(Lucky)`,
            // with the arguments the fake passed.
            let status = unsafe { enshwick_mod_init(api, me) };
            // SAFETY: the fake passes its table, alive for the call.
            let host = unsafe { &*api };
            let (mut guid, mut id, lucky) = (Guid::NIL, BuffTypeId(0), LUCKY.to_string());
            let taken = earlier_type(LUCKY, FOOD);
            let answers = [
                (host.declare_content)(EARLIER, Str::new(&lucky), &raw mut guid),
                (host.buff_type_add)(EARLIER, &raw const taken, &raw mut id),
                (host.declare_content)(EARLIER, Str::new(&lucky), core::ptr::null_mut()),
            ];
            if let Ok(mut kept) = ANSWERS.lock() {
                kept.extend(answers);
            }
            status
        }
        let loader = host().pack(EARLIER, Side::BOTH).install();
        // SAFETY: the query and ready exports of this file's
        // `declare_mod!(Lucky)`, with an init that runs Lucky's first.
        let status = unsafe { loader.start(enshwick_mod_query, earlier_after, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(
            *ANSWERS.lock().unwrap(),
            [Status::REFUSED, Status::FAULTED, Status::FAULTED],
            "Lucky's id lies outside the earlier mod's namespace, so its \
             declare turns it off and the loader answers its later calls so"
        );
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("outside the mod's namespace")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn the_type_names_its_own_icon_once_icons_show() {
        let host = host().ui_images().icons_ready();
        let layout = BuffLayout::resolve(&host).unwrap();
        let hash = host.hash(BUFF_COLLECTION_RESOURCE.id()).unwrap();
        let loader = start(host);
        let lucky = built_lucky(&loader, &layout, hash);
        assert_eq!(&lucky[layout.icon..layout.icon + 16], &ICON.guid().0);
    }

    #[test]
    fn a_rebuild_once_icons_show_names_the_icon_and_keeps_the_length() {
        let host = host().ui_images();
        let layout = BuffLayout::resolve(&host).unwrap();
        let hash = host.hash(BUFF_COLLECTION_RESOURCE.id()).unwrap();
        let loader = start(host);
        let key = BUFF_COLLECTION.key_by_hash(hash);
        let base = game_resource(key).expect("the vanilla collection");
        let before = loader.build_resource(&key, &base).unwrap();
        let lucky = built_lucky(&loader, &layout, hash);
        assert_eq!(
            &lucky[layout.icon..layout.icon + 16],
            &[0; 16],
            "the vanilla icon"
        );
        loader.show_icons();
        let held = loader.held_resource(&key).expect("kept for the rebuild");
        assert_eq!(held.len(), before.len(), "written over the game's copy");
        let lucky = lucky_in(&held, &layout);
        assert_eq!(&lucky[layout.icon..layout.icon + 16], &ICON.guid().0);
    }
}
