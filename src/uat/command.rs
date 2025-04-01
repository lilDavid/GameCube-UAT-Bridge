use json::{object, JsonValue};

use crate::uat::{UAT_PROTOCOL_VERSION, variable::Variable};

pub struct SyncCommand {
    #[allow(dead_code)]
    slot: Option<String>,
}

#[allow(dead_code)]
impl SyncCommand {
    pub fn new() -> Self {
        Self::with_slot(None)
    }

    pub fn with_slot(slot: Option<String>) -> Self {
        Self { slot }
    }
}

pub enum ClientCommand {
    #[allow(dead_code)]
    Sync(SyncCommand),
}

impl TryFrom<&JsonValue> for ClientCommand {
    type Error = ();

    fn try_from(value: &JsonValue) -> Result<Self, Self::Error> {
        if let JsonValue::Object(obj) = value {
            match obj["cmd"].as_str() {
                Some("Sync") => Ok(Self::Sync(SyncCommand { slot: obj["slot"].as_str().map(String::from) })),
                _ => Err(()),
            }
        } else {
            Err(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct InfoCommand {
    name: String,
    version: Option<String>,
    features: Option<Vec<String>>,
    slots: Option<Vec<String>>,
}

impl InfoCommand {
    pub fn new(name: &str, version: Option<&str>) -> Self {
        Self::with_features(name, version, None, None)
    }

    pub fn with_features(name: &str, version: Option<&str>, features: Option<&[&str]>, slots: Option<&[&str]>) -> Self {
        Self {
            name: name.to_owned(),
            version: version.map(str::to_owned),
            features: features.map(|slice| slice.iter().copied().map(str::to_owned).collect()),
            slots: slots.map(|slice| slice.iter().copied().map(str::to_owned).collect()),
        }
    }
}

impl Into<JsonValue> for InfoCommand {
    fn into(self) -> JsonValue {
        let mut cmd = object!{
            cmd: "Info",
            name: self.name,
            version: self.version,
            protocol: UAT_PROTOCOL_VERSION,
        };
        if let Some(features) = self.features {
            cmd["features"] = JsonValue::from(features);
        }
        if let Some(slots) = self.slots {
            cmd["slots"] = JsonValue::from(slots);
        }
        cmd
    }
}

#[derive(Debug, Clone)]
pub struct VarCommand {
    name: String,
    value: Variable,
    slot: Option<i32>,
}

impl VarCommand {
    pub fn new(name: &str, value: Variable) -> Self {
        Self::with_slot(name, value, None)
    }

    pub fn with_slot(name: &str, value: Variable, slot: Option<i32>) -> Self {
        Self {
            name: name.to_owned(),
            value,
            slot,
        }
    }
}

impl Into<JsonValue> for VarCommand {
    fn into(self) -> JsonValue {
        let mut cmd = object!{
            cmd: "Var",
            name: self.name,
            value: self.value,
        };
        if let Some(slot) = self.slot {
            cmd["slot"] = JsonValue::from(slot);
        }
        cmd
    }
}

#[derive(Debug, Clone)]
pub struct ErrorReplyCommand {
}

impl Into<JsonValue> for ErrorReplyCommand {
    fn into(self) -> JsonValue {
        todo!()
    }
}
