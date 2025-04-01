use std::{borrow::Borrow, collections::HashMap, error::Error, fmt::Display};

pub type Variable = Option<u32>;  // TODO: Support more types

#[derive(Debug, Clone)]
pub struct VariableStore(HashMap<Box<str>, Variable>);

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

    pub fn update_variable(&mut self, name: &str, value: Variable) -> Result<bool, VariableUpdateError> {
        let entry = self.0.get_mut(name).ok_or(VariableUpdateError {})?;
        if entry == &value {
            Ok(false)
        } else {
            *entry = value;
            Ok(true)
        }
    }

    pub fn variable_values(&self) -> impl Iterator<Item = (&str, &Variable)> {
        self.0.iter().map(|(key, var)| (key.borrow(), var))
    }
}
