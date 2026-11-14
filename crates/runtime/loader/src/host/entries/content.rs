//! Host functions for the content domain: recipes, data changes, texts, item
//! data, content ids, rates and the game's memory pages.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ModHandle, RateId, Status, Str};
use enshwick_core::content::changes::{Change, Field, ItemsChange, RecipesChange, Value};

use crate::host::entries::base::services::exported;
use crate::host::layer::guards::before_delivery;
use crate::host::layer::{
    BytesOut, Caller, Headed, HeadedSince, In, List, Mod, Out, RuleOff, Text, checked, entries,
};
use crate::host::memory::text;

pub(crate) mod resources;

/// Where the fields of a data change every reader requires end: `item`,
/// `select` and `categories` read zero from a mod built before them.
const CHANGE_REQUIRED: usize = core::mem::offset_of!(api_frame::DataChange, item);

/// [`Status::MISSING`] when the build lacks what a change of `field` writes.
fn reachable(loader: &crate::engine::loader::Loader, field: Field) -> Result<(), Status> {
    if crate::content::data_changes::reachable(loader, field) {
        Ok(())
    } else {
        Err(Status::MISSING)
    }
}

entries! {
    fn recipe_rule(
        me as Mod,
        kind: enshwick_core::abi::RecipeRuleKind,
        name as Text,
        disable: u32,
    ) -> Status {
        checked(crate::content::recipes::rule(me.index, kind, name, disable != 0))
    }

    fn recipe_disabled(_loader as Caller, recipe: enshwick_core::RecipeId, out as Out<u32>) -> Status {
        out.put(u32::from(crate::content::recipes::is_disabled(recipe.get())))
    }

    fn recipe_kind(
        _loader as Caller,
        item: enshwick_core::ItemId,
        out as Out<api_frame::RecipeKind>,
    ) -> Status {
        out.put(
            crate::content::item_kinds::ITEMS
                .kind(item.get())
                .ok_or(Status::MISSING)?,
        )
    }

    fn change_data(me as Mod, raw as HeadedSince<api_frame::DataChange, CHANGE_REQUIRED>) -> Status {
        let source = text(raw.source).unwrap_or_default().to_owned();
        if let Ok(Some(change)) = RecipesChange::from_abi(&raw) {
            reachable(me.loader, change.field)?;
            return checked(crate::content::data_changes::declare_recipes(me.index, change, source));
        }
        if raw.select != api_frame::DataSelect::ENTRY {
            let change = ItemsChange::from_abi(&raw)
                .ok()
                .flatten()
                .ok_or(Status::BAD_ARGS)?;
            reachable(me.loader, change.field)?;
            return checked(crate::content::data_changes::declare_items(me.index, change, source));
        }
        let change = Change::from_abi(&raw).ok().flatten().ok_or(Status::BAD_ARGS)?;
        reachable(me.loader, change.field)?;
        checked(crate::content::data_changes::declare(me.index, change, source))
    }

    fn declare_change(
        me as Mod,
        raw as Headed<api_frame::DataChange>,
        out as Out<api_frame::ChangeId>,
    ) -> Status {
        let mut raw = raw;
        raw.op = api_frame::ChangeOp::TIMES;
        raw.value = 1.0;
        let source = text(raw.source).unwrap_or_default().to_owned();
        let (target, field) = if raw.select == api_frame::DataSelect::ENTRY {
            let change = Change::from_abi(&raw).ok().flatten().ok_or(Status::BAD_ARGS)?;
            (
                crate::content::data_changes::LiveTarget::Entry(change.entry, source),
                change.field,
            )
        } else {
            let change = ItemsChange::from_abi(&raw)
                .ok()
                .flatten()
                .ok_or(Status::BAD_ARGS)?;
            (
                crate::content::data_changes::LiveTarget::Items(change.items, source),
                change.field,
            )
        };
        if field == Field::Ingredient {
            return Err(Status::BAD_ARGS.into());
        }
        if field.seen_by_clients() && !me.record.joins_when_on() {
            return Err(Status::REFUSED.into());
        }
        reachable(me.loader, field)?;
        out.put(crate::content::data_changes::declare_change(me.index, target, field)?)
    }

    fn set_change(
        me as Mod,
        id: api_frame::ChangeId,
        op: api_frame::ChangeOp,
        value: f64,
    ) -> Status {
        let value = match op {
            api_frame::ChangeOp::SET => Some(Value::Set(value)),
            api_frame::ChangeOp::TIMES => Some(Value::Times(value)),
            api_frame::ChangeOp::RESET => None,
            _ => return Err(Status::BAD_ARGS.into()),
        };
        checked(crate::content::data_changes::set_change(me.index, id, value))
    }

    fn add_text(_loader as Caller, id: u32, language: u32, text as Text) -> Status {
        checked(crate::content::served::add_text(id, language, text))
    }

    fn item_data_open(me as Mod, id as Text, out as Out<u32>) -> Status {
        out.put(crate::inventory::item_data::open(me.loader, me.index, id)?)
    }

    fn entity_data_open(
        me as Mod,
        id as Text,
        flags: api_frame::EntityDataFlags,
        out as Out<u32>,
    ) -> Status {
        let sent = flags.0 & api_frame::EntityDataFlags::SENT.0 != 0;
        out.put(crate::inventory::item_data::open_entity(me.loader, me.index, id, sent)?)
    }

    fn item_data_get(
        _loader as Caller,
        kind: u32,
        pide: enshwick_core::Entity,
        item: enshwick_core::ItemId,
        [out, capacity, len] as BytesOut,
    ) -> Status {
        out.put(&crate::inventory::item_data::get(kind, pide.0, item.get())?)
    }

    fn item_data_set(
        _loader as Caller,
        kind: u32,
        pide: enshwick_core::Entity,
        item: enshwick_core::ItemId,
        [data, len] as List<u8>,
    ) -> Status {
        crate::inventory::item_data::set(kind, pide.0, item.get(), data)?;
        Ok(())
    }

    fn game_text(_loader as Caller, id: u32, language: u32, out as Out<Str>) -> Status {
        let text = crate::engine::game_texts::text(id, language).ok_or(Status::MISSING)?;
        out.put(Str::new(text))
    }

    fn icons_ready(_loader as Caller) -> Status {
        if crate::content::textures::ready() {
            Ok(())
        } else {
            Err(Status::MISSING.into())
        }
    }

    fn game_version(_loader as Caller, out as Out<Str>) -> Status {
        // The archive lives for the process.
        let version = crate::engine::archive::game()
            .and_then(crate::engine::archive::Archive::version)
            .ok_or(Status::MISSING)?;
        out.put(Str::new(version))
    }

    fn rate(me as Mod, rate: RateId) -> Status {
        let links = crate::content::rates::links(rate);
        let available = links.iter().all(|&symbol| me.loader.address(symbol).is_some())
            // The loot rate reads stack sizes from the item registry, which
            // the inventory link finds.
            && (rate != RateId::LOOT || crate::inventory::transactions::open(me.loader).is_ok());
        checked(crate::content::rates::RATES.declare(
            me.index,
            rate,
            available,
            me.record.joins_when_on(),
        ))
    }

    fn set_rate(me as Mod, rate: RateId, factor: f32) -> Status {
        let recipes = crate::content::rates::is_recipe_output(rate);
        let before = recipes.then(|| crate::content::rate_links::factor(rate).to_bits());
        checked(crate::content::rates::RATES.set(me.index, rate, factor))?;
        if before.is_some_and(|before| before != crate::content::rate_links::factor(rate).to_bits()) {
            crate::content::rate_links::rebuild_recipes();
        }
        Ok(())
    }

    fn read_only(_me: ModHandle, address: usize, max: usize, out as Out<usize>) -> Status {
        out.put(crate::engine::pages::read_only(address, max))
    }

    fn declare_content(me as Mod, id as Text, out as Out<[u8; 16]>) -> Status {
        before_delivery(me.loader)?;
        out.put(crate::base::ids::declare_content(&me.record.id, id)?.0)
    }

    fn content_name(_me as Mod, guid as In<[u8; 16]>, out as Out<Str>) -> Status {
        out.put(
            crate::base::ids::content_named(enshwick_core::content::Guid(guid))
                .ok_or(Status::MISSING)?,
        )
    }

    fn crafting_prop(
        me as Mod,
        prop as In<[u8; 16]>,
        first: u8,
        width: u8,
        out as Out<u8>,
    ) -> Status {
        before_delivery(me.loader)?;
        let prop = enshwick_core::content::Guid(prop);
        // The server sends clients its bits in base and crafting components,
        // so a prop clients lack would shift every bit after it.
        if !me.record.joins_when_on() {
            return Err(RuleOff(format!(
                "crafting prop {prop}: a crafting prop needs a mod of side both, as clients take its bit too"
            ))
            .into());
        }
        out.put(crate::base::ids::with(|ids| ids.crafting_prop(prop, first, width))?)
    }

    fn use_content(me as Mod, id as Text) -> Status {
        if enshwick_core::content::Guid::try_named(id).is_none() {
            return Err(Status::BAD_ARGS.into());
        }
        exported(me.loader, me.record, crate::base::manifest::Kind::Content, id)?;
        Ok(())
    }
}
