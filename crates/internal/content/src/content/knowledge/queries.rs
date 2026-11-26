//! Queries of a mod's own in the game's query database, and the game's
//! named queries by name; all of a mod's served by one patch of the
//! database.
//!
//! The database lists world queries first and player queries from its
//! `playerQueryStartIndex`; each query's `queryIndex` is its position, and
//! a part that names another query keeps that query's index in its
//! `queryIndex` (the world twin of a query both sides hold) and its
//! `playerProgressionQueryIndex` (the player twin), which the game's query
//! pass reads directly (server 26430253). So a mod's world query goes at
//! the end of the world range, a player query at the end, and every index
//! past the world range moves with it. The database's `npcQueryCount` and
//! `runtimePlayerQueryStartIndex` count and split its `runtimeQueries`, a
//! list of their own that a mod's query leaves alone (39 `Npc` runtime
//! queries and 72 before the player ones, server 387 data).

use std::sync::{Arc, Mutex};

use crate::__load::*;
use crate::abi::{Level, ResourceKey};
use crate::knowledge::Scope;
use crate::locks::Relock as _;
use crate::resources::{Registry, Value};
use crate::typed::{GameKnowledgeQueryResourceDb, Named as _};
use crate::{Host, Off};

use super::super::{EngineFacts as _, Id};
use super::{GameQuery, KnowledgeQuery, KnowledgeTest, hash_key, key_of, key_of_hash, serve_list};

const QUERIES: &str = "queries";
const PLAYER_START: &str = "playerQueryStartIndex";
const QUERY_ID: &str = "queryId";
const QUERY_INDEX: &str = "queryIndex";
const PLAYER_INDEX: &str = "playerProgressionQueryIndex";
const IS_PLAYER: &str = "isPlayerQuery";
const ACTIONS: &str = "actions";
const NAME: &str = "name";
const TYPE: &str = "type";
const QUERY: &str = "query";
const QUERY_DATA_ID: &str = "knowledgeOrQueryId";
/// The action type of a part that names another query.
const OF_QUERY: &str = "Query";
/// The action type of a part that tests a knowledge entry.
const OF_KNOWLEDGE: &str = "Knowledge";

/// How a query of a mod's own joins its parts.
///
/// The names are enshwick's; the game's are `AND`, `OR` and `XOR`.
///
/// # Examples
///
/// See [`NewQuery`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum QueryForm {
    /// Every part holds.
    All,
    /// At least one part holds.
    Any,
    /// Exactly one of its two parts holds.
    OneOf,
}

impl QueryForm {
    /// The game's name of it.
    const fn game_name(self) -> &'static str {
        match self {
            Self::All => "AND",
            Self::Any => "OR",
            Self::OneOf => "XOR",
        }
    }
}

/// A new query for [`Load::add_query`](../../struct.Load.html#method.add_query):
/// parts joined by a form, which the game evaluates as it does its own
/// named queries.
///
/// The name is enshwick's; the game's is `GameKnowledgeQueryResource`.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::content::knowledge::{KnowledgeQuery, NewQuery, QueryForm};
/// use enshwick::knowledge::{KnowledgeId, Scope};
///
/// fn init(load: &mut enshwick::Load<'_>, a: KnowledgeId, b: KnowledgeId) -> Result<(), enshwick::Off> {
///     let both = load.add_query(NewQuery::new(
///         id!("both-found"),
///         Scope::Player,
///         QueryForm::All,
///         vec![KnowledgeQuery::is_set(a, Scope::Player), KnowledgeQuery::is_set(b, Scope::Player)],
///     ))?;
///     let _done_when = KnowledgeQuery::game(both);
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct NewQuery {
    /// Its content id; its query id is the FNV-1a of the id's GUID, and the
    /// id is its name in the database.
    pub id: Id,
    /// Whose it is: a player's is asked for each player, a world's for the
    /// world.
    pub scope: Scope,
    /// How it joins its parts.
    pub form: QueryForm,
    /// Whether it holds where its form does not: the one way to the
    /// opposite of a game query, which the game reads with no compare.
    #[script(default)]
    pub invert: bool,
    /// Its parts: knowledge entries, or game queries of the database (the
    /// game's, or ones a mod added before it).
    pub parts: Vec<KnowledgeQuery>,
}

impl NewQuery {
    /// A query `id` of `scope` that joins `parts` by `form`, not inverted.
    ///
    /// # Examples
    ///
    /// See [`NewQuery`].
    #[must_use]
    pub const fn new(id: Id, scope: Scope, form: QueryForm, parts: Vec<KnowledgeQuery>) -> Self {
        Self {
            id,
            scope,
            form,
            invert: false,
            parts,
        }
    }

    /// Its query id.
    fn key(&self) -> u32 {
        self.id.guid().fnv1a()
    }

    /// Whether it is a player's.
    fn player(&self) -> bool {
        self.scope == Scope::Player
    }

    /// Refuses a query the game could not evaluate as written.
    fn check(&self) -> Result<(), Off> {
        let refuse = |why: &str| Err(Off::new(format!("query {}: {why}", self.id)));
        match (self.form, self.parts.len()) {
            (_, 0) => return refuse("no parts"),
            // shortcut: what the game's XOR makes of three parts is not read; allow more once it is.
            (QueryForm::OneOf, n) if n != 2 => return refuse("one of two parts, not of more"),
            _ => {}
        }
        for part in &self.parts {
            part.to_value()?;
        }
        Ok(())
    }

    /// The game's query of it, from the vanilla query `template`; the
    /// indices of its parts that name a query are left for [`resolve`].
    fn to_value(&self, template: &Value) -> Result<Value, Off> {
        let action = match template.field(ACTIONS) {
            Some(Value::List(actions)) => actions.first(),
            _ => None,
        }
        .ok_or_else(|| Off::new("query database: a query without parts to copy"))?;
        let actions = self
            .parts
            .iter()
            .map(|part| {
                let kind = if part.test == KnowledgeTest::Holds {
                    OF_QUERY
                } else {
                    OF_KNOWLEDGE
                };
                let mut value = action.clone();
                for (field, new) in [
                    (TYPE, Value::Name(kind.into())),
                    (NAME, Value::Text(String::new())),
                    (QUERY, part.to_value()?),
                    (QUERY_INDEX, Value::Uint(0)),
                    (PLAYER_INDEX, Value::Uint(0)),
                    ("invertResult", Value::Bool(false)),
                    ("isPlayerAction", Value::Bool(false)),
                ] {
                    set(&mut value, field, new)?;
                }
                Ok(value)
            })
            .collect::<Result<_, Off>>()?;
        let mut query = template.clone();
        for (field, new) in [
            (NAME, Value::Text(self.id.to_string())),
            (
                QUERY_ID,
                Value::Struct(vec![("id".into(), hash_key(self.key()))]),
            ),
            (TYPE, Value::Name(self.form.game_name().into())),
            (IS_PLAYER, Value::Bool(self.player())),
            ("invertResult", Value::Bool(self.invert)),
            ("hasOptionalPlayerProgression", Value::Bool(false)),
            (ACTIONS, Value::List(actions)),
        ] {
            set(&mut query, field, new)?;
        }
        Ok(query)
    }
}

/// Sets field `name` of `value`; a field the build lacks is an error.
fn set(value: &mut Value, name: &str, new: Value) -> Result<(), Off> {
    *value
        .field_mut(name)
        .ok_or_else(|| Off::new(format!("query database: no field {name}")))? = new;
    Ok(())
}

/// The query id of the database's query `query`.
fn query_id(query: &Value) -> Option<u32> {
    query.field(QUERY_ID).and_then(key_of)
}

/// Whether the database's query `query` is a player's.
fn is_player(query: &Value) -> bool {
    matches!(query.field(IS_PLAYER), Some(Value::Bool(true)))
}

/// Whether the action `action` names another query.
fn names_query(action: &Value) -> bool {
    matches!(action.field(TYPE), Some(Value::Name(kind)) if kind == OF_QUERY)
}

/// The actions of the database's query `query`, to change.
fn actions_mut(query: &mut Value) -> &mut [Value] {
    match query.field_mut(ACTIONS) {
        Some(Value::List(actions)) => actions,
        _ => &mut [],
    }
}

/// The indices the game keeps for the query `id` among `queries`: its world
/// twin's or only position, and its player twin's or only position.
fn indices_of(queries: &[Value], id: u32) -> Option<(u64, u64)> {
    let at = |player: bool| {
        queries
            .iter()
            .position(|held| is_player(held) == player && query_id(held) == Some(id))
            .map(|at| at as u64)
    };
    let (world, player) = (at(false), at(true));
    Some((world.or(player)?, player.or(world)?))
}

/// The knowledge ids each query of the database `db` reads, its own parts'
/// and those of the queries its parts name, by query id, sorted; a world and
/// a player twin of one id read as one query.
fn knowledge_read(db: &Value) -> Vec<(u32, Vec<u32>)> {
    let Some(Value::List(queries)) = db.field(QUERIES) else {
        return Vec::new();
    };
    let mut direct: Vec<(u32, Vec<u32>, Vec<u32>)> = queries
        .iter()
        .filter_map(|query| {
            let Some(Value::List(actions)) = query.field(ACTIONS) else {
                return Some((query_id(query)?, Vec::new(), Vec::new()));
            };
            let (mut ids, mut named) = (Vec::new(), Vec::new());
            for action in actions {
                let id = action
                    .field(QUERY)
                    .and_then(|query| query.field(QUERY_DATA_ID))
                    .and_then(key_of_hash);
                match (id, names_query(action)) {
                    (Some(id), true) => named.push(id),
                    (Some(id), false) => ids.push(id),
                    (None, _) => {}
                }
            }
            Some((query_id(query)?, ids, named))
        })
        .collect();
    direct.sort_unstable_by_key(|(id, _, _)| *id);
    direct.dedup_by(|(id, ids, named), (kept, kept_ids, kept_named)| {
        *id == *kept && {
            kept_ids.append(ids);
            kept_named.append(named);
            true
        }
    });
    let find = |id: u32| direct.binary_search_by_key(&id, |(at, _, _)| *at).ok();
    direct
        .iter()
        .map(|(id, _, _)| {
            let (mut read, mut seen, mut next) = (Vec::new(), vec![*id], vec![*id]);
            while let Some(query) = next.pop() {
                let Some(at) = find(query) else { continue };
                let (_, ids, named) = &direct[at];
                read.extend_from_slice(ids);
                for &other in named {
                    if !seen.contains(&other) {
                        seen.push(other);
                        next.push(other);
                    }
                }
            }
            read.sort_unstable();
            read.dedup();
            (*id, read)
        })
        .collect()
}

/// The knowledge ids each game query reads in the database the game serves
/// now, mods' queries in, by query id, sorted.
pub(crate) fn served_knowledge_read(host: Host) -> Result<Vec<(u32, Vec<u32>)>, Off> {
    let db = host.served_value(database(host)?)?;
    Ok(knowledge_read(&db))
}

/// Gives the parts of the queries at `added` that name a query the indices
/// of that query among `queries`, in their final places.
fn resolve(queries: &mut [Value], added: impl Iterator<Item = usize>) {
    let mut found = Vec::new();
    for at in added {
        let Some(Value::List(actions)) = queries[at].field(ACTIONS) else {
            continue;
        };
        for (n, action) in actions.iter().enumerate().filter(|(_, a)| names_query(a)) {
            let id = action
                .field(QUERY)
                .and_then(|query| query.field(QUERY_DATA_ID))
                .and_then(key_of_hash);
            if let Some(indices) = id.and_then(|id| indices_of(queries, id)) {
                found.push((at, n, indices));
            }
        }
    }
    for (at, n, (world, player)) in found {
        let action = &mut actions_mut(&mut queries[at])[n];
        for (field, index) in [(QUERY_INDEX, world), (PLAYER_INDEX, player)] {
            if let Some(held) = action.field_mut(field) {
                *held = Value::Uint(index);
            }
        }
    }
}

/// The database `db` with the mod's queries `added` in it: world ones at
/// the end of the world range, player ones at the end, each query's index
/// its position and every part's index of a query past the world range
/// moved with it. A query whose id the database holds on its side, or
/// whose part names a query the database lacks, is left out and returned
/// with the reason, so the rest still serve.
fn add_to(mut db: Value, added: &[NewQuery]) -> Result<(Value, Vec<Off>), Off> {
    let start = match db.field(PLAYER_START) {
        Some(Value::Uint(n)) => usize::try_from(*n).ok(),
        _ => None,
    }
    .ok_or_else(|| Off::new("query database: no player start"))?;
    let Some(Value::List(held)) = db.field_mut(QUERIES) else {
        return Err(Off::new("query database: no list of queries"));
    };
    let mut queries = core::mem::take(held);
    if start > queries.len() {
        return Err(Off::new(format!(
            "query database: the player range starts at {start} of {}",
            queries.len()
        )));
    }
    let template = queries
        .iter()
        .find(|query| matches!(query.field(ACTIONS), Some(Value::List(a)) if !a.is_empty()))
        .cloned()
        .ok_or_else(|| Off::new("query database: no query to copy"))?;
    let mut refused = Vec::new();
    let mut kept: Vec<(&NewQuery, Value)> = Vec::new();
    for new in added {
        let known = |id: u32| {
            queries.iter().any(|held| query_id(held) == Some(id))
                || kept.iter().any(|(held, _)| held.key() == id)
        };
        let taken = queries
            .iter()
            .any(|held| is_player(held) == new.player() && query_id(held) == Some(new.key()))
            || kept
                .iter()
                .any(|(held, _)| held.player() == new.player() && held.key() == new.key());
        let missing = new
            .parts
            .iter()
            .find(|part| part.test == KnowledgeTest::Holds && !known(part.id.get()));
        let why = match (taken, missing) {
            (true, _) => "the database holds it".to_owned(),
            (_, Some(part)) => format!("no query {} in the database", part.id),
            _ => match new.to_value(&template) {
                Ok(value) => {
                    kept.push((new, value));
                    continue;
                }
                Err(off) => off.to_string(),
            },
        };
        refused.push(Off::new(format!("query {}: {why}", new.id)));
    }
    let worlds = kept.iter().filter(|(new, _)| !new.player()).count();
    for query in &mut queries {
        for action in actions_mut(query).iter_mut().filter(|a| names_query(a)) {
            for field in [QUERY_INDEX, PLAYER_INDEX] {
                if let Some(Value::Uint(index)) = action.field_mut(field)
                    && *index >= start as u64
                {
                    *index += worlds as u64;
                }
            }
        }
    }
    let (players, world): (Vec<_>, Vec<_>) = kept.into_iter().partition(|(new, _)| new.player());
    let end = start + world.len();
    queries.splice(start..start, world.into_iter().map(|(_, value)| value));
    let tail = queries.len();
    queries.extend(players.into_iter().map(|(_, value)| value));
    for (at, query) in queries.iter_mut().enumerate() {
        set(query, QUERY_INDEX, Value::Uint(at as u64))?;
    }
    let all = queries.len();
    resolve(&mut queries, (start..end).chain(tail..all));
    set(&mut db, PLAYER_START, Value::Uint(end as u64))?;
    set(&mut db, QUERIES, Value::List(queries))?;
    Ok((db, refused))
}

/// The game's own query ids and names, read when a mod adds or names its
/// first query.
#[derive(Debug)]
pub(crate) struct GameQueries {
    /// The database's key.
    key: ResourceKey,
    /// The ids of the game's queries, sorted.
    vanilla: Box<[u32]>,
    /// The game's queries by name, sorted by name.
    names: Box<[(Box<str>, u32)]>,
}

/// The queries a mod added, in the order it added them, which one patch of
/// the query database serves from the first.
#[derive(Debug)]
pub(crate) struct AddedQueries(Arc<Mutex<Vec<NewQuery>>>);

impl GameQueries {
    /// Reads the game's database.
    fn read(host: Host) -> Result<Self, Off> {
        let key = database(host)?;
        let db = host.game_value(key)?;
        let Some(Value::List(queries)) = db.field(QUERIES) else {
            return Err(Off::new("query database: no list of queries"));
        };
        let mut vanilla: Vec<u32> = queries.iter().filter_map(query_id).collect();
        vanilla.sort_unstable();
        vanilla.dedup();
        let mut names: Vec<(Box<str>, u32)> = queries
            .iter()
            .filter_map(|query| match query.field(NAME) {
                Some(Value::Text(name)) => Some((name.as_str().into(), query_id(query)?)),
                _ => None,
            })
            .collect();
        names.sort_unstable();
        names.dedup();
        Ok(Self {
            key,
            vanilla: vanilla.into(),
            names: names.into(),
        })
    }
}

impl AddedQueries {
    /// Serves the patch of the database `key` that adds the mod's queries;
    /// a query refused then is logged and left out.
    fn serve(host: Host, key: ResourceKey) -> Result<Self, Off> {
        serve_list(host, key, move |db, added| {
            let (db, refused) = add_to(db, added)?;
            for off in refused {
                host.log(Level::WARN, &off.to_string());
            }
            Ok(db)
        })
        .map(Self)
    }
}

/// The key of the game's query database, the one resource of its type.
fn database(host: Host) -> Result<ResourceKey, Off> {
    let hash = host.hash(GameKnowledgeQueryResourceDb::ID)?;
    let ty = Registry::game()?
        .hashed(hash)
        .ok_or_else(|| Off::new("query database: its type is not reflected"))?;
    match host.resource_keys(Some(ty))?.as_slice() {
        [one] => Ok(*one),
        keys => Err(Off::new(format!(
            "query database: {} in the archive, not one",
            keys.len()
        ))),
    }
}

/// The game's queries, read on the first call; serves nothing.
fn game(load: &mut crate::BaseLoad) -> Result<&GameQueries, Off> {
    let host = load.host();
    Ok(match load.slot::<GameQueries>() {
        Some(queries) => queries,
        empty => empty.insert(GameQueries::read(host)?),
    })
}

/// Refuses `new` where [`add`] would, before its id is declared.
pub(super) fn check(load: &mut crate::BaseLoad, new: &NewQuery) -> Result<(), Off> {
    new.check()?;
    if game(load)?.vanilla.binary_search(&new.key()).is_ok() {
        return Err(Off::new(format!("query {}: the game holds it", new.id)));
    }
    Ok(())
}

/// Adds `new` to the queries the mod `load` serves, serving the patch with
/// the first.
pub(super) fn add(load: &mut crate::BaseLoad, new: NewQuery) -> Result<GameQuery, Off> {
    check(load, &new)?;
    let key = game(load)?.key;
    let host = load.host();
    let list = match load.slot::<AddedQueries>() {
        Some(added) => Arc::clone(&added.0),
        empty => Arc::clone(&empty.insert(AddedQueries::serve(host, key)?).0),
    };
    load.declare(new.id)?;
    let query = GameQuery::new(new.key());
    list.relock().push(new);
    Ok(query)
}

/// The game's query named `name`.
pub(super) fn named(load: &mut crate::BaseLoad, name: &str) -> Result<GameQuery, Off> {
    let names = &game(load)?.names;
    let at = names.partition_point(|(held, _)| &**held < name);
    match names.get(at) {
        Some((held, id)) if &**held == name => Ok(GameQuery::new(*id)),
        _ => Err(Off::new(format!(
            "query {name}: no query of the game's database"
        ))),
    }
}

#[cfg(test)]
mod tests;
