//! The fake's recipes, data changes, game rates, texts, item data, content
//! renames and entity templates; served resources in its `served` module.

mod served;

use crate::test::table::{found, put, put_list, text, with, with_open};
use enshwick_base::__internal::renames::release;
use enshwick_core::__fake::UnnamedField;
use enshwick_core::abi::{DataChange, ModHandle, RateId, ResourceKey, Status, Str};
pub(crate) use served::*;

/// What the installed fake's content answers and has recorded.
#[derive(Default)]
pub(crate) struct ContentState {
    /// Each patch of a served resource, in build order, with the key whose
    /// data it takes when it is a re-key.
    pub(crate) served: Vec<(
        crate::test::content::ServedResource,
        crate::test::table::Registered<enshwick_core::abi::ResourcePatchFn>,
        Option<ResourceKey>,
    )>,
    /// Every rebuild of a served resource the mod asked for, at init and
    /// after.
    pub(crate) rebuilds: Vec<ResourceKey>,
    /// How many buffers the host lent patch outputs and has not freed yet.
    pub(crate) lent_buffers: usize,
    /// The changes of vanilla values the mod declared, with their sources.
    pub(crate) changes: DataChanges,
    /// The mod's recipe rules in force: kind and name.
    pub(crate) recipe_rules: Vec<(enshwick_core::abi::RecipeRuleKind, String)>,
    pub(crate) texts: Vec<(u32, u32, String)>,
    /// The content renames each mod or pack declared, old GUID to new.
    pub(crate) content_renames: Vec<(
        ModHandle,
        Vec<(enshwick_core::content::Guid, enshwick_core::content::Guid)>,
    )>,
    /// Content ids the mod declared, in order.
    pub(crate) content_ids: Vec<String>,
    /// The GUID of each content id declared, with the mod or data pack that
    /// declared it: only it may add content under that GUID.
    pub(crate) declared_by: Vec<(ModHandle, enshwick_core::content::Guid)>,
    /// The crafting props the mod was given an index for, in order.
    pub(crate) crafting_props: Vec<enshwick_core::content::Guid>,
    /// Item data kinds by full id, and values by kind and entity: the item
    /// and the bytes.
    pub(crate) item_kinds: Vec<String>,
    /// The content the mod served into the fake archive, by hash.
    pub(crate) served_archive: std::collections::BTreeMap<[u32; 4], Vec<u8>>,
    pub(crate) item_values: std::collections::BTreeMap<(u32, u32), (u32, Vec<u8>)>,
    /// The rates the mod declared, with the factor it set for each.
    pub(crate) rates: Vec<(RateId, f32)>,
    /// The patches in `served` whose builder panicked, by place: each build
    /// but a rebuild runs without them, as the loader's.
    pub(crate) served_off: Vec<usize>,
    /// What the game's builds of served keys left for a rebuild.
    pub(crate) served_builds: ServedBuilds,
}

/// The changes of vanilla values a mod declared, each with its source.
#[derive(Debug, Default)]
pub(crate) struct DataChanges {
    /// Changes of one entry.
    pub(crate) one: Vec<(enshwick_core::content::changes::Change, String)>,
    /// Changes of every item a selection picks.
    #[cfg(feature = "unstable")]
    pub(crate) items: Vec<(enshwick_core::content::changes::ItemsChange, String)>,
    /// Changes of every recipe a selection picks.
    pub(crate) recipes: Vec<(enshwick_core::content::changes::RecipesChange, String)>,
    /// Changes the mod sets later, numbered from 1 in this order.
    pub(crate) live: Vec<LiveSlot>,
}

/// A change a mod sets later, as the fake keeps it.
#[derive(Clone, Debug)]
pub(crate) enum LiveSlot {
    /// Of a field this build names, with the value in force.
    Named(crate::test::LiveChange),
    /// Of a field a mod built without `unstable` cannot name, from its raw
    /// record, which the loader takes as well.
    Unnamed(UnnamedField),
}

/// The status the loader answers a raw change `raw` the typed decoding did
/// not take: OK for a valid change of a field this build does not name.
fn unnamed(raw: &DataChange) -> Status {
    match UnnamedField::of(raw) {
        Some(field) if field.takes(raw) => Status::OK,
        _ => Status::BAD_ARGS,
    }
}

/// The bytes from `address` to the end of the test's component that holds
/// it, at most `max`, as the loader counts the read-only set-up data a
/// component lies in; 0 outside every component.
pub(crate) extern "system" fn read_only(
    _me: ModHandle,
    address: u64,
    max: u32,
    out: *mut u32,
) -> Status {
    let (Ok(address), max) = (usize::try_from(address), max as usize) else {
        return put(out, 0);
    };
    let len = with(0, |s| {
        s.host
            .base
            .components
            .values()
            .find(|&&(start, len)| (start..start + len).contains(&address))
            .map_or(0, |&(start, len)| (start + len - address).min(max))
    });
    put(out, u32::try_from(len).unwrap_or(u32::MAX))
}

pub(crate) extern "system" fn declare_content(
    me: ModHandle,
    id: Str,
    out: *mut enshwick_core::content::Guid,
) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    let id = text(id);
    let Some(guid) = enshwick_core::content::Guid::try_named(&id) else {
        return Status::BAD_ARGS;
    };
    if outside(me, &id) {
        let reason = format!("content id {id} lies outside the mod's namespace");
        crate::test::table::base::mods::loader_turns_off(me, reason);
        return Status::REFUSED;
    }
    with_open(|s| {
        let clash = s
            .content
            .content_ids
            .iter()
            .chain(&s.host.base.declared_content)
            .any(|known| {
                *known != id
                    && enshwick_core::content::Guid::try_named(known)
                        .is_some_and(|g| g.fnv1a() == guid.fnv1a())
            });
        if clash {
            return Status::REFUSED;
        }
        if !s.content.content_ids.contains(&id) {
            s.content.content_ids.push(id);
        }
        if !s.content.declared_by.contains(&(me, guid)) {
            s.content.declared_by.push((me, guid));
        }
        claim(s, me, guid);
        put(out, guid)
    })
}

/// As the loader: `guid`, which mod `me` has just declared, leaves every
/// other mod's renames as [`release`] takes it out, and those mods' patches
/// of a key under it made from another GUID's base.
fn claim(s: &mut crate::test::table::State, me: ModHandle, guid: enshwick_core::content::Guid) {
    let mut losers = Vec::new();
    for (owner, renames) in &mut s.content.content_renames {
        if *owner != me && release(renames, guid) {
            losers.push(*owner);
        }
    }
    let made: Vec<ResourceKey> = s
        .content
        .served
        .iter()
        .filter(|(served, ..)| served.key.guid == guid && served.base.guid != guid)
        .map(|(served, ..)| served.key)
        .collect();
    s.content.served.retain(|(served, patch, _)| {
        !made.contains(&served.key) || !patch.owner().is_some_and(|owner| losers.contains(&owner))
    });
}

/// Whether content id `id` lies outside the namespace of mod `me`, which the
/// loader refuses and turns the mod off for. The fake knows no pack's id, and
/// its tests' `id!` names the test crate, not the mod under test, so it
/// refuses only a pack declaring in the namespace of the mod under test.
fn outside(me: ModHandle, id: &str) -> bool {
    with(false, |s| {
        me != s.host.base.handle
            && s.base.manifest.as_ref().is_some_and(|manifest| {
                id.split_once(':')
                    .is_some_and(|(namespace, _)| namespace == manifest.id)
            })
    })
}

/// Whether mod `me` declared the content id `guid` comes from, as the
/// loader asks of every call that adds content by GUID.
pub(crate) fn declared(
    s: &crate::test::table::State,
    me: ModHandle,
    guid: enshwick_core::content::Guid,
) -> bool {
    s.content.declared_by.contains(&(me, guid))
}

/// Whether a mod other than `me` declared the content id `guid` comes
/// from, as the loader refuses for the old side of a rename.
fn another_mods(
    s: &crate::test::table::State,
    me: ModHandle,
    guid: enshwick_core::content::Guid,
) -> bool {
    s.content
        .declared_by
        .iter()
        .any(|&(by, held)| held == guid && by != me)
}

/// As [`declared`] for a GUID that may name the game's content too, as the
/// new side of a rename may: a GUID of the fake archive passes.
pub(crate) fn declared_or_game(
    s: &crate::test::table::State,
    me: ModHandle,
    guid: enshwick_core::content::Guid,
) -> bool {
    s.host
        .content
        .resources
        .keys()
        .any(|&(held, _, _)| held == guid)
        || declared(s, me, guid)
}

/// As [`declared`] for the resource `key` mod `me` serves from `base`, as
/// the loader asks: a key served from its own GUID, one the fake archive
/// holds and an old GUID of the mod's own renames pass.
pub(crate) fn declared_resource(
    s: &crate::test::table::State,
    me: ModHandle,
    key: &ResourceKey,
    base: &ResourceKey,
) -> bool {
    let renamed_from =
        s.content.content_renames.iter().any(|(owner, renames)| {
            *owner == me && renames.iter().any(|&(from, _)| from == key.guid)
        });
    key.guid == base.guid
        || s.host
            .content
            .resources
            .contains_key(&(key.guid, key.type_hash, key.part))
        || renamed_from
        || declared(s, me, key.guid)
}

pub(crate) extern "system" fn content_name(
    _me: ModHandle,
    guid: *const enshwick_core::content::Guid,
    out: *mut Str,
) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the mod passes null or 16 readable bytes for the call.
    let Some(&guid) = (unsafe { guid.as_ref() }) else {
        return Status::BAD_ARGS;
    };

    with(Status::MISSING, |s| {
        // The id's heap text stays where it is while the state keeps it.
        let named = s
            .content
            .content_ids
            .iter()
            .chain(&s.host.base.declared_content)
            .find(|id| enshwick_core::content::Guid::try_named(id) == Some(guid))
            .map(|id| Str::new(id));
        found(named, out)
    })
}

pub(crate) extern "system" fn crafting_prop(
    me: ModHandle,
    prop: *const enshwick_core::content::Guid,
    out: *mut u8,
) -> Status {
    // SAFETY: the safe layer passes the prop's GUID, alive for the call.
    let Some(&prop) = (unsafe { prop.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    with_open(|s| {
        if !super::base::mods::joins(s, me) || !declared(s, me, prop) {
            return Status::REFUSED;
        }
        // As the loader: without the game's mask and props nothing reads.
        let Some((first, width)) = s.host.content.crafting_mask else {
            return Status::REFUSED;
        };
        let added = &mut s.content.crafting_props;
        let at = added
            .iter()
            .position(|known| *known == prop)
            .unwrap_or(added.len());
        let Some(index) = u8::try_from(at)
            .ok()
            .and_then(|at| first.checked_add(at))
            .filter(|index| *index < width)
        else {
            return Status::REFUSED;
        };
        if at == added.len() {
            added.push(prop);
        }
        put(out, index)
    })
}

pub(crate) extern "system" fn use_content(_me: ModHandle, id: Str) -> Status {
    let id = text(id);
    if enshwick_core::content::Guid::try_named(&id).is_none() {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        let refused = s.base.manifest.as_ref().is_some_and(|manifest| {
            manifest.unexported(
                enshwick_core::schema_section::Kind::Content,
                &id,
                &s.host.base.exports,
            )
        });
        if !refused {
            return Status::OK;
        }
        crate::test::table::base::mods::warn_unexported(
            s,
            enshwick_core::schema_section::Kind::Content,
            &id,
        );
        Status::REFUSED
    })
}

pub(crate) extern "system" fn entity_template(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    out: *mut enshwick_core::abi::Guid,
) -> Status {
    found(
        with(None, |s| s.host.content.templates.get(&entity.0).copied()),
        out,
    )
}

pub(crate) extern "system" fn rename_content(
    me: ModHandle,
    renames: *const enshwick_core::abi::ContentRename,
    count: u32,
) -> Status {
    let renames: Vec<(enshwick_core::content::Guid, enshwick_core::content::Guid)> = match count {
        0 => Vec::new(),
        _ if renames.is_null() => return Status::BAD_ARGS,
        // SAFETY: `renames` is non-null, checked just before, and the ABI has
        // the mod keep `count` aligned entries there for the call.
        _ => unsafe { core::slice::from_raw_parts(renames, count as usize) }
            .iter()
            .map(|r| (r.from, r.to))
            .collect(),
    };
    with(Status::MISSING, |s| {
        // In the loader's order: the seal, the declared ids, the renames.
        if s.base.closed() {
            return Status::TOO_LATE;
        }
        // As the loader: only the end of each chain is new, and a cycle has none.
        let others = renames.iter().any(|&(from, _)| another_mods(s, me, from));
        let undeclared = renames.iter().any(|&(_, to)| {
            !renames.iter().any(|&(from, _)| from == to) && !declared_or_game(s, me, to)
        });
        let onto_itself = renames.iter().any(|(from, to)| from == to);
        let next = |at| {
            renames
                .iter()
                .find(|&&(from, _)| from == at)
                .map(|&(_, to)| to)
        };
        let cycle = renames.iter().any(|&(start, _)| {
            (0..renames.len())
                .try_fold(start, |at, _| next(at))
                .is_some_and(|at| next(at).is_some())
        });
        let twice = s
            .content
            .content_renames
            .iter()
            .any(|(owner, _)| *owner == me);
        if others || undeclared || onto_itself || cycle || twice {
            return Status::REFUSED;
        }
        s.content.content_renames.push((me, renames));
        Status::OK
    })
}

pub(crate) extern "system" fn rate(me: ModHandle, rate: RateId) -> Status {
    with(Status::MISSING, |s| {
        if enshwick_core::__fake::Rate::from_id(rate).is_none() {
            return Status::BAD_ARGS;
        }
        let joins = super::base::mods::joins(s, me);
        if !enshwick_core::__fake::rules::may_declare_rate(rate, joins) {
            return Status::REFUSED;
        }
        if s.host.content.missing_rates.contains(&rate) {
            return Status::UNAVAILABLE;
        }
        if s.base.closed() {
            return Status::TOO_LATE;
        }
        if !s
            .content
            .rates
            .iter()
            .any(|&(declared, _)| declared == rate)
        {
            s.content.rates.push((rate, 1.0));
        }
        Status::OK
    })
}

pub(crate) extern "system" fn set_rate(_me: ModHandle, rate: RateId, factor: f32) -> Status {
    if !enshwick_core::__fake::is_valid_factor(factor) {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        match s
            .content
            .rates
            .iter_mut()
            .find(|(declared, _)| *declared == rate)
        {
            Some((_, set)) => {
                *set = factor;
                Status::OK
            }
            None => Status::BAD_ARGS,
        }
    })
}

pub(crate) extern "system" fn recipe_rule(
    _me: ModHandle,
    kind: enshwick_core::abi::RecipeRuleKind,
    name: Str,
    disable: u8,
) -> Status {
    if disable > 1 {
        return Status::BAD_ARGS;
    }
    let name = text(name);
    with(Status::MISSING, |s| {
        s.content
            .recipe_rules
            .retain(|(k, n)| !(*k == kind && *n == name));
        if disable != 0 {
            s.content.recipe_rules.push((kind, name));
        }
        Status::OK
    })
}

pub(crate) extern "system" fn recipe_disabled(
    _me: ModHandle,
    recipe: enshwick_core::RecipeId,
    out: *mut u8,
) -> Status {
    let disabled = with(false, |s| {
        s.host.content.disabled_recipes.contains(&recipe.get())
    });
    if !out.is_null() {
        // SAFETY: the safe layer passes its out value for the call.
        unsafe { out.write(u8::from(disabled)) };
    }
    Status::OK
}

pub(crate) extern "system" fn recipe_kind(
    _me: ModHandle,
    item: enshwick_core::ItemId,
    out: *mut enshwick_core::abi::RecipeKind,
) -> Status {
    found(
        with(None, |s| {
            s.host.content.recipe_kinds.get(&item.get()).copied()
        }),
        out,
    )
}

/// As the loader's: a fixed change, or a live one when `out` is set.
pub(crate) extern "system" fn change_data(
    me: ModHandle,
    change: *const DataChange,
    out: *mut enshwick_core::abi::ChangeId,
) -> Status {
    if !out.is_null() {
        return live(me, change, out);
    }
    // SAFETY: the safe layer passes null or a record with its header, valid
    // for the call; an all-zero record is valid, and `item` reads as zero
    // for a record of a mod built before it.
    let Some(raw) = (unsafe {
        enshwick_core::abi::read_headed(change, core::mem::offset_of!(DataChange, item))
    }) else {
        return Status::BAD_ARGS;
    };
    // shortcut: the loader reads the change before the phase, so a malformed one after init answers BAD_ARGS there; match it when a test needs that order.
    if crate::test::table::late() {
        return Status::TOO_LATE;
    }
    // SAFETY: the safe layer borrows the source for the call.
    let source = unsafe { raw.source.as_str() }
        .unwrap_or_default()
        .to_owned();
    if raw.select != enshwick_core::abi::DataSelect::ENTRY {
        if let Ok(Some(change)) = enshwick_core::content::changes::RecipesChange::from_abi(&raw) {
            return with(Status::MISSING, |s| {
                s.content.changes.recipes.push((change, source));
                Status::OK
            });
        }
        #[cfg(feature = "unstable")]
        if let Ok(Some(change)) = enshwick_core::content::changes::ItemsChange::from_abi(&raw) {
            return with(Status::MISSING, |s| {
                s.content.changes.items.push((change, source));
                Status::OK
            });
        }
        return unnamed(&raw);
    }
    let Ok(Some(change)) = enshwick_core::content::changes::Change::from_abi(&raw) else {
        return unnamed(&raw);
    };
    with(Status::MISSING, |s| {
        s.content.changes.one.push((change, source));
        Status::OK
    })
}

/// A live change, whose value `set_change` sets later; `out` is non-null.
fn live(
    me: ModHandle,
    change: *const DataChange,
    out: *mut enshwick_core::abi::ChangeId,
) -> Status {
    use enshwick_core::content::changes::{Change, Field, Target};
    // SAFETY: the safe layer passes null or a full record with its header,
    // valid for the call.
    let Some(mut raw) = (unsafe {
        enshwick_core::abi::read_headed(change, <DataChange as enshwick_core::abi::Headed>::BORN)
    }) else {
        return Status::BAD_ARGS;
    };
    // As the loader: a change is declared in init only.
    if crate::test::table::late() {
        return Status::TOO_LATE;
    }
    // As the loader: a live change starts as "times 1", whatever the mod passed.
    raw.op = enshwick_core::abi::ChangeOp::TIMES;
    raw.value = 1.0;
    let target = if raw.select == enshwick_core::abi::DataSelect::ENTRY {
        Change::from_abi(&raw)
            .ok()
            .flatten()
            .map(|c| (Target::Entry(c.entry), c.field))
    } else {
        #[cfg(feature = "unstable")]
        let items = enshwick_core::content::changes::ItemsChange::from_abi(&raw)
            .ok()
            .flatten()
            .map(|c| (Target::Items(c.items), c.field));
        #[cfg(not(feature = "unstable"))]
        let items = None;
        items
    };
    // SAFETY: the safe layer borrows the source for the call.
    let source = unsafe { raw.source.as_str() }
        .unwrap_or_default()
        .to_owned();
    let (mut slot, seen_by_clients) = match target {
        Some((_, Field::Ingredient)) => return Status::BAD_ARGS,
        Some((target, field)) => (
            LiveSlot::Named(crate::test::content::LiveChange {
                id: enshwick_core::abi::ChangeId(0),
                target,
                field,
                source,
                value: None,
            }),
            field.seen_by_clients(),
        ),
        // Both fields a build may not name are items', which players' games read.
        None => match UnnamedField::of(&raw) {
            Some(field) if field.takes(&raw) => (LiveSlot::Unnamed(field), true),
            _ => return Status::BAD_ARGS,
        },
    };
    with(Status::MISSING, |s| {
        if seen_by_clients && !super::base::mods::joins(s, me) {
            return Status::REFUSED;
        }
        let id = enshwick_core::abi::ChangeId(
            u32::try_from(s.content.changes.live.len() + 1).unwrap_or(u32::MAX),
        );
        if let LiveSlot::Named(live) = &mut slot {
            live.id = id;
        }
        s.content.changes.live.push(slot);
        // SAFETY: `out` is non-null by the check above and the safe layer
        // points it at a writable `ChangeId` for the call.
        unsafe { out.write(id) };
        Status::OK
    })
}

pub(crate) extern "system" fn set_change(
    _me: ModHandle,
    id: enshwick_core::abi::ChangeId,
    op: enshwick_core::abi::ChangeOp,
    value: f64,
) -> Status {
    use enshwick_core::content::changes::Value;
    let value = match op {
        enshwick_core::abi::ChangeOp::SET => Some(Value::Set(value)),
        enshwick_core::abi::ChangeOp::TIMES => Some(Value::Times(value)),
        enshwick_core::abi::ChangeOp::RESET => None,
        _ => return Status::BAD_ARGS,
    };
    with(Status::MISSING, |s| {
        let at = usize::try_from(id.0).ok().and_then(|n| n.checked_sub(1));
        let Some(live) = at.and_then(|at| s.content.changes.live.get_mut(at)) else {
            return Status::BAD_ARGS;
        };
        match live {
            LiveSlot::Named(named) => {
                if value.is_some_and(|value| named.field.check(value).is_err()) {
                    return Status::BAD_ARGS;
                }
                named.value = value;
            }
            LiveSlot::Unnamed(field) => {
                if value.is_some_and(|value| !field.takes_value(value)) {
                    return Status::BAD_ARGS;
                }
            }
        }
        Status::OK
    })
}

pub(crate) extern "system" fn add_text(
    _me: ModHandle,
    id: u32,
    language: u32,
    value: Str,
) -> Status {
    let value = text(value);
    with_open(|s| {
        s.content.texts.push((id, language, value));
        Status::OK
    })
}

pub(crate) extern "system" fn define_item_data(_me: ModHandle, id: Str, out: *mut u32) -> Status {
    let id = text(id);
    let full = if id.contains(':') {
        id
    } else {
        format!("{}:{id}", crate::test::MOD_ID)
    };
    if crate::test::table::late() {
        return Status::TOO_LATE;
    }
    let kind = with(None, |s| {
        let index = s
            .content
            .item_kinds
            .iter()
            .position(|k| *k == full)
            .unwrap_or_else(|| {
                s.content.item_kinds.push(full.clone());
                s.content.item_kinds.len() - 1
            });
        u32::try_from(index).ok()
    });
    found(kind, out)
}

pub(crate) extern "system" fn define_entity_data(
    me: ModHandle,
    id: Str,
    _flags: enshwick_core::abi::EntityDataFlags,
    out: *mut u32,
) -> Status {
    define_item_data(me, id, out)
}

pub(crate) extern "system" fn item_data_get(
    _me: ModHandle,
    kind: u32,
    pide: enshwick_core::Entity,
    item: enshwick_core::ItemId,
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    let value = with(None, |s| {
        s.content
            .item_values
            .get(&(kind, pide.0))
            .filter(|(held, _)| *held == item.get())
            .map(|(_, bytes)| bytes.clone())
    });
    let Some(value) = value else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes `len` null or a writable, aligned `u32`,
    // and `out` null or `capacity` writable bytes apart from the fake's own.
    unsafe { enshwick_core::__fake::copy_out(&value, out, capacity, len) }
}

pub(crate) extern "system" fn set_item_data(
    _me: ModHandle,
    kind: u32,
    pide: enshwick_core::Entity,
    item: enshwick_core::ItemId,
    data: *const u8,
    len: u32,
) -> Status {
    if pide.is_none() {
        return Status::BAD_ARGS;
    }
    let value = if len == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `len` bytes at `data`.
        unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        // The world's save keeps no player's character in place.
        if !value.is_empty() && s.host.base.players.iter().any(|p| p.entity == pide) {
            return Status::NOT_KEPT;
        }
        if value.is_empty() {
            s.content.item_values.remove(&(kind, pide.0));
        } else {
            s.content
                .item_values
                .insert((kind, pide.0), (item.get(), value));
        }
        Status::OK
    })
}

pub(crate) extern "system" fn content_serve(
    _me: ModHandle,
    data: *const u8,
    len: u32,
    out: *mut [u32; 4],
) -> Status {
    if data.is_null() || len == 0 {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes `len` bytes at `data`.
    let bytes = unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec();
    let hash = [
        len,
        u32::try_from(bytes.len()).unwrap_or(0) ^ 0x5a5a,
        bytes.iter().map(|&b| u32::from(b)).sum(),
        1,
    ];
    with(Status::MISSING, |s| {
        s.content.served_archive.insert(hash, bytes);
        Status::OK
    });
    found(Some(hash), out)
}

pub(crate) extern "system" fn content_read(
    _me: ModHandle,
    hash: *const [u32; 4],
    out: *mut u8,
    capacity: u32,
    len: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes a hash for the call.
    let Some(hash) = (unsafe { hash.as_ref() }).copied() else {
        return Status::BAD_ARGS;
    };
    let Some(data) = with(None, |s| {
        s.content
            .served_archive
            .get(&hash)
            .or_else(|| s.host.content.archive.get(&hash))
            .cloned()
    }) else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes `len` null or a writable, aligned `u32`,
    // and `out` null or `capacity` writable bytes apart from the fake's own.
    unsafe { enshwick_core::__fake::copy_out(&data, out, capacity, len) }
}

pub(crate) extern "system" fn icons_ready(_me: ModHandle) -> Status {
    if with(false, |s| s.host.content.icons_ready) {
        Status::OK
    } else {
        note_icons_read();
        Status::UNAVAILABLE
    }
}

pub(crate) extern "system" fn game_text(
    _me: ModHandle,
    id: u32,
    language: u32,
    out: *mut Str,
) -> Status {
    let english = enshwick_core::i18n::Language::EN_US.0;
    let found = with(None, |s| {
        let texts = &s.host.content.game_texts;
        let language = if language == 0 { english } else { language };
        texts
            .get(&(id, language))
            .or_else(|| texts.get(&(id, english)))
            .copied()
    });
    match found {
        Some(text) => put(out, Str::new(text)),
        None => Status::MISSING,
    }
}

pub(crate) extern "system" fn resource_keys(
    _me: ModHandle,
    type_hash: u32,
    out: *mut ResourceKey,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let mut keys: Vec<ResourceKey> = with(Vec::new(), |s| {
        s.host
            .content
            .resources
            .keys()
            .filter(|(_, ty, _)| type_hash == 0 || *ty == type_hash)
            .map(|&(guid, type_hash, part)| ResourceKey {
                guid,
                type_hash,
                part,
            })
            .collect()
    });
    keys.sort_by_key(|key| (key.type_hash, key.guid, key.part));
    put_list(&keys, out, capacity, count)
}

pub(crate) extern "system" fn content_hashes(
    _me: ModHandle,
    out: *mut [u32; 4],
    capacity: u32,
    count: *mut u32,
) -> Status {
    let hashes: Vec<[u32; 4]> = with(Vec::new(), |s| {
        s.host.content.archive.keys().copied().collect()
    });
    put_list(&hashes, out, capacity, count)
}

pub(crate) extern "system" fn types(
    _me: ModHandle,
    out: *mut enshwick_core::abi::TypeEntry,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(table) = with(None, |s| s.host.content.types.clone()) else {
        return Status::UNAVAILABLE;
    };
    crate::test::table::base::io::list(table.entries(), out, entry_size, capacity, count)
}

pub(crate) extern "system" fn game_version(_me: ModHandle, out: *mut Str) -> Status {
    match with(None, |s| s.host.content.game_version) {
        Some(version) => put(out, Str::new(version)),
        None => Status::UNAVAILABLE,
    }
}
