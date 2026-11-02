//! The `enshwick` table a mod's scripts see: the game namespaces, settings,
//! dependencies, and the subscriptions, events and calls they set up at start.

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock, Weak};

use enshwick::Relock as _;
use enshwick::abi::Level;
use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};

use super::{Asked, Defined, Inner, Scope, Vm};
use crate::bridge::{arguments, to_lua};
use crate::calls::CallDesc;
use crate::files::{Requirer, Scripts};
use crate::points::{EventDesc, PointDesc, Read, encode};
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
                lua.create_function(move |_, ()| Ok(enshwick::mods::is_running(&id)))?,
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

    /// Puts `extend`, `refuse`, `on_game_type`, `game_event_types` and `on`
    /// into `api`, the subscriptions `vm`'s scripts make at start.
    fn subscription_functions(&self, vm: &Arc<Vm>, api: &Table) -> mlua::Result<()> {
        let lua = &vm.lua;
        let refuse = lua.create_table()?;
        refuse.set_readonly(true);
        let _first = vm.refuse.set(refuse.clone());
        api.raw_set("refuse", refuse)?;
        let weak = Arc::downgrade(vm);
        api.raw_set(
            "extend",
            lua.create_function(move |_, (point, handler): (Table, Function)| {
                let vm = weak
                    .upgrade()
                    .ok_or_else(|| mlua::Error::runtime("the mod has stopped"))?;
                let desc = PointDesc::parse(&point)?;
                let sub = Subscription::Point(desc.point.name.clone());
                {
                    let mut points = vm.points.relock();
                    if !points.iter().any(|d| d.point.name == desc.point.name) {
                        points.push(desc);
                    }
                }
                vm.subscribe("enshwick.extend", sub, handler)
            })?,
        )?;
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
            let table = lua.create_table()?;
            for declared in namespace.functions() {
                let scope = scope.clone();
                let opened = opened.clone();
                let what = format!("enshwick.{}.{}", namespace.name(), declared.name());
                let signature = declared.signature();
                table.raw_set(
                    declared.name(),
                    lua.create_function(move |lua, args: MultiValue| {
                        let values = arguments(lua, &scope, args, signature, &what)?;
                        let answer = opened
                            .call(&declared, values)
                            .map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                        to_lua(lua, &scope, answer, signature.ret())
                    })?,
                )?;
            }
            table.set_readonly(true);
            api.raw_set(namespace.name(), table)?;
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
    /// the loader once `main.luau` ran at start, or the one defined then
    /// for a reload.
    fn define_event(&self, lua: &Lua, event: &Table) -> mlua::Result<Table> {
        let mut desc = EventDesc::parse_definition(event)?;
        if !desc.event.name.contains(':') {
            let full = format!("{}:{}", self.id, desc.event.name);
            if !enshwick::content::is_id(&full) {
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

    /// The function that runs the call `call` describes: asked for with the
    /// loader once `main.luau` ran at start, or the one asked for then for
    /// a reload.
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
