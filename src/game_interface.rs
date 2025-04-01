use crate::{connector::GameCubeConnector, uat::variable::Variable};

#[derive(Clone, Copy)]
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

#[derive(Clone)]
pub struct VariableDefinition {
    name: Box<str>,
    type_: VariableType,
    address: u32,
    indirections: Box<[i32]>,
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
