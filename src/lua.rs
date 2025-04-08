use std::{cell::RefCell, collections::HashMap, error::Error, fmt::Display, fs, io, mem, ops::Deref, path::Path, rc::Rc};

use json::JsonValue;
use mlua::{FromLua, FromLuaMulti, IntoLua, IntoLuaMulti, Lua, Table};

use crate::{connection::{GameCubeConnection, Read}, uat::command::InfoCommand};


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

macro_rules! bytes_to_lua {
    ($type_name:ty, $bytes:ident, $lua:ident) => {{
        assert_eq!($bytes.len(), mem::size_of::<$type_name>());
        match $bytes.try_into().map(<$type_name>::from_be_bytes) {
            Ok(i) => i.into_lua($lua),
            Err(_) => Ok(mlua::Value::Nil),
        }
    }}
}

fn convert_bytes(lua: &Lua, bytes: Option<Vec<u8>>, ty: &TypeSpecifier) -> mlua::Result<mlua::Value> {
    let bytes = match bytes {
        Some(bytes) => bytes,
        None => return Ok(mlua::Value::Nil),
    };
    match ty {
        TypeSpecifier::U8 => bytes_to_lua!(u8, bytes, lua),
        TypeSpecifier::S8 => bytes_to_lua!(i8, bytes, lua),
        TypeSpecifier::U16 => bytes_to_lua!(u16, bytes, lua),
        TypeSpecifier::S16 => bytes_to_lua!(i16, bytes, lua),
        TypeSpecifier::U32 => bytes_to_lua!(u32, bytes, lua),
        TypeSpecifier::S32 => bytes_to_lua!(i32, bytes, lua),
        TypeSpecifier::F32 => bytes_to_lua!(f32, bytes, lua),
        TypeSpecifier::S64 => bytes_to_lua!(i64, bytes, lua),
        TypeSpecifier::F64 => bytes_to_lua!(f64, bytes, lua),
        TypeSpecifier::Bytes(size) => {
            assert_eq!(bytes.len(), *size as usize);
            mlua::String::wrap(bytes).into_lua(lua)
        }
    }
}

#[derive(Debug, Clone)]
enum TypeSpecifier {
    U8,
    S8,
    U16,
    S16,
    U32,
    S32,
    F32,
    S64,
    F64,
    Bytes(u8),
}

impl TypeSpecifier {
    fn size(&self) -> u8 {
        let size = match self {
            Self::U8 | Self::S8 => mem::size_of::<u8>(),
            Self::U16 | Self::S16 => mem::size_of::<u16>(),
            Self::U32 | Self::S32 | Self::F32 => mem::size_of::<u32>(),
            Self::S64 | Self::F64 => mem::size_of::<u64>(),
            Self::Bytes(size) => *size as usize,
        };
        size as u8
    }
}

impl FromLua for TypeSpecifier {
    fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
        match value {
            mlua::Value::String(string) => {
                string.to_str().and_then(|string| match string.deref() {
                    "u8" => Ok(Self::U8),
                    "s8" | "i8" => Ok(Self::S8),
                    "u16" => Ok(Self::U16),
                    "s16" | "i16" => Ok(Self::S16),
                    "u32" => Ok(Self::U32),
                    "s32" | "i32" => Ok(Self::S32),
                    "f32" => Ok(Self::F32),
                    "s64" | "i64" => Ok(Self::S64),
                    "f64" => Ok(Self::F64),
                    _ => Err(mlua::Error::FromLuaConversionError { from: "string", to: "TypeSpecifier".into(), message: None })
                })
            }
            mlua::Value::Integer(size) => TryInto::<u8>::try_into(size)
                .map_err(|err| mlua::Error::FromLuaConversionError { from: "integer", to: "u8".into(), message: Some(err.to_string()) })
                .map(Self::Bytes),
            value => Err(mlua::Error::FromLuaConversionError { from: value.type_name(), to: "TypeSpecifier".into(), message: None }),
        }
    }
}

fn read_tuple_from_table(table: mlua::Table, lua: &Lua) -> mlua::Result<(u32, TypeSpecifier, Option<i16>)> {
    FromLuaMulti::from_lua_multi(
        {
            let address: mlua::Value = table.get(1)?;
            let type_specifier: mlua::Value = table.get(2)?;
            let offset: mlua::Value = table.get(3)?;
            (address, type_specifier, offset)
        }.into_lua_multi(lua)?,
        lua
    )
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
        gamecube.set("GameIDAddress", GCN_BASE_ADDRESS)?;
        let connect = Rc::clone(&connection);
        gamecube.set("ReadSingle", lua.create_function(
            move |lua, (_, address, type_specifier, offset): (mlua::Value, u32, TypeSpecifier, Option<i16>)| {
                let connection = connect.borrow();
                let connection = connection.as_ref().ok_or(io::Error::from(io::ErrorKind::NotConnected))?;
                let read = Read::from_parts(address, type_specifier.size(), offset);
                let bytes = connection.gamecube_connection.read_single(read)?;
                let result = convert_bytes(lua, bytes, &type_specifier);
                Ok(result)
            }
        )?)?;
        let connect = Rc::clone(&connection);
        gamecube.set("Read", lua.create_function(
            move |lua, (_, read_list): (mlua::Value, Vec<Table>)| {
                let connection = connect.borrow();
                let connection = connection.as_ref().ok_or(io::Error::from(io::ErrorKind::NotConnected))?;
                let read_list = read_list.into_iter().map(|table| read_tuple_from_table(table, lua)).collect::<mlua::Result<Vec<_>>>()?;
                let (read_list, type_specifiers) = {
                    let mut reads = Vec::with_capacity(read_list.len());
                    let mut types = Vec::with_capacity(read_list.len());
                    for (addr, ty, offset) in read_list {
                        reads.push(Read::from_parts(addr, ty.size(), offset));
                        types.push(ty);
                    }
                    (reads, types)
                };
                let byte_arrays = connection.gamecube_connection.read(&read_list)?;
                Iterator::zip(byte_arrays.into_iter(), type_specifiers.into_iter()).map(|(bytes, type_specifier)| {
                    convert_bytes(lua, bytes, &type_specifier)
                }).collect::<mlua::Result<Vec<mlua::Value>>>()
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

    pub fn is_connected(&self) -> bool {
        self.connection.borrow().as_ref().and_then(|i| i.game_interface.as_ref()).is_some()
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

    pub fn verify_current_game(&self) -> Result<(), VerificationError> {
        let connection = self.connection.borrow();
        let interface = connection.as_ref()
            .and_then(|connection| connection.game_interface.as_ref()).ok_or(VerificationError::NotConnected)?;

        match interface.verify() {
            Ok(true) => Ok(()),
            Ok(false) => Err(VerificationError::VerificationFailed),
            Err(err) => Err(VerificationError::VerificationError(err)),
        }
    }

    pub fn get_info(&self) -> Option<InfoCommand> {
        self.connection.borrow().as_ref()
            .and_then(|c| c.game_interface.as_ref())
            .map(|interface|
                InfoCommand::new(
                    interface.name().unwrap_or(None).as_deref(),
                    interface.version().unwrap_or(None).as_deref()
                )
            )
    }

    pub fn run_game_watcher(&self) -> Option<mlua::Result<Vec<(String, mlua::Result<JsonValue>)>>> {
        let connection = self.connection.borrow();
        let interface = connection.as_ref().and_then(|c| c.game_interface.as_ref())?;
        Some(VariableStore::new(&self.lua)
            .and_then(|(store, table)| interface.run_game_watcher(&table).map(|_| store))
            .map(VariableStore::unwrap))
    }
}

#[derive(Debug, Clone)]
pub enum VerificationError {
    NotConnected,
    VerificationFailed,
    VerificationError(mlua::Error),
}

impl Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationError::NotConnected => "no interface is active".fmt(f),
            VerificationError::VerificationFailed => "active interface failed to verify current game".fmt(f),
            VerificationError::VerificationError(err) => {
                "active interface encountered an error while verifying current game: ".fmt(f)?;
                err.fmt(f)
            }
        }
    }
}

impl Error for VerificationError {}
