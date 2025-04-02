use std::{num::ParseIntError, str::FromStr};

use json::JsonValue;

use crate::{connector::GameCubeConnector, uat::variable::Variable};

#[derive(Clone, Copy, Debug)]
pub enum VariableType {
    U32,
}

impl VariableType {
    fn size(&self) -> u32 {
        match self {
            Self::U32 => 4,
        }
    }
}

impl FromStr for VariableType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "u32" => Ok(Self::U32),
            _ => Err(()),
        }
    }
}

#[derive(Clone)]
pub struct VariableDefinition {
    name: Box<str>,
    type_: VariableType,
    address: u32,
    indirections: Box<[i32]>,
}

trait Word : TryFrom<json::number::Number> + FromStr {
    fn from_str_radix(string: &str, radix: u32) -> Result<Self, ParseIntError>;
}
impl Word for u32 {
    fn from_str_radix(string: &str, radix: u32) -> Result<Self, ParseIntError> {
        Self::from_str_radix(string, radix)
    }
}
impl Word for i32 {
    fn from_str_radix(string: &str, radix: u32) -> Result<Self, ParseIntError> {
        Self::from_str_radix(string, radix)
    }
}

impl VariableDefinition {
    pub fn new(name: &str, type_: VariableType, address: u32, offset_list: &[i32]) -> Self {
        Self {
            name: name.into(),
            type_,
            address,
            indirections: offset_list.into()
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn size(&self) -> u32 {
        self.type_.size()
    }

    fn parse_integer<I: Word>(string: &str) -> Result<I, ParseIntError> {
        if string.starts_with("0x") {
            I::from_str_radix(&string[2..], 16)
        } else {
            I::from_str_radix(&string, 10)
        }
    }

    fn try_get_integer<I: Word>(json: &JsonValue) -> Result<I, ()> {
        match json {
            JsonValue::Number(n) => I::try_from(*n).map_err(|_| ()),
            JsonValue::String(s) => Self::parse_integer(s).map_err(|_| ()),
            JsonValue::Short(s) => Self::parse_integer(s.as_str()).map_err(|_| ()),
            _ => Err(())
        }
    }
}

impl TryFrom<&JsonValue> for VariableDefinition {
    type Error = ();

    fn try_from(value: &JsonValue) -> Result<Self, Self::Error> {
        let name = value["name"].as_str().ok_or(())?;
        let type_ = VariableType::from_str(value["type"].as_str().ok_or(())?)?;
        let address = Self::try_get_integer(&value["address"])?;
        let offsets = match &value["offsets"] {
            JsonValue::Null => vec![],
            JsonValue::Array(arr) => arr.iter()
                .map(Self::try_get_integer)
                .collect::<Result<Vec<_>, ()>>()?,
            _ => Err(())?
        };
        Ok(Self::new(name, type_, address, &offsets))
    }
}

pub struct GameCubeInterface {
    variable_definitions: Box<[VariableDefinition]>,
    connector: Box<dyn GameCubeConnector + Send>,
}

impl GameCubeInterface {
    pub fn new(connector: Box<dyn GameCubeConnector + Send>, variables: impl Into<Box<[VariableDefinition]>>) -> Self {
        Self {
            connector,
            variable_definitions: variables.into()
        }
    }

    pub fn variable_definitions(&self) -> impl Iterator<Item = &VariableDefinition> {
        self.variable_definitions.iter()
    }

    pub fn read_variables(&mut self) -> impl Iterator<Item = (&str, Variable)> {
        self.variable_definitions.iter().map(|var| {
            let result = self.connector.read_pointers(var.size(), var.address, &var.indirections).ok()
                .map(|bytes| match var.type_ {
                    VariableType::U32 => bytes[..4].try_into().ok().map(u32::from_be_bytes)
                }).flatten();
            (var.name.as_ref(), result)
        })
    }
}
