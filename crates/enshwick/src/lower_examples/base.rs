//! The examples of `enshwick-base` that name items above its crate,
//! compiled here as written.
//!
//! `load::Loading`:
//!
//! ```
//! use enshwick::ecs::ComponentType;
//!
//! fn used_item(load: &mut enshwick::Load<'_>) -> Result<u32, enshwick::Off> {
//!     Ok(ComponentType::resolve(load, "keen::ecs::UsedItem")?.hash())
//! }
//! ```
//!
//! `config::table::TableConfig`:
//!
//! ```
//! use enshwick::config::{Field, Schema};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let limit = Field::new("limit", &4).with_doc("How many.", "");
//!     let (settings, defaults) = load.table_config(Schema::new("", "config", vec![limit]))?;
//!     assert_eq!(defaults["limit"].as_integer(), Some(4));
//!     settings.on_change(load, |values| enshwick::info!("limit {}", values["limit"]))
//! }
//! ```
//!
//! `ecs::Ecs`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Ecs, enshwick::Off> {
//!     enshwick::ecs::Ecs::resolve(load)
//! }
//! ```
//!
//! `ecs::ComponentType`:
//!
//! ```
//! use enshwick::ecs::ComponentType;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<ComponentType, enshwick::Off> {
//!     ComponentType::resolve(load, "keen::ecs::UsedItem")
//! }
//! ```
//!
//! `ecs::ComponentType::hash`:
//!
//! ```
//! fn hash(load: &enshwick::Load<'_>) -> Result<u32, enshwick::Off> {
//!     Ok(enshwick::ecs::ComponentType::resolve(load, "keen::ecs::UsedItem")?.hash())
//! }
//! ```
//!
//! `ecs::Components`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Components, enshwick::Off> {
//!     enshwick::ecs::Components::resolve(load)
//! }
//! ```
//!
//! `ecs::Worlds`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Worlds, enshwick::Off> {
//!     enshwick::ecs::Worlds::resolve(load)
//! }
//! ```
//!
//! `ecs::Attributes`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<enshwick::ecs::Attributes, enshwick::Off> {
//!     enshwick::ecs::Attributes::resolve(load)
//! }
//! ```
//!
//! `events::Kind`:
//!
//! ```
//! use enshwick::events::{Kind, PlayerJoined};
//! use enshwick::game_events::EntityDiedEvent;
//!
//! fn watch<E: Kind>(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<E>(|_| enshwick::debug!("{}", core::any::type_name::<E>()))
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     watch::<PlayerJoined>(load)?;
//!     watch::<EntityDiedEvent>(load)
//! }
//! ```
//!
//! `events::TickPeriod`:
//!
//! ```
//! use std::time::Duration;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let period = load.on_tick(Duration::from_secs(1), |_| enshwick::debug!("a second"))?;
//!     period.set_every(Duration::from_millis(250))
//! }
//! ```
//!
//! `facts::EngineFacts`:
//!
//! ```
//! use enshwick::content::EngineFacts;
//! use enshwick::content::items::ItemLayout;
//!
//! fn layout(facts: &impl EngineFacts) -> Result<ItemLayout, enshwick::Off> {
//!     ItemLayout::resolve(facts)
//! }
//! ```
//!
//! `game_events::EventType`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let died = load.game_event_type("keen::ecs::EntityDiedEvent")?;
//!     assert_eq!(died.name(), "keen::ecs::EntityDiedEvent");
//!     Ok(())
//! }
//! ```
//!
//! `game_events::EventType::hash`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let died = load.game_event_type("keen::ecs::EntityDiedEvent")?.hash();
//!     load.on_every_raw_game_event(move |event| {
//!         if event.type_hash() == died {
//!             enshwick::debug!("an entity died");
//!         }
//!     })
//! }
//! ```
//!
//! `game_events::EventType::size`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     assert_eq!(load.game_event_type("keen::ecs::EntityDiedEvent")?.size(), 16);
//!     Ok(())
//! }
//! ```
//!
//! `game_events::EventType::field`:
//!
//! ```
//! use enshwick::Entity;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let died = load.game_event_type("keen::ecs::EntityDiedEvent")?;
//!     let dead = died.field::<Entity>(load, "deadEntityId")?;
//!     load.on_raw_game_event(died, move |event| {
//!         if let Some(entity) = event.get(dead) {
//!             enshwick::info!("entity {} died", entity);
//!         }
//!     })
//! }
//! ```
//!
//! `game_events::EventType::inherited_field`:
//!
//! ```
//! use enshwick::Entity;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let hit = load.game_event_type("keen::ecs::HitEvent")?;
//!     let attacker = hit.inherited_field::<Entity>(load, "keen::ecs::BaseHitEvent", "rootSourceId")?;
//!     let _ = attacker;
//!     Ok(())
//! }
//! ```
//!
//! `game_events::Field<T>::offset`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let died = load.game_event_type("keen::ecs::EntityDiedEvent")?;
//!     assert_eq!(died.field::<u32>(load, "deadEntityId")?.offset(), 8);
//!     Ok(())
//! }
//! ```
//!
//! `game_events::kinds::HitEvent`:
//!
//! ```
//! use enshwick::game_events::HitEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<HitEvent>(|hit| {
//!         if hit.health_change < 0 {
//!             enshwick::debug!("{:?} took {} from {:?}", hit.target_id, -hit.health_change, hit.root_source_id);
//!         }
//!     })
//! }
//! ```
//!
//! `game_events::kinds::EntityDiedEvent`:
//!
//! ```
//! use enshwick::game_events::EntityDiedEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<EntityDiedEvent>(|died| {
//!         enshwick::debug!("entity {} died", died.dead_entity_id);
//!     })
//! }
//! ```
//!
//! `game_events::kinds::LevelUpEvent`:
//!
//! ```
//! use enshwick::game_events::LevelUpEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<LevelUpEvent>(|up| enshwick::info!("entity {} reached level {}", up.player_entity_id.0, up.level))
//! }
//! ```
//!
//! `game_events::kinds::XpGainedEvent`:
//!
//! ```
//! use enshwick::game_events::{XpGainedEvent, ExperienceSourceType};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<XpGainedEvent>(|gained| {
//!         if gained.source == ExperienceSourceType::Combat {
//!             enshwick::debug!("{} experience for a kill", gained.amount);
//!         }
//!     })
//! }
//! ```
//!
//! `game_events::kinds::CraftRecipeEvent`:
//!
//! ```
//! use enshwick::game_events::CraftRecipeEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<CraftRecipeEvent>(|craft| enshwick::debug!("recipe {:#x} x{}", craft.recipe_id, craft.recipe_amount))
//! }
//! ```
//!
//! `game_events::kinds::CraftRecipeSuccessEvent`:
//!
//! ```
//! use enshwick::game_events::CraftRecipeSuccessEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<CraftRecipeSuccessEvent>(|done| enshwick::debug!("workshop {:#x} made {:#x}", done.workshop_id, done.recipe_id))
//! }
//! ```
//!
//! `game_events::kinds::AcceptOfferedInteractionEvent`:
//!
//! ```
//! use enshwick::game_events::AcceptOfferedInteractionEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<AcceptOfferedInteractionEvent>(|used| enshwick::debug!("{:?} used {:?}", used.guest_entity_id, used.host_entity_id))
//! }
//! ```
//!
//! `game_events::kinds::BuildingPlaceEvent`:
//!
//! ```
//! use enshwick::game_events::BuildingPlaceEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<BuildingPlaceEvent>(|built| enshwick::debug!("{:?} built at {:?}", built.block.owner_id, built.block.position))
//! }
//! ```
//!
//! `game_events::kinds::BuildingTearDownEvent`:
//!
//! ```
//! use enshwick::game_events::BuildingTearDownEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<BuildingTearDownEvent>(|torn| enshwick::debug!("torn down at {:?}", torn.block.position))
//! }
//! ```
//!
//! `game_events::kinds::BuildingPickUpEvent`:
//!
//! ```
//! use enshwick::game_events::BuildingPickUpEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<BuildingPickUpEvent>(|picked| enshwick::debug!("picked up {:?}", picked.picked_entity_id))
//! }
//! ```
//!
//! `game_events::kinds::FlameAltarEvent`:
//!
//! ```
//! use enshwick::game_events::FlameAltarEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<FlameAltarEvent>(|used| enshwick::info!("{:?} at altar {:?}", used.r#type, used.base_entity_id))
//! }
//! ```
//!
//! `game_events::kinds::LootInteractionEvent`:
//!
//! ```
//! use enshwick::game_events::LootInteractionEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<LootInteractionEvent>(|loot| enshwick::debug!("{:?} looted {:?}", loot.guest_id, loot.host_id))
//! }
//! ```
//!
//! `game_events::kinds::HandleEquipmentSlotChangedEvent`:
//!
//! ```
//! use enshwick::game_events::HandleEquipmentSlotChangedEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<HandleEquipmentSlotChangedEvent>(|changed| enshwick::debug!("{:?} of {:?}", changed.equipment_slot, changed.player_entity_id))
//! }
//! ```
//!
//! `game_events::kinds::RevivedPlayerEvent`:
//!
//! ```
//! use enshwick::game_events::RevivedPlayerEvent;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<RevivedPlayerEvent>(|revived| enshwick::info!("{:?} revived by {:?}", revived.target_entity_id, revived.source_entity_id))
//! }
//! ```
//!
//! `mods::join_missing`:
//!
//! ```
//! use enshwick::events::JoinRefused;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.on::<JoinRefused>(|()| {
//!         for entry in enshwick::mods::join_missing() {
//!             enshwick::info!("the server runs {} {}", entry.id, entry.version);
//!         }
//!     })
//! }
//! ```
//!
//! `net`:
//!
//! ```
//! use enshwick::net::Peer;
//!
//! #[derive(serde::Serialize, serde::Deserialize)]
//! enum Sync {
//!     Hello { version: u32 },
//!     Marker { at: [f64; 3] },
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let markers = load.channel::<Sync, _>("markers", 1, |from, message| match (from, message) {
//!         (Peer::Player(key), Sync::Hello { version }) => {
//!             enshwick::info!("player {key:016x} runs version {version}");
//!         }
//!         (Peer::Server, Sync::Marker { at }) => enshwick::debug!("marker at {at:?}"),
//!         _ => {}
//!     })?;
//!     // On a client: greet the server once it takes messages.
//!     if markers.is_ready(Peer::Server) {
//!         let _sent = markers.send(Peer::Server, &Sync::Hello { version: 1 });
//!     }
//!     Ok(())
//! }
//! ```
//!
//! `pacing`:
//!
//! ```
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let found = load.on_loop_wait(|wait| {
//!         if wait.ms == 0 && wait.left_ns > 0 {
//!             // Sleep `left_ns` here instead of letting the game poll.
//!         }
//!     })?;
//!     if !found.handles {
//!         enshwick::warn!("the event loop's wait was not found");
//!     }
//!     Ok(())
//! }
//! ```
//!
//! `points::OwnedPoint`:
//!
//! ```
//! #[enshwick::point(name = "price", mode = Override, output = u32)]
//! #[derive(Clone, Copy, Debug)]
//! pub struct Price {
//!     pub item: u32,
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let price = load.define_point::<Price>()?;
//!     let coins = price.decide(&Price { item: 7 }, 10).value_or(10);
//!     enshwick::info!("item 7 costs {coins}");
//!     Ok(())
//! }
//! ```
//!
//! `points::OwnedPoint<P>::is_extended`:
//!
//! ```
//! #[enshwick::point(name = "price", mode = Override, output = u32)]
//! #[derive(Clone, Copy, Debug)]
//! pub struct Price {
//!     pub item: u32,
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let price = load.define_point::<Price>()?;
//!     assert!(!price.is_extended(), "links run once every mod is initialised");
//!     Ok(())
//! }
//! ```
//!
//! `resources::read::Host::game_resource`:
//!
//! ```
//! use enshwick::content::{Guid, types};
//!
//! if let Some(host) = enshwick::host() {
//!     let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
//!     if let Ok(key) = types::ITEM_INFO.key(chest) {
//!         let _ = host.game_resource(key);
//!     }
//! }
//! ```
//!
//! `resources::read::Host::served_resource`:
//!
//! ```
//! use enshwick::content::{Guid, types};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let resin = types::ITEM_INFO.key(Guid::parse("2f04a05f-697d-489b-a32f-76bb3439f5bc"))?;
//!     let wood = types::ITEM_INFO.key(Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"))?;
//!     let host = load.host();
//!     load.serve(resin, resin, move |resin| {
//!         // Wood as other mods serve it; inside a build of wood, resin
//!         // stays as it is rather than failing.
//!         if let Ok(wood) = host.served_resource(wood) {
//!             enshwick::info!("wood is served as {} bytes", wood.len());
//!         }
//!         Some(resin.to_vec())
//!     })
//! }
//! ```
//!
//! `resources::table::Host::served_value`:
//!
//! ```
//! use enshwick::content::{Guid, types};
//!
//! if let Some(host) = enshwick::host() {
//!     let wood = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
//!     if let Ok(key) = types::ITEM_INFO.key(wood) {
//!         let _value = host.served_value(key);
//!     }
//! }
//! ```
//!
//! `schedule::Scheduler`:
//!
//! ```
//! use std::ops::ControlFlow;
//! use std::time::Duration;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let scheduler = load.scheduler()?;
//!     scheduler.after(Duration::from_secs(5), || enshwick::info!("five seconds in"));
//!     scheduler.every(Duration::from_secs(60), || {
//!         enshwick::info!("a minute passed");
//!         ControlFlow::Continue(())
//!     });
//!     Ok(())
//! }
//! ```
//!
//! `script::events::GameEventKind::subscribe`:
//!
//! ```rust,standalone_crate
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let died = enshwick::script::game_events()
//!         .find(|e| e.name() == "EntityDiedEvent")
//!         .ok_or(enshwick::Off::new("no such event"))?;
//!     died.subscribe(load, |data| enshwick::debug!("{:?}", data()))
//! }
//! ```
//!
//! `services::Emitter`:
//!
//! ```
//! #[enshwick::event(name = "sold")]
//! #[derive(Clone, Copy, Debug)]
//! pub struct Sold {
//!     pub item: u32,
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let sold = load.define_event::<Sold>()?;
//!     sold.send(&Sold { item: 7 });
//!     Ok(())
//! }
//! ```
//!
//! `state::WorldStore`:
//!
//! ```
//! #[derive(Default, serde::Deserialize, serde::Serialize)]
//! struct Bounties {
//!     paid: u64,
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let bounties = load.world_store::<Bounties>("bounties")?;
//!     load.on::<enshwick::events::PlayerJoined>(move |_| {
//!         let _paid = bounties.update(|b| b.paid += 1);
//!     })
//! }
//! ```
//!
//! `time`:
//!
//! ```
//! use std::time::Duration;
//!
//! use enshwick::time::StepClock;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let clock = load.step_clock()?;
//!     let started = clock.now();
//!     load.on::<enshwick::events::Ticked>(move |_| {
//!         if clock.now().saturating_sub(started) > Duration::from_secs(10) {
//!             enshwick::info!("ten seconds of world time");
//!         }
//!     })
//! }
//! ```
