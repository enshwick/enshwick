//! Values between Luau and the API, shaped by the script [`Ty`] of each.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::script::{Callback, Choice, Class, Error, Handle, Signature, Ty, Union, Value};
use mlua::{Lua, MultiValue, Value as LuaValue};

use crate::run::Scope;

/// What a Luau value is called in an error.
fn kind(value: &LuaValue) -> &'static str {
    match value {
        LuaValue::Nil => "nil",
        LuaValue::Boolean(_) => "a boolean",
        LuaValue::Integer(_) | LuaValue::Number(_) => "a number",
        LuaValue::String(_) => "a string",
        LuaValue::Vector(_) => "a vector",
        LuaValue::Table(_) => "a table",
        LuaValue::Function(_) => "a function",
        LuaValue::Buffer(_) => "a buffer",
        LuaValue::UserData(_) => "an object",
        none if *none == LuaValue::NULL => "resources.none()",
        _ => "another kind of value",
    }
}

fn expected(what: &str, value: &LuaValue) -> Error {
    Error::new(format!("expected {what}, got {}", kind(value)))
}

/// The Luau value of `value`, a `ty`.
pub(crate) fn to_lua(
    lua: &Lua,
    scope: &Scope,
    value: Value,
    ty: &'static Ty,
) -> mlua::Result<LuaValue> {
    Ok(match (value, ty) {
        (Value::Nil, Ty::Optional(_)) => LuaValue::Nil,
        // Before the map arms, so an optional union's value keeps its type.
        (value, Ty::Optional(inner_ty)) => to_lua(lua, scope, value, inner_ty)?,
        (Value::Bool(b), _) => LuaValue::Boolean(b),
        (Value::Number(n), _) => LuaValue::Number(n),
        (Value::String(s), _) => LuaValue::String(lua.create_string(s)?),
        (Value::Name(s), _) => LuaValue::String(Interned::name(lua, s)?),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a Luau vector holds three f32"
        )]
        (Value::Vector([x, y, z]), _) => {
            LuaValue::Vector(mlua::Vector::new(x as f32, y as f32, z as f32))
        }
        (Value::List(items), Ty::List(inner_ty)) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for item in items {
                table.raw_push(to_lua(lua, scope, item, inner_ty)?)?;
            }
            LuaValue::Table(table)
        }
        (Value::Record(fields), Ty::Record(record)) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (value, field) in fields.into_iter().zip(record.fields()) {
                table.raw_set(field.name(), to_lua(lua, scope, value, field.ty())?)?;
            }
            LuaValue::Table(table)
        }
        (Value::List(items), Ty::Data) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for item in items {
                table.raw_push(to_lua(lua, scope, item, &Ty::Data)?)?;
            }
            LuaValue::Table(table)
        }
        (Value::Bytes(bytes), _) => LuaValue::Buffer(lua.create_buffer(bytes)?),
        (Value::Object(handle), _) => LuaValue::UserData(lua.create_userdata(Object {
            handle,
            scope: scope.clone(),
            ended: false,
        })?),
        (Value::Map(fields), Ty::Union(union)) => {
            let table = lua.create_table_with_capacity(0, 1)?;
            for (name, value) in fields {
                let ty = union
                    .variant(&name)
                    .and_then(enshwick::script::Variant::ty)
                    .unwrap_or(&Ty::Data);
                table.raw_set(name, to_lua(lua, scope, value, ty)?)?;
            }
            LuaValue::Table(table)
        }
        (Value::Map(fields), _) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            // An absent field reads as nil, so `if value.field then` holds.
            for (name, value) in fields.into_iter().filter(|(_, v)| !matches!(v, Value::Nil)) {
                table.raw_set(name, to_lua(lua, scope, value, &Ty::Data)?)?;
            }
            LuaValue::Table(table)
        }
        // A list drops a nil item, so nil inside game data is `none`.
        (Value::Nil, Ty::Data) => LuaValue::NULL,
        (Value::Function(callback), Ty::Function(signature)) => {
            let scope = scope.clone();
            LuaValue::Function(lua.create_function(move |lua, args: LuaArgs| {
                let mut values = arguments(&scope, args, signature, "function")?;
                let answer = callback
                    .call(values.slots())
                    .map_err(mlua::Error::runtime)?;
                to_lua(lua, &scope, answer, signature.ret())
            })?)
        }
        _ => LuaValue::Nil,
    })
}

/// The API value of the Luau value `value`, which is to be a `ty`.
pub(crate) fn from_lua(scope: &Scope, value: LuaValue, ty: &'static Ty) -> Result<Value, Error> {
    match ty {
        Ty::Nil => Ok(Value::Nil),
        Ty::Bool => match value {
            LuaValue::Boolean(b) => Ok(Value::Bool(b)),
            other => Err(expected("a boolean", &other)),
        },
        Ty::Number => match value {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a Luau integer past 2^53 has no exact f64; the API's numbers are smaller"
            )]
            LuaValue::Integer(i) => Ok(Value::Number(i as f64)),
            LuaValue::Number(n) => Ok(Value::Number(n)),
            other => Err(expected("a number", &other)),
        },
        Ty::Choice(choice) => choice_from_lua(scope, value, choice),
        Ty::String => match value {
            LuaValue::String(s) => Ok(Value::String(
                s.to_str()
                    .map_err(|_| Error::new("expected text in UTF-8"))?
                    .to_owned(),
            )),
            other => Err(expected("a string", &other)),
        },
        Ty::Vector => match value {
            LuaValue::Vector(v) => Ok(Value::Vector([v.x(), v.y(), v.z()].map(f64::from))),
            other => Err(expected("a vector", &other)),
        },
        Ty::Optional(inner_ty) => match value {
            LuaValue::Nil => Ok(Value::Nil),
            value => from_lua(scope, value, inner_ty),
        },
        Ty::List(inner_ty) => {
            let LuaValue::Table(table) = value else {
                return Err(expected("a list", &value));
            };
            let mut items = Vec::new();
            for (i, item) in table.sequence_values::<LuaValue>().enumerate() {
                let item = item.map_err(|e| Error::new(e.to_string()))?;
                items.push(
                    from_lua(scope, item, inner_ty)
                        .map_err(|e| Error::new(format!("[{}]: {e}", i + 1)))?,
                );
            }
            Ok(Value::List(items))
        }
        Ty::Record(record) => {
            let LuaValue::Table(table) = value else {
                return Err(expected("a table", &value));
            };
            record
                .fields()
                .iter()
                .map(|field| {
                    let value: LuaValue = table
                        .raw_get(field.name())
                        .map_err(|e| Error::new(e.to_string()))?;
                    if field.has_default() && value.is_nil() {
                        return Ok(Value::Nil);
                    }
                    from_lua(scope, value, field.ty())
                        .map_err(|e| Error::new(format!("{}: {e}", field.name())))
                })
                .collect::<Result<_, _>>()
                .map(Value::Record)
        }
        Ty::Function(signature) => {
            let LuaValue::Function(function) = value else {
                return Err(expected("a function", &value));
            };
            let scope = scope.clone();
            let faulted = scope.clone();
            let lends = signature
                .params()
                .iter()
                .any(|p| matches!(p.ty(), Ty::Object(class) if class.is_lent()));
            // An empty slice allocates nothing: only a function lent an object pays.
            let lent: Box<[Lent]> = if lends {
                signature.params().iter().map(|_| Lent::default()).collect()
            } else {
                Box::default()
            };
            Ok(Value::Function(
                Callback::new(move |args| scope.call_function(&function, args, signature, &lent))
                    .with_fault(move |error| faulted.fault(error)),
            ))
        }
        Ty::Bytes => match value {
            LuaValue::Buffer(buffer) => Ok(Value::Bytes(buffer.to_vec())),
            LuaValue::String(text) => Ok(Value::Bytes(text.as_bytes().to_vec())),
            other => Err(expected("a buffer", &other)),
        },
        Ty::Data => data(value, 0),
        Ty::Union(union) => union_from_lua(scope, value, union),
        Ty::Object(class) => object_from_lua(&value, class),
        _ => Err(Error::new("a type this runtime does not know")),
    }
}

/// The API value of `value`, a variant of `choice`: the variant's own
/// static name, which also checks it.
fn choice_from_lua(scope: &Scope, value: LuaValue, choice: &Choice) -> Result<Value, Error> {
    let LuaValue::String(s) = value else {
        return Err(expected("a string", &value));
    };
    if let Some(Some(name)) = scope.with_lua(|lua| Interned::variant(lua, choice, &s)) {
        return Ok(Value::Name(name));
    }
    let text = s
        .to_str()
        .map_err(|_| Error::new("expected text in UTF-8"))?;
    choice
        .names()
        .find(|name| *name == &*text)
        .map(Value::Name)
        .ok_or_else(|| {
            let names: Vec<String> = choice.names().map(|name| format!("{name:?}")).collect();
            Error::new(format!("{:?} is not one of {}", &*text, names.join(", ")))
        })
}

/// The API value of `value`, a variant of `union`: its name, or a table of
/// one variant and its value.
fn union_from_lua(scope: &Scope, value: LuaValue, union: &Union) -> Result<Value, Error> {
    match value {
        LuaValue::String(name) => {
            let name = name
                .to_str()
                .map_err(|_| Error::new("expected text in UTF-8"))?;
            match union.variant(&name) {
                Some(variant) if variant.ty().is_none() => Ok(Value::String(name.to_owned())),
                _ => Err(Error::new(format!(
                    "{name:?} is no {} without a value",
                    union.name()
                ))),
            }
        }
        LuaValue::Table(table) => {
            let mut pairs = table.pairs::<String, LuaValue>();
            let (Some(pair), None) = (pairs.next(), pairs.next()) else {
                return Err(Error::new(format!(
                    "expected a {}: a table of one variant and its value",
                    union.name()
                )));
            };
            let (name, inner) = pair.map_err(|e| Error::new(e.to_string()))?;
            let Some(ty) = union.variant(&name).and_then(enshwick::script::Variant::ty) else {
                return Err(Error::new(format!(
                    "{name:?} is no {} with a value",
                    union.name()
                )));
            };
            let inner =
                from_lua(scope, inner, ty).map_err(|e| Error::new(format!("{name}: {e}")))?;
            Ok(Value::Map(vec![(name, inner)]))
        }
        other => Err(expected(union.name(), &other)),
    }
}

/// The API value of `value`, an object of `class`.
fn object_from_lua(value: &LuaValue, class: &'static Class) -> Result<Value, Error> {
    let LuaValue::UserData(object) = value else {
        return Err(expected(class.name(), value));
    };
    match object.borrow::<Object>() {
        // A class is a `const`, whose address differs between uses; the
        // name picks it, and `FromScript` checks the Rust type.
        Ok(object) if !object.ended && object.handle.class().name() == class.name() => {
            Ok(Value::Object(object.handle.clone()))
        }
        _ => Err(expected(class.name(), value)),
    }
}

/// The script object one argument of a script's function gets when its class
/// is [lent](Class::lent): kept from call to call, so a filter on a hot path
/// makes no object per call.
#[derive(Default)]
pub(crate) struct Lent {
    object: OnceLock<mlua::AnyUserData>,
    /// Whether a call holds the kept object now.
    busy: AtomicBool,
}

impl Lent {
    /// The script object holding `handle`, and whether it is the kept one,
    /// which [`Lent::give_back`] returns once the call ends; a call inside
    /// the one that holds it gets an object of its own.
    pub(crate) fn lend(
        &self,
        lua: &Lua,
        scope: &Scope,
        handle: Handle,
    ) -> mlua::Result<(LuaValue, bool)> {
        if self.busy.swap(true, Ordering::Acquire) {
            let object = lua.create_userdata(Object {
                handle,
                scope: scope.clone(),
                ended: false,
            })?;
            return Ok((LuaValue::UserData(object), false));
        }
        let kept = match self.object.get() {
            Some(kept) => kept
                .borrow_mut::<Object>()
                .map(|mut object| {
                    object.handle = handle;
                    object.ended = false;
                })
                .map(|()| kept.clone()),
            None => lua
                .create_userdata(Object {
                    handle,
                    scope: scope.clone(),
                    ended: false,
                })
                .map(|made| self.object.get_or_init(|| made).clone()),
        };
        kept.map(|kept| (LuaValue::UserData(kept), true))
            .inspect_err(|_| self.give_back())
    }

    /// Ends the call that holds the kept object, which refuses use until the
    /// next call lends it again; run under the VM's lock, as `lend` is.
    pub(crate) fn give_back(&self) {
        if let Some(mut object) = self
            .object
            .get()
            .and_then(|o| o.borrow_mut::<Object>().ok())
        {
            object.ended = true;
        }
        self.busy.store(false, Ordering::Release);
    }
}

/// What a VM keeps of the API's own names, so a hot call pushes no text:
/// under a memory limit mlua pushes text in protected mode, which on Luau
/// leaves a closure behind on every push.
#[derive(Default)]
struct Interned {
    /// The script string of each name a choice or a variant crossed as.
    names: Vec<(&'static str, mlua::LuaString)>,
    /// Each class's table of method functions, by method name, filled as
    /// `__index` makes them.
    methods: Vec<(&'static str, mlua::Table)>,
    /// Each choice's table from a variant's script string to its place.
    choices: Vec<(&'static str, mlua::Table)>,
}

impl Interned {
    /// The entry of `key` in the list `pick` chooses, made by `make` and
    /// kept on first use.
    fn get<T: Clone>(
        lua: &Lua,
        key: &'static str,
        pick: fn(&mut Self) -> &mut Vec<(&'static str, T)>,
        make: impl FnOnce() -> mlua::Result<T>,
    ) -> mlua::Result<T> {
        if let Some(mut interned) = lua.app_data_mut::<Self>()
            && let Some((_, kept)) = pick(&mut interned).iter().find(|(k, _)| *k == key)
        {
            return Ok(kept.clone());
        }
        let value = make()?;
        if lua.app_data_ref::<Self>().is_none() {
            let _none = lua.set_app_data(Self::default());
        }
        if let Some(mut interned) = lua.app_data_mut::<Self>() {
            pick(&mut interned).push((key, value.clone()));
        }
        Ok(value)
    }

    /// The script string of the API's name `name`.
    fn name(lua: &Lua, name: &'static str) -> mlua::Result<mlua::LuaString> {
        Self::get(lua, name, |i| &mut i.names, || lua.create_string(name))
    }

    /// The table of method functions of the class `class`.
    fn methods(lua: &Lua, class: &'static str) -> mlua::Result<mlua::Table> {
        Self::get(lua, class, |i| &mut i.methods, || lua.create_table())
    }

    /// The variant of `choice` the script string `text` names, read without
    /// borrowing its bytes: borrowing a fresh string's bytes allocates.
    fn variant(lua: &Lua, choice: &Choice, text: &mlua::LuaString) -> Option<&'static str> {
        let table = Self::get(
            lua,
            choice.name(),
            |i| &mut i.choices,
            || {
                let table = lua.create_table_with_capacity(0, choice.values().len())?;
                for (at, name) in choice.names().enumerate() {
                    table.raw_set(name, at)?;
                }
                Ok(table)
            },
        )
        .ok()?;
        let at = table.raw_get::<Option<usize>>(text).ok()??;
        choice.names().nth(at)
    }
}

/// An [`Object`](enshwick::script::Object) as a script holds it: its
/// class's methods, called as `object:method()`.
struct Object {
    handle: Handle,
    scope: Scope,
    /// A kept [`Lent`] object between its calls: a script that kept it gets
    /// an error, never the next call's value.
    ended: bool,
}

impl mlua::UserData for Object {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(
            mlua::MetaMethod::Index,
            |lua, this, key: mlua::LuaString| {
                let class = this.handle.class();
                // One function per class and method in a VM, made on first use:
                // a filter on a hot path indexes methods on every hit.
                let cached = Interned::methods(lua, class.name())?;
                if let Some(function) = cached.raw_get::<Option<mlua::Function>>(&key)? {
                    return Ok(LuaValue::Function(function));
                }
                let key = key.to_str()?.to_owned();
                let Some(method) = class.methods().into_iter().find(|m| m.name() == key) else {
                    return Ok(LuaValue::Nil);
                };
                let scope = this.scope.clone();
                let what = format!("{}:{key}", class.name());
                let function = lua.create_function(
                    move |lua, (object, args): (mlua::AnyUserData, LuaArgs)| {
                        let this = object.borrow::<Self>()?;
                        if this.ended {
                            return Err(mlua::Error::runtime(format!(
                                "{what}: the object was lent to a call that ended"
                            )));
                        }
                        let handle = this.handle.clone();
                        drop(this);
                        // A method indexes its own class's list: `a.m(b)` must not
                        // run b's method at m's place.
                        if handle.class().name() != class.name() {
                            return Err(mlua::Error::runtime(format!(
                                "{what}: called on a {}",
                                handle.class().name()
                            )));
                        }
                        let signature = method.signature();
                        let mut values = arguments(&scope, args, signature, &what)?;
                        let answer = handle
                            .call(&method, values.slots())
                            .map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                        to_lua(lua, &scope, answer, signature.ret())
                    },
                )?;
                cached.raw_set(key, &function)?;
                Ok(LuaValue::Function(function))
            },
        );
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(this.handle.class().name())
        });
    }
}

/// Tables nested deeper than this are taken for one that holds itself.
const MAX_DATA_DEPTH: usize = 64;

/// The API value of game data: a table with only the keys 1 to n is a list,
/// any other a table keyed by name.
fn data(value: LuaValue, depth: usize) -> Result<Value, Error> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a Luau integer past 2^53 has no exact f64; game data past it crosses as text"
    )]
    Ok(match value {
        LuaValue::Nil => Value::Nil,
        none if none == LuaValue::NULL => Value::Nil,
        LuaValue::Boolean(b) => Value::Bool(b),
        LuaValue::Buffer(buffer) => Value::Bytes(buffer.to_vec()),
        LuaValue::Integer(i) => Value::Number(i as f64),
        LuaValue::Number(n) => Value::Number(n),
        LuaValue::String(s) => Value::String(
            s.to_str()
                .map_err(|_| Error::new("expected text in UTF-8"))?
                .to_owned(),
        ),
        LuaValue::Table(table) => {
            if depth >= MAX_DATA_DEPTH {
                return Err(Error::new("game data nested deeper than 64 tables"));
            }
            let len = table.raw_len();
            let mut keyed = Vec::new();
            for pair in table.clone().pairs::<LuaValue, LuaValue>() {
                let (key, item) = pair.map_err(|e| Error::new(e.to_string()))?;
                match key {
                    LuaValue::String(name) => keyed.push((
                        name.to_str()
                            .map_err(|_| Error::new("expected a field name in UTF-8"))?
                            .to_owned(),
                        item,
                    )),
                    LuaValue::Integer(i)
                        if i >= 1 && usize::try_from(i).is_ok_and(|i| i <= len) => {}
                    other => return Err(expected("a field name or a list index", &other)),
                }
            }
            if keyed.is_empty() {
                let mut items = Vec::with_capacity(len);
                for (i, item) in table.sequence_values::<LuaValue>().enumerate() {
                    let item = item.map_err(|e| Error::new(e.to_string()))?;
                    items.push(
                        data(item, depth + 1)
                            .map_err(|e| Error::new(format!("[{}]: {e}", i + 1)))?,
                    );
                }
                if items.len() != len {
                    return Err(Error::new(format!(
                        "a list with a hole: item {} of {len} is nil",
                        items.len() + 1
                    )));
                }
                Value::List(items)
            } else if len > 0 {
                return Err(Error::new(
                    "a table of data is either a list or keyed by name",
                ));
            } else {
                let mut fields = Vec::with_capacity(keyed.len());
                for (name, item) in keyed {
                    let item =
                        data(item, depth + 1).map_err(|e| Error::new(format!("{name}: {e}")))?;
                    fields.push((name, item));
                }
                Value::Map(fields)
            }
        }
        other => return Err(expected("game data", &other)),
    })
}

pub(crate) fn arguments(
    scope: &Scope,
    args: impl Spread,
    signature: &'static Signature,
    what: &str,
) -> mlua::Result<Args> {
    let params = signature.params();
    let (given, mut values) = args.spread();
    if given > params.len() {
        return Err(mlua::Error::runtime(format!(
            "{what} takes {} arguments, not {given}",
            params.len(),
        )));
    }
    let mut out = Args::new(params.len());
    for (slot, param) in out.slots().iter_mut().zip(params) {
        let value = values.next().unwrap_or(LuaValue::Nil);
        *slot = from_lua(scope, value, param.ty()).map_err(|e| {
            let name = if param.name().is_empty() {
                "argument"
            } else {
                param.name()
            };
            mlua::Error::runtime(format!("{what}: {name}: {e}"))
        })?;
    }
    Ok(out)
}

/// The arguments of a call into the API as mlua hands them over: the first
/// four straight from the stack, any past them in the list, which stays
/// empty, and so unallocated, for a call of four or fewer.
pub(crate) type LuaArgs = (LuaValue, LuaValue, LuaValue, LuaValue, MultiValue);

/// Arguments from Luau: how many the script passed, and the values in order.
pub(crate) trait Spread {
    fn spread(self) -> (usize, impl Iterator<Item = LuaValue>);
}

impl Spread for LuaArgs {
    /// A trailing nil among the first four counts as not passed.
    fn spread(self) -> (usize, impl Iterator<Item = LuaValue>) {
        let (a, b, c, d, rest) = self;
        let first = [a, b, c, d];
        let given = if rest.is_empty() {
            first
                .iter()
                .rposition(|v| !v.is_nil())
                .map_or(0, |at| at + 1)
        } else {
            first.len() + rest.len()
        };
        (given, first.into_iter().chain(rest))
    }
}

impl Spread for MultiValue {
    fn spread(self) -> (usize, impl Iterator<Item = LuaValue>) {
        (self.len(), self.into_iter())
    }
}

/// The API values of a call's arguments: eight in place, more in a list.
pub(crate) struct Args {
    near: [Value; 8],
    far: Vec<Value>,
    len: usize,
}

impl Args {
    fn new(len: usize) -> Self {
        let far = if len > 8 {
            (0..len).map(|_| Value::Nil).collect()
        } else {
            Vec::new()
        };
        Self {
            near: [const { Value::Nil }; 8],
            far,
            len,
        }
    }

    /// One value per parameter, which the API takes out of the slice.
    pub(crate) fn slots(&mut self) -> &mut [Value] {
        if self.len > self.near.len() {
            &mut self.far
        } else {
            &mut self.near[..self.len]
        }
    }
}

/// The Luau value of the TOML value `value`, such as a setting's; a date
/// becomes its text.
pub(crate) fn toml_to_lua(lua: &Lua, value: &toml::Value) -> mlua::Result<LuaValue> {
    Ok(match value {
        toml::Value::String(s) => LuaValue::String(lua.create_string(s)?),
        #[expect(
            clippy::cast_precision_loss,
            reason = "a Luau number holds 53 bits; a setting past that is not exact"
        )]
        toml::Value::Integer(i) => LuaValue::Number(*i as f64),
        toml::Value::Float(f) => LuaValue::Number(*f),
        toml::Value::Boolean(b) => LuaValue::Boolean(*b),
        toml::Value::Datetime(d) => LuaValue::String(lua.create_string(d.to_string())?),
        toml::Value::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for item in items {
                table.raw_push(toml_to_lua(lua, item)?)?;
            }
            LuaValue::Table(table)
        }
        toml::Value::Table(entries) => {
            let table = lua.create_table_with_capacity(0, entries.len())?;
            for (key, item) in entries {
                table.raw_set(key.as_str(), toml_to_lua(lua, item)?)?;
            }
            LuaValue::Table(table)
        }
    })
}

#[cfg(test)]
mod tests {
    use enshwick::script::{IntoScript as _, Typed};

    use super::*;

    #[derive(Clone, Debug, enshwick::Script)]
    struct Count {
        /// How many.
        pub n: u32,
    }

    #[derive(Clone, Debug, enshwick::Script)]
    enum Status {
        /// Done, with a count.
        Done(Count),
        /// Not yet.
        Waiting,
    }

    #[test]
    fn an_optional_union_s_variant_keeps_its_record() {
        let lua = Lua::new();
        let value = Some(Status::Done(Count { n: 3 })).into_value();
        let ty = <Option<Status> as Typed>::TY;
        let LuaValue::Table(table) = to_lua(&lua, &Scope::detached(), value, ty).unwrap() else {
            panic!("no table");
        };
        let done: mlua::Table = table.get("Done").unwrap();
        assert_eq!(done.get::<u32>("n").unwrap(), 3);
        let waiting = to_lua(
            &lua,
            &Scope::detached(),
            Some(Status::Waiting).into_value(),
            ty,
        );
        assert_eq!(waiting.unwrap().to_string().unwrap(), "Waiting");
    }
}
