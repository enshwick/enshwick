//! Rust handles as script objects: their methods, looked up in the VM,
//! and the one object a hot filter is lent on every call.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use enshwick_core::script::{Class, Error, Handle, Value};
use mlua::{Lua, Value as LuaValue};

use super::{Interned, LuaArgs, call, expected, raising, refused};
use crate::run::Scope;

/// The API value of `value`, an object of `class`.
pub(super) fn object_from_lua(value: &LuaValue, class: &'static Class) -> Result<Value, Error> {
    let LuaValue::UserData(object) = value else {
        return Err(expected(class.name(), value));
    };
    match object.borrow::<Object>() {
        // A class is a `const`, whose address differs between uses; the
        // name picks it, and `FromScript` checks the Rust type.
        Ok(object) if !object.ended() && object.handle.class().name() == class.name() => {
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
    /// Whether the call that held the kept object ended, which the object
    /// reads without a borrow of its own.
    ended: Arc<AtomicBool>,
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
            let object = Object::create(lua, handle, scope, None)?;
            return Ok((LuaValue::UserData(object), false));
        }
        let kept = match self.object.get() {
            Some(kept) => kept.borrow_mut::<Object>().and_then(|mut object| {
                let moved = object.handle.class().name() != handle.class().name();
                object.handle = handle;
                drop(object);
                // Its next lookup takes the new class's methods.
                if moved {
                    Interned::objects(lua)?.raw_set(kept, LuaValue::Nil)?;
                }
                Ok(kept.clone())
            }),
            None => Object::create(lua, handle, scope, Some(Arc::clone(&self.ended)))
                .map(|made| self.object.get_or_init(|| made).clone()),
        };
        self.ended.store(false, Ordering::Release);
        kept.map(|kept| (LuaValue::UserData(kept), true))
            .inspect_err(|_| self.give_back())
    }

    /// Ends the call that holds the kept object, which refuses use until the
    /// next call lends it again.
    pub(crate) fn give_back(&self) {
        self.ended.store(true, Ordering::Release);
        self.busy.store(false, Ordering::Release);
    }
}

/// An [`Object`](enshwick_core::script::Object) as a script holds it: its
/// class's methods, called as `object:method()`.
pub(super) struct Object {
    handle: Handle,
    scope: Scope,
    /// A kept [`Lent`] object's flag, set between its calls: a script that
    /// kept it gets an error, never the next call's value.
    ended: Option<Arc<AtomicBool>>,
}

/// An object's `__index`, run by the VM: its class's method from
/// [`Interned::objects`], else `find`, which makes it. A filter on a hot path
/// indexes a method on every hit, and a lookup through a call into Rust
/// costs about three times this one. An object enters the table on its first
/// lookup, set here: from Rust under a memory limit a set leaves a closure
/// behind, and an object no script indexes costs nothing.
const INDEX: &str = "
    local objects, class, find = ...
    return function(self, key)
        local methods = objects[self]
        if methods == nil then
            methods = class(self)
            objects[self] = methods
        end
        local method = methods[key]
        if method ~= nil then
            return method
        end
        return find(self, key)
    end
";

impl Object {
    /// A new script object holding `handle`, its methods found by
    /// [`INDEX`]; `ended` is a kept [`Lent`] object's flag.
    pub(super) fn create(
        lua: &Lua,
        handle: Handle,
        scope: &Scope,
        ended: Option<Arc<AtomicBool>>,
    ) -> mlua::Result<mlua::AnyUserData> {
        lua.create_userdata(Self {
            handle,
            scope: scope.clone(),
            ended,
        })
    }

    /// Whether it is a kept [`Lent`] object between its calls.
    fn ended(&self) -> bool {
        self.ended
            .as_ref()
            .is_some_and(|e| e.load(Ordering::Acquire))
    }

    /// The method `key` of its class, made once per class and VM and kept in
    /// [`Interned::methods`]; nil for a name its class lacks.
    fn method(&self, lua: &Lua, key: &mlua::LuaString) -> mlua::Result<LuaValue> {
        let class = self.handle.class();
        let cached = Interned::methods(lua, class.name())?;
        if let Some(function) = cached.raw_get::<Option<mlua::Function>>(key)? {
            return Ok(LuaValue::Function(function));
        }
        let key = key.to_str()?.to_owned();
        let Some(method) = class.methods().into_iter().find(|m| m.name() == key) else {
            return Ok(LuaValue::Nil);
        };
        let scope = self.scope.clone();
        let what = format!("{}:{key}", class.name());
        let function = lua.create_function(
            move |lua, (this, args): (mlua::UserDataRef<Self>, LuaArgs)| {
                if this.ended() {
                    let ended = Error::new("the object was lent to a call that ended");
                    return refused(lua, &what, &ended);
                }
                // Not borrowed through the call, which may lend this object again.
                let handle = this.handle.clone();
                drop(this);
                // A method indexes its own class's list: `a.m(b)` must not
                // run b's method at m's place.
                if handle.class().name() != class.name() {
                    let other = Error::new(format!("called on a {}", handle.class().name()));
                    return refused(lua, &what, &other);
                }
                call(lua, &scope, args, method.signature(), &what, |slots| {
                    handle.call(&method, slots)
                })
            },
        )?;
        let function = raising(lua, function)?;
        cached.raw_set(key, &function)?;
        Ok(LuaValue::Function(function))
    }
}

impl mlua::UserData for Object {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field_with(mlua::MetaMethod::Index, |lua| {
            let class = lua.create_function(|lua, this: mlua::UserDataRef<Self>| {
                Interned::methods(lua, this.handle.class().name())
            })?;
            let find = lua.create_function(
                |lua, (this, key): (mlua::UserDataRef<Self>, mlua::LuaString)| {
                    this.method(lua, &key)
                },
            )?;
            lua.load(INDEX)
                .set_name("=enshwick.object")
                .set_mode(mlua::chunk::ChunkMode::Text)
                .call::<mlua::Function>((Interned::objects(lua)?, class, find))
        });
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(this.handle.class().name())
        });
    }
}

#[cfg(test)]
mod tests {
    use enshwick::script::IntoScript;

    use super::*;

    #[derive(Clone, Debug)]
    pub(crate) struct Counter(u32);

    #[enshwick::script(counters)]
    impl Counter {
        /// The count.
        #[script]
        pub(crate) fn get(&self) -> u32 {
            self.0
        }
    }

    impl enshwick::script::Object for Counter {
        const CLASS: &'static Class = &Class::of::<Self>("Counter", "A count.");
        const PARTS: &'static [&'static [enshwick::script::Function<Self>]] =
            &[Self::SCRIPT_COUNTERS];
    }

    #[derive(Clone, Debug)]
    pub(crate) struct Twice(u32);

    #[enshwick::script(twice)]
    impl Twice {
        /// The count, doubled.
        #[script]
        pub(crate) fn get(&self) -> u32 {
            self.0 * 2
        }
    }

    impl enshwick::script::Object for Twice {
        const CLASS: &'static Class = &Class::of::<Self>("Twice", "A doubled count.");
        const PARTS: &'static [&'static [enshwick::script::Function<Self>]] = &[Self::SCRIPT_TWICE];
    }

    fn handle(value: impl IntoScript) -> Handle {
        let Value::Object(handle) = value.into_value() else {
            panic!("no object");
        };
        handle
    }

    fn counter(lua: &Lua, n: u32) -> mlua::AnyUserData {
        Object::create(lua, handle(Counter(n)), &Scope::detached(), None).unwrap()
    }

    fn tracked(lua: &Lua) -> usize {
        let objects = Interned::objects(lua).unwrap();
        objects.pairs::<LuaValue, LuaValue>().count()
    }

    #[test]
    fn objects_of_a_class_share_its_methods_and_read_nil_for_a_name_it_lacks() {
        let lua = Lua::new();
        let globals = lua.globals();
        globals.set("a", counter(&lua, 2)).unwrap();
        globals.set("b", counter(&lua, 5)).unwrap();
        let (shared, missing, a, b): (bool, bool, u32, u32) = lua
            .load("return a.get == b.get, a.missing == nil, a:get(), b:get()")
            .eval()
            .unwrap();
        assert_eq!((shared, missing, a, b), (true, true, 2, 5));
    }

    #[test]
    fn an_object_enters_the_table_of_objects_only_when_a_script_indexes_it() {
        let lua = Lua::new();
        lua.globals().set("a", counter(&lua, 1)).unwrap();
        assert_eq!(tracked(&lua), 0);
        lua.load("local _ = a.get").exec().unwrap();
        assert_eq!(tracked(&lua), 1);
    }

    #[test]
    fn a_collected_object_leaves_the_table_of_objects() {
        let lua = Lua::new();
        lua.globals().set("a", counter(&lua, 1)).unwrap();
        lua.load("local _ = a:get(); a = nil").exec().unwrap();
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        assert_eq!(tracked(&lua), 0);
    }

    #[test]
    fn a_lent_object_given_another_class_calls_that_class_s_methods() {
        let lua = Lua::new();
        let lent = Lent::default();
        let scope = Scope::detached();
        let get = |lua: &Lua, object: LuaValue| -> u32 {
            lua.globals().set("o", object).unwrap();
            lua.load("return o:get()").eval().unwrap()
        };
        let (object, kept) = lent.lend(&lua, &scope, handle(Counter(3))).unwrap();
        assert!(kept);
        assert_eq!(get(&lua, object), 3);
        lent.give_back();
        let (object, _) = lent.lend(&lua, &scope, handle(Twice(3))).unwrap();
        assert_eq!(get(&lua, object), 6);
        lent.give_back();
    }
}
