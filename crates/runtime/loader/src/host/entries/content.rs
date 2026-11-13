//! Host functions for the content domain: recipes, data changes, texts, item
//! data, content ids, rates and the game's memory pages.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{Level, ModHandle, RateId, Status, Str};

use crate::host::entries::base::services::unexported;
use crate::host::memory::{put, text};
use crate::host::{caller, caller_index, calling, entry};

pub(crate) mod resources;

pub(crate) extern "system" fn recipe_rule(
    me: ModHandle,
    kind: enshwick_core::abi::RecipeRuleKind,
    name: Str,
    disable: u32,
) -> Status {
    entry("recipe_rule", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Some(name) = text(name) else {
            return Status::BAD_ARGS;
        };
        crate::content::recipes::rule(index, kind, name, disable != 0)
    })
}

pub(crate) extern "system" fn recipe_disabled(
    me: ModHandle,
    recipe: enshwick_core::RecipeId,
    out: *mut u32,
) -> Status {
    entry("recipe_disabled", || {
        if let Err(status) = caller(me) {
            return status;
        }
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `out` is non-null, checked just before; the ABI has the mod
        // pass it pointing at a writable, aligned `u32`.
        unsafe {
            out.write(u32::from(crate::content::recipes::is_disabled(
                recipe.get(),
            )));
        };
        Status::OK
    })
}

pub(crate) extern "system" fn recipe_kind(
    me: ModHandle,
    item: enshwick_core::ItemId,
    out: *mut api_frame::RecipeKind,
) -> Status {
    entry("recipe_kind", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::content::item_kinds::ITEMS.kind(item.get()) {
            // SAFETY: `HostApi::recipe_kind` has the mod pass `out` null or
            // pointing at a writable, aligned `RecipeKind` for the call.
            Some(kind) => unsafe { put(out, kind) },
            None => Status::MISSING,
        }
    })
}

pub(crate) extern "system" fn change_data(
    me: ModHandle,
    change: *const api_frame::DataChange,
) -> Status {
    entry("change_data", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: `HostApi::change_data` has the mod pass null or a record
        // with its header, valid for the call; an all-zero record is valid,
        // and `item`, `select` and `categories` read as zero for a record of
        // a mod built before them.
        let Some(raw) = (unsafe {
            api_frame::read_headed(change, core::mem::offset_of!(api_frame::DataChange, item))
        }) else {
            return Status::BAD_ARGS;
        };
        let source = text(raw.source).unwrap_or_default().to_owned();
        if let Ok(Some(change)) = enshwick_core::content::changes::RecipesChange::from_abi(&raw) {
            if !crate::content::data_changes::reachable(loader, change.field) {
                return Status::MISSING;
            }
            return crate::content::data_changes::declare_recipes(index, change, source);
        }
        if raw.select != api_frame::DataSelect::ENTRY {
            let Ok(Some(change)) = enshwick_core::content::changes::ItemsChange::from_abi(&raw)
            else {
                return Status::BAD_ARGS;
            };
            if !crate::content::data_changes::reachable(loader, change.field) {
                return Status::MISSING;
            }
            return crate::content::data_changes::declare_items(index, change, source);
        }
        let Ok(Some(change)) = enshwick_core::content::changes::Change::from_abi(&raw) else {
            return Status::BAD_ARGS;
        };
        if !crate::content::data_changes::reachable(loader, change.field) {
            return Status::MISSING;
        }
        crate::content::data_changes::declare(index, change, source)
    })
}

pub(crate) extern "system" fn declare_change(
    me: ModHandle,
    change: *const api_frame::DataChange,
    out: *mut api_frame::ChangeId,
) -> Status {
    entry("declare_change", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `HostApi::declare_change` has the mod pass null or a
        // record with its header, valid for the call; a mod that knows this
        // entry knows every field of the record.
        let Some(mut raw) = (unsafe {
            api_frame::read_headed(
                change,
                <api_frame::DataChange as enshwick_core::abi::Headed>::BORN,
            )
        }) else {
            return Status::BAD_ARGS;
        };
        raw.op = api_frame::ChangeOp::TIMES;
        raw.value = 1.0;
        let source = text(raw.source).unwrap_or_default().to_owned();
        let (target, field) = if raw.select == api_frame::DataSelect::ENTRY {
            match enshwick_core::content::changes::Change::from_abi(&raw) {
                Ok(Some(change)) => (
                    crate::content::data_changes::LiveTarget::Entry(change.entry, source),
                    change.field,
                ),
                _ => return Status::BAD_ARGS,
            }
        } else {
            match enshwick_core::content::changes::ItemsChange::from_abi(&raw) {
                Ok(Some(change)) => (
                    crate::content::data_changes::LiveTarget::Items(change.items, source),
                    change.field,
                ),
                _ => return Status::BAD_ARGS,
            }
        };
        if field == enshwick_core::content::changes::Field::Ingredient {
            return Status::BAD_ARGS;
        }
        if field.seen_by_clients() && !record.joins_when_on() {
            return Status::REFUSED;
        }
        if !crate::content::data_changes::reachable(loader, field) {
            return Status::MISSING;
        }
        match crate::content::data_changes::declare_change(index, target, field) {
            // SAFETY: `out` is non-null by the check above and, by
            // `HostApi::declare_change`, points at a writable `ChangeId` for
            // the call.
            Ok(id) => unsafe { put(out, id) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn set_change(
    me: ModHandle,
    id: api_frame::ChangeId,
    op: api_frame::ChangeOp,
    value: f64,
) -> Status {
    entry("set_change", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let value = match op {
            api_frame::ChangeOp::SET => Some(enshwick_core::content::changes::Value::Set(value)),
            api_frame::ChangeOp::TIMES => {
                Some(enshwick_core::content::changes::Value::Times(value))
            }
            api_frame::ChangeOp::RESET => None,
            _ => return Status::BAD_ARGS,
        };
        crate::content::data_changes::set_change(index, id, value)
    })
}

pub(crate) extern "system" fn add_text(me: ModHandle, id: u32, language: u32, text: Str) -> Status {
    entry("add_text", || {
        let Some(text) = self::text(text) else {
            return Status::BAD_ARGS;
        };
        match caller(me) {
            Ok(_) => crate::content::served::add_text(id, language, text),
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn item_data_open(me: ModHandle, id: Str, out: *mut u32) -> Status {
    entry("item_data_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        match caller_index(me)
            .and_then(|(loader, index)| crate::inventory::item_data::open(loader, index, id))
        {
            // SAFETY: `HostApi::item_data_open` has the mod pass `out` null or
            // pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(kind) => unsafe { put(out, kind) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn entity_data_open(
    me: ModHandle,
    id: Str,
    flags: api_frame::EntityDataFlags,
    out: *mut u32,
) -> Status {
    entry("entity_data_open", || {
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        let sent = flags.0 & api_frame::EntityDataFlags::SENT.0 != 0;
        match caller_index(me).and_then(|(loader, index)| {
            crate::inventory::item_data::open_entity(loader, index, id, sent)
        }) {
            // SAFETY: `HostApi::entity_data_open` has the mod pass `out` null
            // or pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(kind) => unsafe { put(out, kind) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn item_data_get(
    me: ModHandle,
    kind: u32,
    pide: enshwick_core::Entity,
    item: enshwick_core::ItemId,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    entry("item_data_get", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let value = match crate::inventory::item_data::get(kind, pide.0, item.get()) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: `HostApi::item_data_get` has the mod pass `len` null or a
        // writable, aligned `u32`, and `out` null or `capacity` writable
        // bytes apart from the loader's own buffer.
        unsafe { enshwick_core::__private::copy_out(&value, out, capacity, len) }
    })
}

pub(crate) extern "system" fn item_data_set(
    me: ModHandle,
    kind: u32,
    pide: enshwick_core::Entity,
    item: enshwick_core::ItemId,
    data: *const u8,
    len: u32,
) -> Status {
    entry("item_data_set", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Ok(len) = usize::try_from(len) else {
            return Status::BAD_ARGS;
        };
        let value = if len == 0 {
            &[][..]
        } else if data.is_null() {
            return Status::BAD_ARGS;
        } else {
            // SAFETY: the caller passes `len` bytes at `data` for the call.
            unsafe { core::slice::from_raw_parts(data, len) }
        };
        match crate::inventory::item_data::set(kind, pide.0, item.get(), value) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn game_text(
    me: ModHandle,
    id: u32,
    language: u32,
    out: *mut Str,
) -> Status {
    entry("game_text", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::engine::game_texts::text(id, language) {
            // SAFETY: `HostApi::game_text` has the mod pass `out` null or
            // pointing at a writable, aligned `Str` that no other thread
            // touches during the call.
            Some(text) => unsafe { put(out, Str::new(text)) },
            None => Status::MISSING,
        }
    })
}

pub(crate) extern "system" fn icons_ready(me: ModHandle) -> Status {
    entry("icons_ready", || match caller(me) {
        Ok(_) if crate::content::textures::ready() => Status::OK,
        Ok(_) => Status::MISSING,
        Err(status) => status,
    })
}

pub(crate) extern "system" fn game_version(me: ModHandle, out: *mut Str) -> Status {
    entry("game_version", || {
        if let Err(status) = caller(me) {
            return status;
        }
        match crate::engine::archive::game().and_then(crate::engine::archive::Archive::version) {
            // SAFETY: `HostApi::game_version` has the mod pass `out` null or
            // pointing at a writable, aligned `Str` that no other thread
            // touches during the call; the archive lives for the process.
            Some(version) => unsafe { put(out, Str::new(version)) },
            None => Status::MISSING,
        }
    })
}

pub(crate) extern "system" fn rate(me: ModHandle, rate: RateId) -> Status {
    entry("rate", || match calling(me) {
        Ok((loader, index, record)) => {
            let links = crate::content::rates::links(rate);
            let available = links.iter().all(|&symbol| loader.address(symbol).is_some())
                // The loot rate reads stack sizes from the item registry,
                // which the inventory link finds.
                && (rate != RateId::LOOT || crate::inventory::transactions::open(loader).is_ok());
            crate::content::rates::RATES.declare(index, rate, available, record.joins_when_on())
        }
        Err(status) => status,
    })
}

pub(crate) extern "system" fn set_rate(me: ModHandle, rate: RateId, factor: f32) -> Status {
    entry("set_rate", || match caller_index(me) {
        Ok((_, index)) => {
            let recipes = crate::content::rates::is_recipe_output(rate);
            let before = recipes.then(|| crate::content::rate_links::factor(rate).to_bits());
            let status = crate::content::rates::RATES.set(index, rate, factor);
            if status == Status::OK
                && before.is_some_and(|before| {
                    before != crate::content::rate_links::factor(rate).to_bits()
                })
            {
                crate::content::rate_links::rebuild_recipes();
            }
            status
        }
        Err(status) => status,
    })
}

pub(crate) extern "system" fn read_only(
    _me: ModHandle,
    address: usize,
    max: usize,
    out: *mut usize,
) -> Status {
    entry("read_only", || {
        // SAFETY: `HostApi::read_only` has the mod pass `out` null or pointing
        // at a writable, aligned `usize` that no other thread touches during
        // the call.
        unsafe { put(out, crate::engine::pages::read_only(address, max)) }
    })
}

pub(crate) extern "system" fn declare_content(
    me: ModHandle,
    id: Str,
    out: *mut [u8; 16],
) -> Status {
    entry("declare_content", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.delivery.get().is_some() {
            return Status::TOO_LATE;
        }
        let Some(id) = text(id) else {
            return Status::BAD_ARGS;
        };
        match crate::base::ids::declare_content(&record.id, id) {
            // SAFETY: `HostApi::declare_content` has the mod pass `out` null or
            // pointing at a writable, aligned `[u8; 16]` that no other thread
            // touches during the call.
            Ok(guid) => unsafe { put(out, guid.0) },
            Err(why) => {
                loader.turn_off(index, &why.to_string());
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn content_name(
    me: ModHandle,
    guid: *const [u8; 16],
    out: *mut Str,
) -> Status {
    entry("content_name", || {
        if let Err(status) = calling(me) {
            return status;
        }
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        // SAFETY: `HostApi::content_name` has the mod pass `guid` null or
        // pointing at 16 readable bytes for the call; null is checked here.
        let Some(&guid) = (unsafe { guid.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        match crate::base::ids::content_named(enshwick_core::content::Guid(guid)) {
            // SAFETY: `out` is non-null (checked above) and
            // `HostApi::content_name` has the mod pass a writable, aligned
            // `Str` that no other thread touches during the call.
            Some(name) => unsafe { put(out, name) },
            None => Status::MISSING,
        }
    })
}

pub(crate) extern "system" fn crafting_prop(
    me: ModHandle,
    prop: *const [u8; 16],
    first: u8,
    width: u8,
    out: *mut u8,
) -> Status {
    entry("crafting_prop", || {
        let (loader, index, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if loader.delivery.get().is_some() {
            return Status::TOO_LATE;
        }
        // SAFETY: `HostApi::crafting_prop` has the mod pass `prop` null or
        // pointing at a readable `[u8; 16]` for the call; `as_ref` turns
        // away null.
        let Some(&prop) = (unsafe { prop.as_ref() }) else {
            return Status::BAD_ARGS;
        };
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        let prop = enshwick_core::content::Guid(prop);
        // The server sends clients its bits in base and crafting components,
        // so a prop clients lack would shift every bit after it.
        if !record.joins_when_on() {
            loader.turn_off(
                index,
                &format!("crafting prop {prop}: a crafting prop needs a mod of side both, as clients take its bit too"),
            );
            return Status::REFUSED;
        }
        match crate::base::ids::with(|ids| ids.crafting_prop(prop, first, width)) {
            // SAFETY: `HostApi::crafting_prop` has the mod pass `out` null or
            // pointing at a writable `u8` that no other thread touches
            // during the call.
            Ok(at) => unsafe { put(out, at) },
            Err(why @ crate::base::ids::CraftingPropError::Bounds { .. }) => {
                crate::log!(WARN, "{why}");
                Status::BAD_ARGS
            }
            Err(why) => {
                loader.turn_off(index, &why.to_string());
                Status::REFUSED
            }
        }
    })
}

pub(crate) extern "system" fn use_content(me: ModHandle, id: Str) -> Status {
    entry("use_content", || {
        let (loader, _, record) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Some(id) = text(id).filter(|id| enshwick_core::content::Guid::try_named(id).is_some())
        else {
            return Status::BAD_ARGS;
        };
        match unexported(loader, record, crate::base::manifest::Kind::Content, id) {
            Some(reason) => {
                crate::engine::log::write(Level::WARN, &record.id, &reason);
                Status::REFUSED
            }
            None => Status::OK,
        }
    })
}
