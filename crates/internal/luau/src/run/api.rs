//! The `enshwick` table a mod's scripts see: the game namespaces, settings,
//! dependencies, and the subscriptions, events and calls they set up at start.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use enshwick_core::abi::Level;
use enshwick_core::points::{Decision, DescribedOwnedPoint, Order};
use enshwick_core::script;
use enshwick_core::{Load, Relock as _};
use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};

use super::{Asked, Defined, Inner, Owned, Scope, Vm};
use crate::bridge::{LuaArgs, arguments, to_lua};
use crate::calls::{CallDesc, ServedCall};
use crate::files::{Requirer, Scripts};
use crate::points::{EventDesc, PointDesc, Read, decode, encode};
use crate::subscribe::Subscription;

impl Vm {
    /// Every subscription a handler was registered for, once each, in the
    /// order of the first.
    pub(super) fn events(&self) -> Vec<Subscription> {
        let mut events: Vec<Subscription> = Vec::new();
        for (sub, _) in self.handlers.relock().iter() {
            if !events.contains(sub) {
                events.push(sub.clone());
            }
        }
        events
    }

    fn subscribe(&self, what: &str, sub: Subscription, handler: Function) -> mlua::Result<()> {
        if !self.starting.load(Ordering::Acquire) {
            return Err(mlua::Error::runtime(format!(
                "{what}: subscribe at the top level of main.luau, while the mod starts"
            )));
        }
        self.handlers.relock().push((sub, handler));
        Ok(())
    }

    fn on(&self, name: &str, handler: Function) -> mlua::Result<()> {
        let sub = Subscription::named(name).ok_or_else(|| {
            mlua::Error::runtime(format!("enshwick.on: no event is named {name:?}"))
        })?;
        self.subscribe("enshwick.on", sub, handler)
    }
}

impl Inner {
    /// `enshwick.deps`: a read-only table per dependency alias, with the
    /// mod's `id` and `running()`, whether it runs now.
    fn deps_table(&self, lua: &Lua) -> mlua::Result<Table> {
        let deps = lua.create_table_with_capacity(0, self.deps.len())?;
        for (alias, id) in &self.deps {
            let dep = lua.create_table_with_capacity(0, 2)?;
            dep.raw_set("id", id.as_str())?;
            let id = id.clone();
            dep.raw_set(
                "running",
                lua.create_function(move |_, ()| Ok(enshwick_core::mods::is_running(&id)))?,
            )?;
            dep.set_readonly(true);
            deps.raw_set(alias.as_str(), dep)?;
        }
        deps.set_readonly(true);
        Ok(deps)
    }

    /// Puts `define_event` and `caller` into `api`: what the scripts offer
    /// other mods and ask of them, settled at start.
    fn mod_functions(&self, lua: &Lua, api: &Table) -> mlua::Result<()> {
        let me = Weak::clone(&self.me);
        api.raw_set(
            "define_event",
            lua.create_function(move |lua, event: Table| {
                let inner = me
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                inner.define_event(lua, &event)
            })?,
        )?;
        let me = Weak::clone(&self.me);
        namespace_table(lua, api, "points")?.raw_set(
            "define_point",
            lua.create_function(move |lua, name: String| {
                let inner = me
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                inner.define_point(lua, &name)
            })?,
        )?;
        let me = Weak::clone(&self.me);
        api.raw_set(
            "caller",
            lua.create_function(move |lua, call: Table| {
                let inner = me
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                inner.caller(lua, &call)
            })?,
        )?;
        Ok(())
    }

    /// Puts `serve_call` into `api`: the calls `vm`'s scripts serve other
    /// mods, settled at start.
    fn serve_functions(&self, vm: &Arc<Vm>, api: &Table) -> mlua::Result<()> {
        let weak = Arc::downgrade(vm);
        let id = self.id.clone();
        api.raw_set(
            "serve_call",
            vm.lua
                .create_function(move |_, (call, handler): (Table, Function)| {
                    let vm = weak
                        .upgrade()
                        .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                    let desc = ServedCall::parse(&call, &id)?;
                    let name = desc.call.name.clone();
                    {
                        let mut served = vm.served.relock();
                        if served.iter().any(|d| d.call.name == name) {
                            return Err(mlua::Error::runtime(format!(
                                "enshwick.serve_call: {name} is served already"
                            )));
                        }
                        served.push(desc);
                    }
                    vm.subscribe("enshwick.serve_call", Subscription::Call(name), handler)
                })?,
        )
    }

    /// `enshwick.extend(point, handler, order)`: links the handler into the
    /// point, its order parsed as `points::Order` reads.
    fn extend_function(&self, vm: &Arc<Vm>) -> mlua::Result<Function> {
        let weak = Arc::downgrade(vm);
        let bridge = Scope {
            inner: Weak::clone(&self.me),
            vm: Arc::downgrade(vm),
        };
        vm.lua.create_function(
            move |_, (point, handler, order): (Table, Function, LuaValue)| {
                let vm = weak
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                let mut desc = PointDesc::parse(&point)?;
                if !order.is_nil() {
                    desc.order =
                        crate::bridge::from_lua(&bridge, order, <Order as script::Typed>::TY)
                            .and_then(script::FromScript::from_value)
                            .map_err(|e| {
                                mlua::Error::runtime(format!("enshwick.extend: order: {e}"))
                            })?;
                }
                let sub = Subscription::Point(desc.point.name.clone());
                {
                    // A script's handlers of one point make one link, with one order.
                    let mut points = vm.points.relock();
                    match points.iter().find(|d| d.point.name == desc.point.name) {
                        Some(linked) if linked.order != desc.order => {
                            return Err(mlua::Error::runtime(format!(
                                "enshwick.extend: {} is linked already, in another order",
                                desc.point.name
                            )));
                        }
                        Some(_) => {}
                        None => points.push(desc),
                    }
                }
                vm.subscribe("enshwick.extend", sub, handler)
            },
        )
    }

    /// Puts `extend`, `refuse`, `on_game_type`, `game_event_types` and `on`
    /// into `api`, the subscriptions `vm`'s scripts make at start.
    fn subscription_functions(&self, vm: &Arc<Vm>, api: &Table) -> mlua::Result<()> {
        let lua = &vm.lua;
        let refuse = lua.create_table()?;
        refuse.set_readonly(true);
        let _first = vm.refuse.set(refuse.clone());
        api.raw_set("refuse", refuse)?;
        api.raw_set("extend", self.extend_function(vm)?)?;
        let weak = Arc::downgrade(vm);
        api.raw_set(
            "on_mod_event",
            lua.create_function(move |_, (event, handler): (Table, Function)| {
                let vm = weak
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                let desc = EventDesc::parse(&event)?;
                let sub = Subscription::ModEvent(desc.event.name.clone());
                {
                    let mut events = vm.events.relock();
                    match events.iter().find(|d| d.event.name == desc.event.name) {
                        Some(known) if *known != desc => {
                            return Err(mlua::Error::runtime(format!(
                                "enshwick.on_mod_event: {} is described twice, differently",
                                desc.event.name
                            )));
                        }
                        Some(_) => {}
                        None => events.push(desc),
                    }
                }
                vm.subscribe("enshwick.on_mod_event", sub, handler)
            })?,
        )?;
        let weak = Arc::downgrade(vm);
        api.raw_set(
            "on_game_type",
            lua.create_function(
                move |_, (name, fields, handler): (String, Table, Function)| {
                    let vm = weak
                        .upgrade()
                        .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                    let sub = Subscription::raw(name, &fields)?;
                    vm.subscribe("enshwick.on_game_type", sub, handler)
                },
            )?,
        )?;
        let types = self.event_types.clone();
        api.raw_set(
            "game_event_types",
            lua.create_function(move |lua, ()| {
                let list = lua.create_table_with_capacity(types.len(), 0)?;
                for (name, size) in &types {
                    let entry = lua.create_table_with_capacity(0, 2)?;
                    entry.raw_set("name", *name)?;
                    entry.raw_set("size", *size)?;
                    list.raw_push(entry)?;
                }
                Ok(list)
            })?,
        )?;
        let weak = Arc::downgrade(vm);
        api.raw_set(
            "on",
            lua.create_function(move |_, (name, handler): (String, Function)| {
                let vm = weak
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                vm.on(&name, handler)
            })?,
        )?;
        Ok(())
    }

    /// Puts `enshwick`, `print` and `require` among the globals of `vm`.
    pub(super) fn globals(&self, vm: &Arc<Vm>, scripts: Scripts) -> mlua::Result<Table> {
        let lua = &vm.lua;
        let scope = Scope {
            inner: Weak::clone(&self.me),
            vm: Arc::downgrade(vm),
        };
        let api = lua.create_table()?;
        for (namespace, opened) in &self.namespaces {
            let table = namespace_table(lua, &api, namespace.name())?;
            for declared in namespace.functions() {
                let scope = scope.clone();
                let opened = opened.clone();
                let what = format!("enshwick.{}.{}", namespace.name(), declared.name());
                let signature = declared.signature();
                table.raw_set(
                    declared.name(),
                    lua.create_function(move |lua, args: LuaArgs| {
                        let mut values = arguments(&scope, args, signature, &what)?;
                        let answer = opened
                            .call(&declared, values.slots())
                            .map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                        to_lua(lua, &scope, answer, signature.ret())
                    })?,
                )?;
            }
        }
        let settings = scope.clone();
        api.raw_set(
            "settings",
            lua.create_function(move |lua, ()| {
                let inner = settings
                    .inner
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                let values = inner.settings.load();
                crate::bridge::toml_to_lua(lua, &toml::Value::Table((**values).clone()))
            })?,
        )?;
        api.raw_set("deps", self.deps_table(lua)?)?;
        self.subscription_functions(vm, &api)?;
        self.serve_functions(vm, &api)?;
        follow_settings(lua, &api)?;
        self.mod_functions(lua, &api)?;
        let globals = lua.globals();
        globals.raw_set("enshwick", api.clone())?;
        let host = self.host;
        globals.raw_set(
            "print",
            lua.create_function(move |_, args: MultiValue| {
                let line = args
                    .iter()
                    .map(|v| v.to_string().unwrap_or_else(|_| "?".to_owned()))
                    .collect::<Vec<_>>()
                    .join("\t");
                host.log(Level::INFO, &line);
                Ok(())
            })?,
        )?;
        globals.raw_set(
            "require",
            lua.create_require_function(Requirer::new(scripts))?,
        )?;
        Ok(api)
    }

    /// The emitter table of the mod event `event` describes: defined with
    /// the loader once `main.luau` ran at start, or the one defined before
    /// under its name.
    fn define_event(&self, lua: &Lua, event: &Table) -> mlua::Result<Table> {
        let mut desc = EventDesc::parse_definition(event)?;
        if !desc.event.name.contains(':') {
            let full = format!("{}:{}", self.id, desc.event.name);
            if !enshwick_core::content::is_id(&full) {
                return Err(mlua::Error::runtime(format!(
                    "enshwick.define_event: {:?} is no local name, 1 to 128 of [a-z0-9_./-]",
                    desc.event.name
                )));
            }
            desc.event.name = full;
        }
        let emitter = {
            let mut defined = self.defined.relock();
            match defined
                .iter()
                .find(|d| d.desc.event.name == desc.event.name)
            {
                Some(known) if known.desc != desc => {
                    return Err(mlua::Error::runtime(format!(
                        "enshwick.define_event: {} changed its fields; the change takes the next start",
                        desc.event.name
                    )));
                }
                Some(known) => Arc::clone(&known.emitter),
                None if self.started.load(Ordering::Acquire) => {
                    return Err(mlua::Error::runtime(format!(
                        "enshwick.define_event: {} is defined from the next start",
                        desc.event.name
                    )));
                }
                None => {
                    let emitter = Arc::new(OnceLock::new());
                    defined.push(Defined {
                        desc: desc.clone(),
                        emitter: Arc::clone(&emitter),
                    });
                    emitter
                }
            }
        };
        let read = Read::Struct(desc.fields);
        let size = usize::try_from(desc.event.size).unwrap_or(0);
        let table = lua.create_table()?;
        table.raw_set(
            "send",
            lua.create_function(move |_, data: LuaValue| {
                let mut bytes = vec![0u8; size];
                encode(&data, &read, &mut bytes, 0)?;
                if let Some(emitter) = emitter.get() {
                    emitter.send(&bytes);
                }
                Ok(())
            })?,
        )?;
        table.set_readonly(true);
        Ok(table)
    }

    /// The table of the point `pack.toml` declares under the local name
    /// `name`: defined with the loader once `main.luau` ran at start, or the
    /// one defined before under that name. Its `decide(input, answer)`
    /// returns the answer and `"kept"` or `"changed"`, or nil and
    /// `"refused"`, as `OwnedPoint::decide` gives a `Decision`.
    fn define_point(&self, lua: &Lua, name: &str) -> mlua::Result<Table> {
        let what = "enshwick.points.define_point";
        let desc = self
            .declared_points
            .iter()
            .find(|d| d.local == name)
            .cloned()
            .ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "{what}: pack.toml declares no [[point]] named {name:?}"
                ))
            })?;
        let handle = {
            let mut owned = self.owned.relock();
            match owned.iter().find(|o| o.desc.local == name) {
                Some(known) => Arc::clone(&known.handle),
                None if self.started.load(Ordering::Acquire) => {
                    return Err(mlua::Error::runtime(format!(
                        "{what}: {} is defined from the next start, at the top level of main.luau",
                        desc.point.name
                    )));
                }
                None => {
                    let handle = Arc::new(OnceLock::new());
                    owned.push(Owned {
                        desc: Arc::clone(&desc),
                        handle: Arc::clone(&handle),
                    });
                    handle
                }
            }
        };
        let words = ["kept", "changed", "refused"].map(|word| lua.create_string(word));
        let [kept, changed, refused] = words;
        let (kept, changed, refused) = (kept?, changed?, refused?);
        let size = usize::try_from(desc.point.input_size).unwrap_or(0);
        let answer_size = usize::try_from(desc.point.output_size).unwrap_or(0);
        let asked = Arc::clone(&handle);
        let table = lua.create_table_with_capacity(0, 2)?;
        table.raw_set(
            "decide",
            lua.create_function(
                move |lua, (_, input, answer): (LuaValue, LuaValue, LuaValue)| {
                    let mut small = Aligned([0u8; SMALL_INPUT]);
                    let mut large = Vec::new();
                    let bytes = if let Some(bytes) = small.0.get_mut(..size) {
                        bytes
                    } else {
                        large.resize(size, 0u8);
                        large.as_mut_slice()
                    };
                    encode(&input, &desc.input, bytes, 0)
                        .map_err(|e| mlua::Error::runtime(format!("{what}: the input: {e}")))?;
                    let mut out = Aligned([0u8; 16]);
                    let out = out.0.get_mut(..answer_size).unwrap_or_default();
                    encode(&answer, &desc.output, out, 0)
                        .map_err(|e| mlua::Error::runtime(format!("{what}: the answer: {e}")))?;
                    let Some(point) = asked.get() else {
                        return Ok((answer, kept.clone()));
                    };
                    Ok(match point.decide(bytes, out) {
                        Decision::Changed(()) => {
                            (decode(lua, out, 0, &desc.output)?, changed.clone())
                        }
                        Decision::Refused => (LuaValue::Nil, refused.clone()),
                        _ => (answer, kept.clone()),
                    })
                },
            )?,
        )?;
        table.raw_set(
            "is_extended",
            lua.create_function(move |_, _: LuaValue| {
                Ok(handle.get().is_some_and(DescribedOwnedPoint::is_extended))
            })?,
        )?;
        table.set_readonly(true);
        Ok(table)
    }

    /// The function that runs the call `call` describes: asked for with the
    /// loader once `main.luau` ran at start, or the one asked for before
    /// under its name and major.
    fn caller(&self, lua: &Lua, call: &Table) -> mlua::Result<Function> {
        let desc = CallDesc::parse(call)?;
        let caller = {
            let mut asked = self.asked.relock();
            let name = &desc.call.name;
            let major = desc.call.major;
            match asked
                .iter()
                .find(|a| a.desc.call.name == *name && a.desc.call.major == major)
            {
                Some(known) if known.desc != desc => {
                    return Err(mlua::Error::runtime(format!(
                        "enshwick.caller: {name} v{major} is described differently than at the start"
                    )));
                }
                Some(known) => Arc::clone(&known.caller),
                None if self.started.load(Ordering::Acquire) => {
                    return Err(mlua::Error::runtime(format!(
                        "enshwick.caller: {name} v{major} is asked for from the next start"
                    )));
                }
                None => {
                    let caller = Arc::new(OnceLock::new());
                    asked.push(Asked {
                        desc: desc.clone(),
                        caller: Arc::clone(&caller),
                    });
                    caller
                }
            }
        };
        desc.function(lua, caller)
    }
}

/// The table of the namespace `name` in `api`, made on first use: parts of
/// one name share it.
fn namespace_table(lua: &Lua, api: &Table, name: &str) -> mlua::Result<Table> {
    if name.is_empty() {
        return Ok(api.clone());
    }
    if let Some(table) = api.raw_get::<Option<Table>>(name)? {
        return Ok(table);
    }
    let table = lua.create_table()?;
    api.raw_set(name, table.clone())?;
    Ok(table)
}

/// Follows the mod's settings as `Load::on_settings` and
/// `Load::on_settings_change` do: the handler gets the values in force now,
/// as each world loads and stops, and, for the second, as they change.
const FOLLOW_SETTINGS: &str = r#"
local api, live = ...
return function(handler)
    handler(api.settings())
    api.on("SettingsReplaced", function() handler(api.settings()) end)
    if live then
        api.on("SettingsChanged", function() handler(api.settings()) end)
    end
end
"#;

/// Puts `on_settings` and `on_settings_change` in `api`, beside `settings`
/// and `on`, which they call.
fn follow_settings(lua: &Lua, api: &Table) -> mlua::Result<()> {
    for (name, live) in [("on_settings", false), ("on_settings_change", true)] {
        let follow: Function = lua
            .load(FOLLOW_SETTINGS)
            .set_name(format!("=enshwick.{name}"))
            .call((api.clone(), live))?;
        api.raw_set(name, follow)?;
    }
    Ok(())
}

impl Inner {
    /// Puts each namespace's [`Load`] functions into `api` for the start:
    /// each runs on `load` through a function of `scope`, which ends with
    /// the start; a call after it raises an error naming the call. Then
    /// makes every namespace table read-only.
    pub(super) fn init_functions<'scope, 'env>(
        &self,
        vm: &Arc<Vm>,
        api: &Table,
        scope: &'scope mlua::Scope<'scope, 'env>,
        load: &'env Mutex<&'env mut Load<'_>>,
    ) -> mlua::Result<()> {
        let lua = &vm.lua;
        let bridge = Scope {
            inner: Weak::clone(&self.me),
            vm: Arc::downgrade(vm),
        };
        for namespace in script::NAMESPACES {
            let table = namespace_table(lua, api, namespace.name())?;
            for function in namespace.init_functions() {
                let what = if namespace.name().is_empty() {
                    format!("enshwick.{}", function.name())
                } else {
                    format!("enshwick.{}.{}", namespace.name(), function.name())
                };
                let signature = function.signature();
                let during = {
                    let (bridge, what) = (bridge.clone(), what.clone());
                    scope.create_function(move |lua, args: MultiValue| {
                        let mut values = arguments(&bridge, args, signature, &what)?;
                        let mut load = load.try_relock().ok_or_else(|| {
                            mlua::Error::runtime(format!(
                                "{what}: called from inside another start call"
                            ))
                        })?;
                        let answer = function
                            .call(&mut load, values.slots())
                            .map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                        to_lua(lua, &bridge, answer, signature.ret())
                    })?
                };
                table.raw_set(function.name(), start_only(vm, what, during)?)?;
            }
            if let Some(getter) = namespace.request_function() {
                requested(vm, &bridge, &table, namespace, getter, scope, load)?;
            }
        }
        for pair in api.pairs::<String, LuaValue>() {
            if let (_, LuaValue::Table(table)) = pair? {
                table.set_readonly(true);
            }
        }
        Ok(())
    }
}

/// `during`, a function of the start's scope, as the function `what` that
/// raises an error once the start is over.
fn start_only(vm: &Arc<Vm>, what: String, during: Function) -> mlua::Result<Function> {
    let weak = Arc::downgrade(vm);
    vm.lua.create_function(move |_, args: MultiValue| {
        if !weak
            .upgrade()
            .is_some_and(|vm| vm.starting.load(Ordering::Acquire))
        {
            return Err(mlua::Error::runtime(format!(
                "{what}: allowed only while the mod starts, at the top level of main.luau"
            )));
        }
        during.call::<MultiValue>(args)
    })
}

/// Puts `namespace`, which a script asks for, into `table`: its `getter`
/// opens it on `load` at start and returns the handle, as a Rust mod's
/// `Load` method does in `init`; its functions raise an error until then.
fn requested<'scope, 'env>(
    vm: &Arc<Vm>,
    bridge: &Scope,
    table: &Table,
    namespace: &'static script::Namespace,
    getter: script::Declared,
    scope: &'scope mlua::Scope<'scope, 'env>,
    load: &'env Mutex<&'env mut Load<'_>>,
) -> mlua::Result<()> {
    let lua = &vm.lua;
    let ask = format!("enshwick.{}.{}", namespace.name(), getter.name());
    let slot: Arc<OnceLock<script::Opened>> = Arc::default();
    for declared in namespace.functions() {
        let (slot, bridge, ask) = (Arc::clone(&slot), bridge.clone(), ask.clone());
        let what = format!("enshwick.{}.{}", namespace.name(), declared.name());
        let signature = declared.signature();
        table.raw_set(
            declared.name(),
            lua.create_function(move |lua, args: LuaArgs| {
                let opened = slot.get().ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "{what}: call {ask}() at the top level of main.luau first"
                    ))
                })?;
                let mut values = arguments(&bridge, args, signature, &what)?;
                let answer = opened
                    .call(&declared, values.slots())
                    .map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                to_lua(lua, &bridge, answer, signature.ret())
            })?,
        )?;
    }
    let during = {
        let (bridge, what) = (bridge.clone(), ask.clone());
        scope.create_function(move |lua, ()| {
            let mut load = load.try_relock().ok_or_else(|| {
                mlua::Error::runtime(format!("{what}: called from inside another start call"))
            })?;
            let (opened, handle) = namespace
                .request(&mut load)
                .ok_or_else(|| mlua::Error::runtime(format!("{what}: no such function")))?
                .map_err(|off| mlua::Error::runtime(format!("{what}: {off}")))?;
            let _first = slot.set(opened);
            to_lua(lua, &bridge, handle, getter.signature().ret())
        })?
    };
    table.raw_set(getter.name(), start_only(vm, ask, during)?)
}

/// The input size a script's `decide` lays out on the stack; a larger one
/// goes to the heap.
const SMALL_INPUT: usize = 64;

/// Bytes aligned for any field a Rust link reads, so `decide` hands them to
/// the links without a copy.
#[repr(C, align(16))]
struct Aligned<const N: usize>([u8; N]);
