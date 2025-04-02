use std::{borrow::Borrow, collections::HashMap, error::Error, fmt::Display};

use json::JsonValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Variable {
    U32(u32),
}

impl Into<JsonValue> for Variable {
    fn into(self) -> JsonValue {
        (&self).into()
    }
}

impl Into<JsonValue> for &Variable {
    fn into(self) -> JsonValue {
        match self {
            Variable::U32(i) => (*i).into()
        }
    }
}

#[derive(Debug, Clone)]
pub struct VariableStore(HashMap<Box<str>, Option<Variable>>);

#[derive(Debug)]
pub struct VariableRegisterError;

impl Display for VariableRegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Variable already registered")
    }
}

impl Error for VariableRegisterError {}

#[derive(Debug)]
pub struct VariableUpdateError;

impl Display for VariableUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Variable not registered")
    }
}

impl Error for VariableUpdateError {}

impl VariableStore {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn register_variable(&mut self, name: &str) -> Result<(), VariableRegisterError> {
        if let Some(_) = self.0.get(name) {
            return Err(VariableRegisterError);
        }

        self.0.insert(name.into(), None);
        Ok(())
    }

    pub fn update_variable(&mut self, name: &str, value: Option<Variable>) -> Result<bool, VariableUpdateError> {
        let entry = self.0.get_mut(name).ok_or(VariableUpdateError {})?;
        if entry == &value {
            Ok(false)
        } else {
            *entry = value;
            Ok(true)
        }
    }

    pub fn variable_values(&self) -> impl Iterator<Item = (&str, Option<&Variable>)> {
        self.0.iter().map(|(key, var)| (key.borrow(), var.as_ref()))
    }
}
