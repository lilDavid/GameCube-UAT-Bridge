use std::collections::HashMap;

use json::JsonValue;

use crate::game_interface::VariableDefinition;


pub struct GameInfo {
    game_name: Box<str>,
    version: Box<str>,
    variables: Vec<VariableDefinition>,
}

impl GameInfo {
    pub fn new(name: &str, version: &str, variables: &[VariableDefinition]) -> Self {
        Self {
            game_name: name.into(),
            version: version.into(),
            variables: variables.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.game_name
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn variables(&self) -> &[VariableDefinition] {
        &self.variables
    }
}

pub struct GameRegister(HashMap<(Box<str>, u8), GameInfo>);

impl GameRegister {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn register(&mut self, game_id: &str, revision: u8, game_name: &str, version: &str, variables: &[VariableDefinition]) {
        self.0.insert((game_id.into(), revision), GameInfo::new(game_name, version, variables));
    }

    pub fn register_from_json(&mut self, json_value: &JsonValue) -> Result<(), ()> {
        let obj = match json_value {
            JsonValue::Object(o) => o,
            _ => Err(())?,
        };
        let game_id = obj["gameID"].as_str().ok_or(())?;
        let revision = obj["revision"].as_u8().ok_or(())?;
        let game_name = obj["game"].as_str().ok_or(())?;
        let version = obj["version"].as_str().ok_or(())?;
        let variables = &obj["variables"];
        if !variables.is_array() {
            return Err(());
        }
        let variables = variables.members()
            .map(VariableDefinition::try_from)
            .collect::<Result<Vec<_>, ()>>()?;
        self.register(game_id, revision, game_name, version, &variables);
        Ok(())
    }

    pub fn identify(&self, game_id: &str, revision: u8) -> Option<&GameInfo> {
        self.0.get(&(game_id.into(), revision))
    }
}
