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
pub struct VariableStore(HashMap<String, JsonValue>);

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

    pub fn update_variable(&mut self, name: &str, value: JsonValue) -> bool {
        let entry = self.0.remove_entry(name);
        let (key, result) = if let Some((key, old_value)) = entry {
            if old_value == value {
                (key, false)
            } else {
                (key, true)
            }
        } else {
            (name.to_owned(), false)
        };
        self.0.insert(key, value);
        result
    }

    pub fn variable_values(&self) -> impl Iterator<Item = (&str, &JsonValue)> {
        self.0.iter().map(|(key, var)| (key.borrow(), var))
    }
}
