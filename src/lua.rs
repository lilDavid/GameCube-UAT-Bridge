use std::{cell::RefCell, collections::HashMap, fs, io, ops::Deref, path::Path, rc::Rc};

use json::JsonValue;
use mlua::{FromLua, IntoLua, Lua, Table};

use crate::connection::GameCubeConnection;


const GCN_BASE_ADDRESS: u32 = 0x80000000;


/// Coerce a value to true or false, following Lua semantics:
/// null, nil, and false are false, and anything else is true.
fn coerce_boolean(val: &mlua::Value) -> bool {
    if val.is_null() {
        false
    } else if val.is_nil() {
        false
    } else if let Some(b) = val.as_boolean() {
        b
    } else {
        true
    }
}

fn convert_lua_to_string(lua: &Lua, string: &mlua::Value) -> mlua::Result<String> {
    Ok(lua.coerce_string(string.clone())?
        .ok_or(mlua::Error::FromLuaConversionError {
            from: string.type_name(),
            to: "JsonValue".into(),
            message: Some("Key could not be converted to a string".into())
        })?.to_str()?.deref().to_owned()
    )
}

/// Convert a Lua value into a JSON value
fn convert_lua_to_json(lua: &Lua, value: &mlua::Value) -> mlua::Result<JsonValue> {
    if value.is_nil() {
        Ok(JsonValue::Null)
    } else if let Some(b) = value.as_boolean() {
        Ok(JsonValue::from(b))
    } else if let Some(i) = value.as_integer() {
        Ok(JsonValue::from(i))
    } else if let Some(n) = value.as_number() {
        Ok(JsonValue::from(n))
    } else if let Some(s) = value.as_str() {
        Ok(JsonValue::from(s.deref()))
    } else if let Some(table) = value.as_table() {
        let int_keys = table.pairs()
            .map(|result: mlua::Result<(mlua::Value, mlua::Value)>| result.map(|(k, _)| k.as_integer()))
            .collect::<mlua::Result<Option<Vec<_>>>>()?;
        let array_keys = if let Some(mut keys) = int_keys {
            keys.sort();
            let mut iterator = keys.into_iter();
            if let Some(start) = match iterator.next() {
                None => return Ok(JsonValue::new_array()),
                Some(0) => Some(0),
                Some(1) => Some(1),
                Some(_) => None,
            } {
                iterator.try_fold(start, |prev, next| if next == prev + 1 { Some(next) } else { None }).map(|end| start..end)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(keys) = array_keys {
            Ok(JsonValue::Array(
                keys.map(|i|
                    table.get(i)
                        .and_then(|v: mlua::Value| convert_lua_to_json(lua, &v)))
                        .collect::<mlua::Result<Vec<JsonValue>>>()?
            ))
        } else {
            Ok(JsonValue::from(
                table.pairs()
                    .map(|result| result.and_then(|(k, v): (mlua::Value, mlua::Value)|
                        Ok((convert_lua_to_string(lua, &k)?, convert_lua_to_json(lua, &v)?))
                    ))
                    .collect::<mlua::Result<HashMap<String, JsonValue>>>()?
            ))
        }
    } else {
        Err(mlua::Error::FromLuaConversionError { from: value.type_name(), to: "JsonValue".into(), message: Some("Value cannot be represented in JSON".into()) })
    }
}

fn convert_bytes(lua: &Lua, mut bytes: Vec<u8>, ty: &str) -> mlua::Result<mlua::Value> {
    match ty {
        "integer" | "unsigned" => {
            bytes.reverse();
            bytes.extend((bytes.len()..size_of::<mlua::Integer>()).map(|_| 0));
            match bytes[..8].try_into().map(mlua::Integer::from_le_bytes) {  // Result is big endian but the bytes are reversed
                Ok(i) => i.into_lua(lua),
                Err(_) => Ok(mlua::Value::Nil),
            }
        },
        "signed" => {
            bytes.reverse();
            if let Some(msb) = bytes.last().copied() {
                let extension = if (msb as i8) < 0 { u8::MAX } else { 0 };
                bytes.extend((bytes.len()..size_of::<mlua::Integer>()).map(|_| extension));
                match bytes[..8].try_into().map(mlua::Integer::from_le_bytes) {
                    Ok(i) => i.into_lua(lua),
                    Err(_) => Ok(mlua::Value::Nil),
                }
            } else {
                0.into_lua(lua)
            }
        },
        "float" => {
            match bytes.try_into().map(mlua::Number::from_be_bytes) {
                Ok(f) => f.into_lua(lua),
                Err(_) => Ok(mlua::Value::Nil),
            }
        },
        _ => mlua::String::wrap(bytes).into_lua(lua),
    }
}

#[derive(Clone)]
struct VariableStore(Rc<RefCell<Vec<(String, mlua::Result<JsonValue>)>>>);

impl VariableStore {
    fn new(lua: &Lua) -> mlua::Result<(Self, Table)> {
        let table = lua.create_table()?;

        let storage = Rc::new(RefCell::new(vec![]));

        let store = Self(Rc::clone(&storage));

        table.set("WriteVariable", lua.create_function(
            move |lua, (_, key, value,): (mlua::Value, mlua::Value, mlua::Value,)| {
                let key = convert_lua_to_string(lua, &key)?;
                let value = convert_lua_to_json(lua, &value);
                Ok(storage.borrow_mut().push((key, value)))
            }
        )?)?;

        Ok((store, table))
    }

    fn unwrap(self) -> Vec<(String, mlua::Result<JsonValue>)> {
        self.0.borrow().clone()
    }
}

#[derive(Clone)]
pub struct GameInterface(Table);

impl GameInterface {
    fn create_table(lua: &Lua) -> mlua::Result<Table> {
        let table = lua.create_table()?;

        table.set("Name", mlua::Value::Nil)?;
        table.set("Version", mlua::Value::Nil)?;
        table.set("Features", mlua::Value::Nil)?;
        table.set("Slots", mlua::Value::Nil)?;
        table.set("VerifyFunc", mlua::Value::Nil)?;
        table.set("GameWatcher", mlua::Value::Nil)?;

        Ok(table)
    }

    pub fn name(&self) -> mlua::Result<Option<String>> {
        self.0.get("Name")
    }

    pub fn version(&self) -> mlua::Result<Option<String>> {
        self.0.get("Version")
    }

    #[allow(dead_code)]
    pub fn features(&self) -> mlua::Result<Option<Vec<String>>> {
        self.0.get("Features")
    }

    #[allow(dead_code)]
    pub fn slots(&self) -> mlua::Result<Option<Vec<String>>> {
        self.0.get("Slots")
    }

    fn verify(&self) -> mlua::Result<bool> {
        let verify_func: mlua::Value = self.0.get("VerifyFunc")?;
        let verify_func = match verify_func.as_function() {
            Some(f) => f,
            None => return Ok(false),
        };
        Ok(coerce_boolean(&verify_func.call((&self.0,))?))
    }

    fn run_game_watcher(&self, store: &Table) -> mlua::Result<()> {
        let verify_func: mlua::Value = self.0.get("GameWatcher")?;
        let verify_func = match verify_func.as_function() {
            Some(f) => f,
            None => return Ok(()),
        };
        let _: mlua::Value = verify_func.call((&self.0, store))?;
        Ok(())
    }
}

impl FromLua for GameInterface {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        Ok(Self(Table::from_lua(value, lua)?))
    }
}

struct LuaGcnConnection {
    gamecube_connection: Box<dyn GameCubeConnection>,
    game_interface: Option<GameInterface>,
}

impl LuaGcnConnection {
    fn connect(gamecube: Box<dyn GameCubeConnection>, game_interface: Option<GameInterface>) -> Self {
        Self {
            gamecube_connection: gamecube,
            game_interface,
        }
    }
}

pub struct LuaInterface {
    lua: Lua,
    game_interfaces: Rc<RefCell<HashMap<String, GameInterface>>>,
    connection: Rc<RefCell<Option<LuaGcnConnection>>>,
}

impl LuaInterface {
    pub fn new() -> mlua::Result<Self> {
        let lua = Lua::new();
        let connection: Rc<RefCell<Option<LuaGcnConnection>>> = Rc::new(RefCell::new(None));
        let game_interfaces = Rc::new(RefCell::new(HashMap::new()));

        let script_host = lua.create_table()?;
        script_host.set("CreateGameInterface", lua.create_function(
            |lua, (_,): (mlua::Value,)| GameInterface::create_table(lua)
        )?)?;
        let interfaces = Rc::clone(&game_interfaces);
        script_host.set("AddGameInterface", lua.create_function(
            move |_, (_, name, value): (mlua::Value, String, GameInterface)| Ok({ interfaces.borrow_mut().insert(name, value); })
        )?)?;
        lua.globals().set("ScriptHost", script_host)?;

        let gamecube = lua.create_table()?;
        gamecube.set("BaseAddress", GCN_BASE_ADDRESS)?;
        let connect = Rc::clone(&connection);
        gamecube.set("ReadAddress", lua.create_function(
            move |lua, (_, address, size, ty): (mlua::Value, u32, u32, Option<String>)| {
                let mut connection = connect.borrow_mut();
                let connection = connection.as_mut().ok_or(io::Error::from(io::ErrorKind::NotConnected))?;
                let bytes = match connection.gamecube_connection.read_address(size, address) {
                    Err(e) if e.kind() == io::ErrorKind::InvalidData => Ok(None),
                    Err(e) => Err(mlua::Error::from(e)),
                    Ok(bytes) => Ok(Some(bytes)),
                }?;
                let bytes = match bytes {
                    Some(bytes) => bytes,
                    None => return Ok(mlua::Value::Nil),
                };
                convert_bytes(lua, bytes, ty.as_deref().unwrap_or("bytes"))
            }
        )?)?;
        let connect = Rc::clone(&connection);
        gamecube.set("ReadPointerChain", lua.create_function(
            move |lua, (_, address, size, offsets, ty): (mlua::Value, u32, u32, Vec<i32>, Option<String>)| {
                let mut connection = connect.borrow_mut();
                let connection = connection.as_mut().ok_or(io::Error::from(io::ErrorKind::NotConnected))?;
                let bytes = match connection.gamecube_connection.read_pointers(size, address, &offsets) {
                    Err(e) if e.kind() == io::ErrorKind::InvalidData => Ok(None),
                    Err(e) => Err(mlua::Error::from(e)),
                    Ok(bytes) => Ok(Some(bytes)),
                }?;
                let bytes = match bytes {
                    Some(bytes) => bytes,
                    None => return Ok(mlua::Value::Nil),
                };
                convert_bytes(lua, bytes, ty.as_deref().unwrap_or("bytes"))
            }
        )?)?;
        lua.globals().set("GameCube", gamecube)?;

        Ok(Self {
            lua,
            game_interfaces,
            connection,
        })
    }

    pub fn run_script(&self, path: impl AsRef<Path>) -> mlua::Result<()> {
        let data = fs::read(path)?;
        let script = self.lua.load(data);
        script.exec()?;
        Ok(())
    }

    pub fn connect(&self, connection: Box<dyn GameCubeConnection>) -> Result<(String, GameInterface), Box<dyn GameCubeConnection>> {
        self.disconnect();

        self.connection.borrow_mut().replace(LuaGcnConnection::connect(connection, None) );

        let interfaces = self.game_interfaces.borrow();
        let interface = interfaces.iter()
            .filter_map(|(name, interface)| match interface.verify() {
                Ok(true) => Some((name, interface)),
                Ok(false) => None,
                Err(e) => { eprintln!("{}", e); None },
            })
            .next()
            .map(|(k, v)| (k.clone(), v.clone()));

        let mut connection = self.connection.borrow_mut();
        match interface {
            Some((name, interface)) => {
                connection.as_mut().expect("GCN connection was unexpectedly set None").game_interface.replace(interface.clone());
                Ok((name, interface))
            }
            None => {
                let connection = connection.take().expect("GCN connection was unexpectedly set None").gamecube_connection;
                Err(connection)
            }
        }
    }

    pub fn disconnect(&self) {
        self.connection.borrow_mut().take();
    }

    pub fn run_game_watcher(&self) -> Option<mlua::Result<Vec<(String, mlua::Result<JsonValue>)>>> {
        let interface = self.connection.borrow_mut().as_mut().and_then(|connection| connection.game_interface.clone());
        interface.map(|interface| {
            let (store, table) = VariableStore::new(&self.lua)?;
            interface.run_game_watcher(&table)?;
            Ok(store.unwrap())
        })
    }
}
