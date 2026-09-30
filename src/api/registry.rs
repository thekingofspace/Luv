use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use mlua::{
    AnyUserData, Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataFields, UserDataMethods, Value,
};

use crate::objects::Signal;
use crate::runtime::{CurrentThread, Engine, Message, Packet, decode_value, encode_value};

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn check_id(id: &str) -> Result<()> {
    if id.is_empty() || id.starts_with('.') || id.ends_with('.') || id.contains("..") {
        return Err(runtime(format!(
            "'{id}' is not a registry id, use names split by dots like \"items.sword\""
        )));
    }
    Ok(())
}

fn under(id: &str, prefix: &str) -> bool {
    prefix.is_empty() || id == prefix || (id.starts_with(prefix) && id.as_bytes().get(prefix.len()) == Some(&b'.'))
}

fn freeze(value: &Value, seen: &mut Vec<*const c_void>) -> Result<()> {
    let Value::Table(table) = value else {
        return Ok(());
    };
    let pointer = table.to_pointer();
    if seen.contains(&pointer) {
        return Ok(());
    }
    seen.push(pointer);
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        freeze(&key, seen)?;
        freeze(&value, seen)?;
    }
    table.set_readonly(true);
    Ok(())
}

fn frozen(value: Value) -> Result<Value> {
    freeze(&value, &mut Vec::new())?;
    Ok(value)
}

fn prefix_range<'a, V>(entries: &'a BTreeMap<String, V>, prefix: &'a str) -> impl Iterator<Item = (&'a String, &'a V)> {
    entries
        .range(prefix.to_owned()..)
        .take_while(move |(id, _)| prefix.is_empty() || id.starts_with(prefix))
        .filter(move |(id, _)| under(id, prefix))
}

pub struct LocalRegistry {
    name: String,
    entries: RefCell<BTreeMap<String, Value>>,
    staged: RefCell<Vec<(String, Value)>>,
    changed: AnyUserData,
}

impl LocalRegistry {
    fn apply(&self, lua: &Lua, id: String, value: Value) -> Result<()> {
        let value = frozen(value)?;
        match &value {
            Value::Nil => {
                self.entries.borrow_mut().remove(&id);
            }
            value => {
                self.entries.borrow_mut().insert(id.clone(), value.clone());
            }
        }
        Signal::fire(lua, &self.changed, MultiValue::from_vec(vec![Value::String(lua.create_string(&id)?), value]))
    }
}

pub struct SafeStore {
    entries: RwLock<BTreeMap<String, (u64, Packet)>>,
    version: AtomicU64,
}

#[derive(Default)]
pub struct SafeRegistries {
    stores: Mutex<HashMap<String, Arc<SafeStore>>>,
}

impl SafeRegistries {
    fn store(&self, name: &str) -> Arc<SafeStore> {
        self.stores
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(name.to_owned())
            .or_insert_with(|| {
                Arc::new(SafeStore {
                    entries: RwLock::new(BTreeMap::new()),
                    version: AtomicU64::new(0),
                })
            })
            .clone()
    }
}

pub struct SafeRegistry {
    name: Arc<str>,
    store: Arc<SafeStore>,
    cache: RefCell<HashMap<String, (u64, Value)>>,
    staged: RefCell<Vec<(String, Option<Packet>)>>,
    changed: AnyUserData,
}

#[derive(Default)]
struct OpenRegistries {
    local: RefCell<HashMap<String, AnyUserData>>,
    safe: RefCell<HashMap<String, AnyUserData>>,
}

fn open(lua: &Lua) -> Rc<OpenRegistries> {
    if let Some(open) = lua.app_data_ref::<Rc<OpenRegistries>>() {
        return open.clone();
    }
    let open = Rc::new(OpenRegistries::default());
    lua.set_app_data(open.clone());
    open
}

fn engine(lua: &Lua) -> Result<Arc<Engine>> {
    lua.app_data_ref::<Arc<Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| runtime("the luv engine is not running"))
}

impl SafeRegistry {
    fn read(&self, lua: &Lua, id: &str) -> Result<Value> {
        let found = self
            .store
            .entries
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned();
        let Some((version, packet)) = found else {
            self.cache.borrow_mut().remove(id);
            return Ok(Value::Nil);
        };
        if let Some((cached, value)) = self.cache.borrow().get(id)
            && *cached == version
        {
            return Ok(value.clone());
        }
        let value = frozen(decode_value(lua, &packet)?)?;
        self.cache.borrow_mut().insert(id.to_owned(), (version, value.clone()));
        Ok(value)
    }

    fn encode(lua: &Lua, id: &str, value: Value) -> Result<Option<Packet>> {
        match value {
            Value::Nil => Ok(None),
            value => encode_value(lua, value)
                .map(Some)
                .map_err(|error| runtime(format!("Registry.Safe cannot keep '{id}': {error}"))),
        }
    }

    fn commit(&self, lua: &Lua, changes: Vec<(String, Option<Packet>)>) -> Result<()> {
        {
            let mut entries = self.store.entries.write().unwrap_or_else(PoisonError::into_inner);
            for (id, packet) in &changes {
                match packet {
                    Some(packet) => {
                        let version = self.store.version.fetch_add(1, Ordering::Relaxed) + 1;
                        entries.insert(id.clone(), (version, packet.clone()));
                    }
                    None => {
                        entries.remove(id);
                    }
                }
            }
        }
        let engine = engine(lua)?;
        let sender = lua.app_data_ref::<CurrentThread>().map(|thread| thread.0);
        for (id, _) in changes {
            let value = self.read(lua, &id)?;
            Signal::fire(
                lua,
                &self.changed,
                MultiValue::from_vec(vec![Value::String(lua.create_string(&id)?), value]),
            )?;
            engine.bus().broadcast_except(sender, || Message::Registry {
                name: self.name.clone(),
                id: id.as_str().into(),
            });
        }
        Ok(())
    }
}

pub fn changed_elsewhere(lua: &Lua, name: &str, id: &str) -> Result<()> {
    let Some(registry) = open(lua).safe.borrow().get(name).cloned() else {
        return Ok(());
    };
    let (value, changed) = {
        let this = registry.borrow::<SafeRegistry>()?;
        (this.read(lua, id)?, this.changed.clone())
    };
    Signal::fire(lua, &changed, MultiValue::from_vec(vec![Value::String(lua.create_string(id)?), value]))
}

fn list(lua: &Lua, ids: impl Iterator<Item = String>) -> Result<Table> {
    lua.create_sequence_from(ids)
}

impl UserData for LocalRegistry {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "Registry");
        fields.add_field_method_get("ClassName", |_, _| Ok("Registry"));
        fields.add_field_method_get("Name", |_, this| Ok(this.name.clone()));
        fields.add_field_method_get("IsSafe", |_, _| Ok(false));
        fields.add_field_method_get("Count", |_, this| Ok(this.entries.borrow().len()));
        fields.add_field_method_get("Staged", |_, this| Ok(this.staged.borrow().len()));
        fields.add_field_method_get("Changed", |_, this| Ok(this.changed.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(format!("Registry({})", this.name)));
        methods.add_method("Register", |lua, this, (id, value): (String, Value)| {
            check_id(&id)?;
            this.apply(lua, id, value)
        });
        methods.add_method("Stage", |_, this, (id, value): (String, Value)| {
            check_id(&id)?;
            this.staged.borrow_mut().push((id, value));
            Ok(())
        });
        methods.add_method("Commit", |lua, this, ()| {
            let staged = std::mem::take(&mut *this.staged.borrow_mut());
            let count = staged.len();
            for (id, value) in staged {
                this.apply(lua, id, value)?;
            }
            Ok(count)
        });
        methods.add_method("Discard", |_, this, ()| {
            let count = this.staged.borrow().len();
            this.staged.borrow_mut().clear();
            Ok(count)
        });
        methods.add_method("Get", |_, this, id: String| {
            Ok(this.entries.borrow().get(&id).cloned().unwrap_or(Value::Nil))
        });
        methods.add_method("Has", |_, this, id: String| Ok(this.entries.borrow().contains_key(&id)));
        methods.add_method("Remove", |lua, this, id: String| {
            let had = this.entries.borrow().contains_key(&id);
            if had {
                this.apply(lua, id, Value::Nil)?;
            }
            Ok(had)
        });
        methods.add_method("List", |lua, this, prefix: Option<String>| {
            let prefix = prefix.unwrap_or_default();
            let entries = this.entries.borrow();
            list(lua, prefix_range(&entries, &prefix).map(|(id, _)| id.clone()))
        });
        methods.add_method("GetAll", |lua, this, prefix: Option<String>| {
            let prefix = prefix.unwrap_or_default();
            let entries = this.entries.borrow();
            let table = lua.create_table()?;
            for (id, value) in prefix_range(&entries, &prefix) {
                table.raw_set(id.as_str(), value.clone())?;
            }
            Ok(table)
        });
        methods.add_method("Clear", |lua, this, ()| {
            let ids: Vec<String> = this.entries.borrow().keys().cloned().collect();
            for id in &ids {
                this.apply(lua, id.clone(), Value::Nil)?;
            }
            Ok(ids.len())
        });
    }
}

impl UserData for SafeRegistry {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "Registry");
        fields.add_field_method_get("ClassName", |_, _| Ok("Registry"));
        fields.add_field_method_get("Name", |_, this| Ok(this.name.to_string()));
        fields.add_field_method_get("IsSafe", |_, _| Ok(true));
        fields.add_field_method_get("Count", |_, this| {
            Ok(this.store.entries.read().unwrap_or_else(PoisonError::into_inner).len())
        });
        fields.add_field_method_get("Staged", |_, this| Ok(this.staged.borrow().len()));
        fields.add_field_method_get("Changed", |_, this| Ok(this.changed.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Registry.Safe({})", this.name))
        });
        methods.add_method("Register", |lua, this, (id, value): (String, Value)| {
            check_id(&id)?;
            let packet = SafeRegistry::encode(lua, &id, value)?;
            this.commit(lua, vec![(id, packet)])
        });
        methods.add_method("Stage", |lua, this, (id, value): (String, Value)| {
            check_id(&id)?;
            let packet = SafeRegistry::encode(lua, &id, value)?;
            this.staged.borrow_mut().push((id, packet));
            Ok(())
        });
        methods.add_method("Commit", |lua, this, ()| {
            let staged = std::mem::take(&mut *this.staged.borrow_mut());
            let count = staged.len();
            this.commit(lua, staged)?;
            Ok(count)
        });
        methods.add_method("Discard", |_, this, ()| {
            let count = this.staged.borrow().len();
            this.staged.borrow_mut().clear();
            Ok(count)
        });
        methods.add_method("Get", |lua, this, id: String| this.read(lua, &id));
        methods.add_method("Has", |_, this, id: String| {
            Ok(this.store.entries.read().unwrap_or_else(PoisonError::into_inner).contains_key(&id))
        });
        methods.add_method("Remove", |lua, this, id: String| {
            let had = this
                .store
                .entries
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .contains_key(&id);
            if had {
                this.commit(lua, vec![(id, None)])?;
            }
            Ok(had)
        });
        methods.add_method("List", |lua, this, prefix: Option<String>| {
            let prefix = prefix.unwrap_or_default();
            let ids: Vec<String> = {
                let entries = this.store.entries.read().unwrap_or_else(PoisonError::into_inner);
                prefix_range(&entries, &prefix).map(|(id, _)| id.clone()).collect()
            };
            list(lua, ids.into_iter())
        });
        methods.add_method("GetAll", |lua, this, prefix: Option<String>| {
            let prefix = prefix.unwrap_or_default();
            let ids: Vec<String> = {
                let entries = this.store.entries.read().unwrap_or_else(PoisonError::into_inner);
                prefix_range(&entries, &prefix).map(|(id, _)| id.clone()).collect()
            };
            let table = lua.create_table()?;
            for id in ids {
                let value = this.read(lua, &id)?;
                table.raw_set(id, value)?;
            }
            Ok(table)
        });
        methods.add_method("Clear", |lua, this, ()| {
            let ids: Vec<String> = this
                .store
                .entries
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .keys()
                .cloned()
                .collect();
            let count = ids.len();
            this.commit(lua, ids.into_iter().map(|id| (id, None)).collect())?;
            Ok(count)
        });
    }
}

fn check_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(runtime("a registry needs a name"));
    }
    Ok(())
}

pub fn create(lua: &Lua) -> Result<Table> {
    let library = lua.create_table()?;
    library.set(
        "new",
        lua.create_function(|lua, name: String| {
            check_name(&name)?;
            let open = open(lua);
            if let Some(existing) = open.local.borrow().get(&name) {
                return Ok(existing.clone());
            }
            let registry = lua.create_userdata(LocalRegistry {
                name: name.clone(),
                entries: RefCell::new(BTreeMap::new()),
                staged: RefCell::new(Vec::new()),
                changed: lua.create_userdata(Signal::named("Changed"))?,
            })?;
            open.local.borrow_mut().insert(name, registry.clone());
            Ok(registry)
        })?,
    )?;
    library.set(
        "Safe",
        lua.create_function(|lua, name: String| {
            check_name(&name)?;
            let open = open(lua);
            if let Some(existing) = open.safe.borrow().get(&name) {
                return Ok(existing.clone());
            }
            let store = engine(lua)?.registries().store(&name);
            let registry = lua.create_userdata(SafeRegistry {
                name: name.as_str().into(),
                store,
                cache: RefCell::new(HashMap::new()),
                staged: RefCell::new(Vec::new()),
                changed: lua.create_userdata(Signal::named("Changed"))?,
            })?;
            open.safe.borrow_mut().insert(name, registry.clone());
            Ok(registry)
        })?,
    )?;
    library.set_readonly(true);
    Ok(library)
}
