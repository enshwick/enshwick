//! An example mod with a buff type of its own: "Lucky", a copy of a food
//! buff with a green icon, its own name and a luck modifier, which a control
//! on L gives the player for five minutes through a server action.

use core::time::Duration;

use enshwick::actions::{Action, Outcome};
use enshwick::buffs::{Apply, BuffSlotType, BuffTypeId, NewBuff};
use enshwick::content::icons::{Icon, Pixels, Tint};
use enshwick::content::{Guid, Id, id};
use enshwick::controls::{Control, Key};
use enshwick::events::WorldLoaded;
use enshwick::i18n::Texts;
use enshwick::stats::{Modifier, Op, StatDef};
use enshwick::{Load, Mod, Off};

/// The buff type.
const LUCKY: Id = id!("lucky");
/// The tags of its name and description.
const LUCKY_NAME: Id = LUCKY.part("name");
const LUCKY_DESCRIPTION: Id = LUCKY.part("description");
/// Its icon.
const LUCKY_ICON: Id = LUCKY.part("icon");
/// The food buff it copies, ten minutes of the game's own (client 26430625).
const FOOD: BuffTypeId = BuffTypeId(0xf9b3_64f6);
/// That food buff's icon, which the new one recolours.
const FOOD_ICON: Guid = Guid::parse("02778393-2031-44cd-b7cd-3bc215acd7af");

/// The mod's texts.
const TEXTS: &str = r#"
[en]
"lucky.name" = "Lucky"
"lucky.description" = "Fortune smiles on you."
"luck.name" = "Luck"
"feel-lucky" = "Feel lucky"
"lucky-now" = "{name} feels lucky."
"no-luck" = "No luck right now."
[uk]
"lucky.name" = "Удача"
"lucky.description" = "Фортуна вам усміхається."
"luck.name" = "Удача"
"feel-lucky" = "Відчути удачу"
"lucky-now" = "{name} відчуває удачу."
"no-luck" = "Зараз удачі немає."
"#;

/// The mod: everything happens in its handlers.
#[derive(Debug)]
pub struct Lucky;

impl Mod for Lucky {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = Texts::parse(TEXTS)?;
        load.add_texts(
            &texts,
            &[
                ("lucky.name", LUCKY_NAME),
                ("lucky.description", LUCKY_DESCRIPTION),
            ],
        )?;
        let luck = load.define_stat(id!("luck"), &StatDef::additive().range(0.0, 1.0))?;
        let icon = Icon::new(
            LUCKY_ICON,
            FOOD_ICON,
            Pixels::Tint(Tint::new([60, 200, 90], 70)),
        );
        let lucky = load.add_buff(
            NewBuff::new(LUCKY, FOOD)
                .name(LUCKY_NAME.guid())
                .description(LUCKY_DESCRIPTION.guid())
                .icon(icon)
                .default_life_time(Duration::from_mins(5))
                .slot(BuffSlotType::Buff)
                .plain(),
        )?;
        load.add_buff_modifiers(lucky, &[Modifier::new(luck, Op::Add, 0.1)])?;

        let buffs = load.buffs();
        let actions = load.actions(Texts::parse(TEXTS)?)?;
        actions.add(
            Action::new("lucky", "feel-lucky"),
            move |request| match buffs.apply(request.player.entity, lucky, Apply::new()) {
                Ok(()) => {
                    Outcome::done(request.say("lucky-now", &[("name", &request.player.name)]))
                }
                Err(why) => {
                    enshwick::warn!("no luck for {}: {why}", request.player.name);
                    Outcome::refused(request.say("no-luck", &[]))
                }
            },
        )?;
        let language = load.host().language();
        let label = texts
            .get("feel-lucky", language)
            .unwrap_or("Feel lucky")
            .to_owned();
        load.control(
            Control::new("feel-lucky", &label).key(Key::parse("L")?),
            move |_| {
                if let Err(why) = actions.run("lucky", &[]) {
                    enshwick::warn!("the luck did not reach the server: {why}");
                }
            },
        )?;

        // Says at each world load whether the game knows the type.
        load.on::<WorldLoaded>(move |()| {
            let types = match buffs.types() {
                Ok(types) => types,
                Err(why) => {
                    enshwick::warn!("the buff types are not readable: {why}");
                    return;
                }
            };
            match types.iter().find(|ty| ty.buff_type_id == lucky) {
                Some(ty) => enshwick::info!(
                    "buff type {lucky:?} is in the game: {:?}, {:?}",
                    ty.slot,
                    ty.default_life_time
                ),
                None => enshwick::warn!("buff type {lucky:?} is not in the game"),
            }
        })?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Lucky);

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::buffs::BUFF_COLLECTION_RESOURCE;
    use enshwick::content::EngineFacts;
    use enshwick::controls::Key;

    use super::*;

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: the exports of this crate's `declare_mod!(Lucky)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn the_buff_type_is_served_and_l_asks_the_server_for_it() {
        let host = enshwick::test::FakeHost::new()
            .buffs(&[FOOD])
            .side(enshwick::abi::Side::CLIENT);
        let collection = host.hash(BUFF_COLLECTION_RESOURCE.id()).unwrap();
        let loader = start(host);
        assert!(
            loader
                .resources()
                .iter()
                .any(|served| served.key.type_hash == collection),
            "the collection is served"
        );
        let l = Key::parse("L").unwrap();
        assert_eq!(loader.keys(&[l], Duration::from_millis(16)), 1);
        assert_eq!(loader.action_runs(), [("lucky".to_owned(), String::new())]);
    }

    #[test]
    fn each_world_load_says_whether_the_game_knows_the_type() {
        use enshwick::abi::BuffTypeInfo;
        use enshwick::events::WorldLoaded;

        let loader = start(enshwick::test::FakeHost::new().buffs(&[FOOD]));
        let found = |loader: &enshwick::test::Installed, line: &str| {
            loader
                .logs()
                .iter()
                .filter(|(_, l)| l.contains(line))
                .count()
        };
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        assert_eq!(found(&loader, "is not in the game"), 1);
        let lucky = BuffTypeId::from_guid(LUCKY.guid());
        let ty = enshwick::abi::headed!(BuffTypeInfo {
            id: lucky,
            slot: BuffTypeInfo::BUFF,
            flags: 0,
            name: 0,
            duration_ns: 300_000_000_000,
        });
        loader.set_buffs(&[ty], &[]);
        assert_eq!(loader.raise::<WorldLoaded>(&()), 1);
        assert_eq!(
            found(&loader, "is in the game"),
            1,
            "the next world is checked too: {:?}",
            loader.logs()
        );
    }
}
