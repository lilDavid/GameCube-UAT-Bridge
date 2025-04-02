use std::{borrow::Borrow, error::Error, fmt::Display, num::ParseIntError, str::FromStr};

use json::JsonValue;

use crate::{connector::GameCubeConnector, uat::variable::Variable};

#[derive(Clone, Copy, Debug)]
pub enum VariableTypeName {
    U32,
    Constant,
}

#[derive(Clone, Debug)]
pub struct InvalidTypeName;

impl Display for InvalidTypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        "Invalid type name".fmt(f)
    }
}

impl Error for InvalidTypeName {}

impl FromStr for VariableTypeName {
    type Err = InvalidTypeName;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "u32" => Ok(Self::U32),
            "const" => Ok(Self::Constant),
            _ => Err(InvalidTypeName),
        }
    }
}

#[derive(Clone, Debug)]
pub enum VariableType {
    U32,
    Constant(Box<VariableType>),
}

impl VariableType {
    fn size(&self) -> u32 {
        match self {
            Self::U32 => 4,
            Self::Constant(ty) => ty.size(),
        }
    }
}

#[derive(Debug)]
pub enum TypeParseError {
    InvalidName(InvalidTypeName),
    InvalidNesting,
}

impl Display for TypeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(e) => e.fmt(f),
            Self::InvalidNesting => "Invalid nested type".fmt(f)
        }
    }
}

impl Error for TypeParseError {}

impl From<InvalidTypeName> for TypeParseError {
    fn from(value: InvalidTypeName) -> Self {
        Self::InvalidName(value)
    }
}

impl FromStr for VariableType {
    type Err = TypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut type_def = s.split_ascii_whitespace().map(VariableTypeName::from_str).collect::<Result<Vec<_>, _>>()?;
        let mut ty = match type_def.pop() {
            None => Err(TypeParseError::InvalidNesting),
            Some(VariableTypeName::Constant) => Err(TypeParseError::InvalidNesting),
            Some(VariableTypeName::U32) => Ok(Self::U32),
        }?;
        for (index, type_name) in type_def.iter().enumerate().rev() {
            ty = match (index, type_name) {
                (0, VariableTypeName::Constant) => Ok(Self::Constant(Box::new(ty))),
                _ => Err(TypeParseError::InvalidNesting),
            }?;
        }
        Ok(ty)
    }
}

#[derive(Clone, Debug)]
pub struct PointedVariable {
    address: u32,
    offset_list: Vec<i32>,
}

#[derive(Clone, Debug)]
pub enum VariableData {
    U32(PointedVariable),
    Constant(Variable),
}

#[derive(Debug)]
pub enum VariableParseError {
    MissingField(&'static str),
    TypeParseError(TypeParseError),
    ValueParseError(Option<ParseIntError>),
}

impl Display for VariableParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(s) => { "Missing field ".fmt(f)?; s.fmt(f) }
            Self::TypeParseError(e) => e.fmt(f),
            Self::ValueParseError(Some(e)) => e.fmt(f),
            Self::ValueParseError(None) => "Could not parse value".fmt(f),
        }
    }
}

impl Error for VariableParseError {}

impl From<TypeParseError> for VariableParseError {
    fn from(value: TypeParseError) -> Self {
        Self::TypeParseError(value)
    }
}

impl From<ParseIntError> for VariableParseError {
    fn from(value: ParseIntError) -> Self {
        Self::ValueParseError(Some(value))
    }
}

impl TryFrom<&JsonValue> for VariableData {
    type Error = VariableParseError;

    fn try_from(value: &JsonValue) -> Result<Self, Self::Error> {
        let ty = value["type"].as_str().ok_or(VariableParseError::MissingField("type"))?.parse()?;
        match ty {
            VariableType::U32 => {
                let address = parse_integer(value["address"].as_str().ok_or(VariableParseError::MissingField("address"))?)?;
                let offsets = parse_int_array(&value["offsets"])?;
                Ok(VariableData::U32(PointedVariable { address, offset_list: offsets }))
            },
            VariableType::Constant(v) => {
                match v.borrow() {
                    VariableType::U32 => {
                        let value = parse_integer(value["value"].as_str().ok_or(VariableParseError::MissingField("value"))?)?;
                        Ok(VariableData::Constant(Variable::U32(value)))
                    }
                    VariableType::Constant(_) => panic!()
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct VariableDefinition {
    name: String,
    data: VariableData,
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

fn parse_integer<I: Word>(string: &str) -> Result<I, VariableParseError> {
    if string.starts_with("0x") {
        I::from_str_radix(&string[2..], 16)
    } else {
        I::from_str_radix(&string, 10)
    }.map_err(VariableParseError::from)
}

fn try_get_integer<I: Word>(json: &JsonValue) -> Result<I, VariableParseError> {
    match json {
        JsonValue::Number(n) => I::try_from(*n).map_err(|_| VariableParseError::ValueParseError(None)),
        JsonValue::String(s) => parse_integer(s),
        JsonValue::Short(s) => parse_integer(s.as_str()),
        _ => Err(VariableParseError::ValueParseError(None))
    }
}

fn parse_int_array<I: Word>(json: &JsonValue) -> Result<Vec<I>, VariableParseError> {
    match json {
        JsonValue::Null => Ok(vec![]),
        JsonValue::Array(arr) => arr.iter()
            .map(try_get_integer)
            .collect::<Result<Vec<_>, VariableParseError>>(),
        _ => Err(VariableParseError::ValueParseError(None))
    }
}

impl VariableDefinition {
    pub fn new(name: &str, data: VariableData) -> Self {
        Self {
            name: name.into(),
            data
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn variable_type(&self) -> VariableType {
        match &self.data {
            VariableData::Constant(var) => match var {
                Variable::U32(_) => VariableType::Constant(Box::new(VariableType::U32))
            }
            VariableData::U32(_) => VariableType::U32,
        }
    }

    pub fn size(&self) -> u32 {
        self.variable_type().size()
    }
}

#[derive(Debug)]
pub enum VariableDefinitionParseError {
    MissingField(&'static str),
    VariableParseError(VariableParseError),
}

impl Display for VariableDefinitionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => { "Missing field: ".fmt(f)?; field.fmt(f) }
            Self::VariableParseError(e) => e.fmt(f),
        }
    }
}

impl Error for VariableDefinitionParseError {}

impl From<VariableParseError> for VariableDefinitionParseError {
    fn from(value: VariableParseError) -> Self {
        Self::VariableParseError(value)
    }
}

impl TryFrom<&JsonValue> for VariableDefinition {
    type Error = VariableDefinitionParseError;

    fn try_from(value: &JsonValue) -> Result<Self, Self::Error> {
        let name = value["name"].as_str().ok_or(VariableDefinitionParseError::MissingField("name"))?;
        let data = VariableData::try_from(value)?;
        Ok(Self::new(name, data))
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

    pub fn convert_bytes(bytes: &[u8], ty: &VariableType) -> Option<Variable> {
        match ty {
            VariableType::Constant(_) => None,
            VariableType::U32 => bytes[0..ty.size() as usize].try_into().map(u32::from_be_bytes).map(Variable::U32).ok(),
        }
    }

    pub fn read_variables(&mut self) -> impl Iterator<Item = (&str, Option<Variable>)> {
        self.variable_definitions.iter().map(|var| {
            let result = match &var.data {
                VariableData::Constant(val) => Some(val.clone()),
                VariableData::U32(ptr) => self.connector.read_pointers(var.size(), ptr.address, &ptr.offset_list)
                    .map(|bytes| Self::convert_bytes(&bytes, &VariableType::U32)).ok().flatten(),
            };
            (var.name.as_ref(), result)
        })
    }
}
