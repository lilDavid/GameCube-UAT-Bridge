use std::{borrow::Borrow, collections::HashMap};

use json::JsonValue;

#[derive(Debug, Clone)]
pub struct VariableStore(HashMap<String, JsonValue>);

impl VariableStore {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn update_variable(&mut self, name: &str, value: JsonValue) -> bool {
        let entry = self.0.remove_entry(name);
        let (key, result) = match entry {
            Some((key, old_value)) => (key, old_value != value),
            None => (name.to_owned(), true),
        };
        self.0.insert(key, value);
        result
    }

    pub fn variable_values(&self) -> impl Iterator<Item = (&str, &JsonValue)> {
        self.0.iter().map(|(key, var)| (key.borrow(), var))
    }
}
