use json::{object, JsonValue};

use crate::uat::UAT_PROTOCOL_VERSION;

#[allow(dead_code)]
pub struct SyncCommand {
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

#[allow(dead_code)]
pub enum ClientCommand {
    Sync(SyncCommand),
    Invalid,
}

impl From<JsonValue> for ClientCommand {
    fn from(value: JsonValue) -> Self {
        if let JsonValue::Object(obj) = value {
            match obj["cmd"].as_str() {
                Some("Sync") => Self::Sync(SyncCommand { slot: obj["slot"].as_str().map(String::from) }),
                _ => Self::Invalid,
            }
        } else {
            Self::Invalid
        }
    }
}

#[derive(Debug, Clone)]
pub struct InfoCommand {
    name: Option<String>,
    version: Option<String>,
    features: Option<Vec<String>>,
    slots: Option<Vec<String>>,
}

impl InfoCommand {
    pub fn new(name: Option<&str>, version: Option<&str>) -> Self {
        Self::with_features(name, version, None, None)
    }

    pub fn with_features(name: Option<&str>, version: Option<&str>, features: Option<&[&str]>, slots: Option<&[&str]>) -> Self {
        Self {
            name: name.map(str::to_owned),
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
    value: JsonValue,
    slot: Option<i32>,
}

impl VarCommand {
    pub fn new(name: &str, value: JsonValue) -> Self {
        Self::with_slot(name, value, None)
    }

    pub fn with_slot(name: &str, value: JsonValue, slot: Option<i32>) -> Self {
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

impl ErrorReplyCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl Into<JsonValue> for ErrorReplyCommand {
    fn into(self) -> JsonValue {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub enum ServerCommand {
    Info(InfoCommand),
    Var(VarCommand),
    ErrorReply(ErrorReplyCommand),
}

#[allow(dead_code)]
impl ServerCommand {
    pub fn info(name: Option<&str>, version: Option<&str>) -> Self {
        Self::Info(InfoCommand::new(name, version))
    }
    pub fn info_with_features(name: Option<&str>, version: Option<&str>, features: Option<&[&str]>, slots: Option<&[&str]>) -> Self {
        Self::Info(InfoCommand::with_features(name, version, features, slots))
    }

    pub fn var(name: &str, value: JsonValue) -> Self {
        Self::Var(VarCommand::new(name, value))
    }
    pub fn var_with_slot(name: &str, value: JsonValue, slot: Option<i32>) -> Self {
        Self::Var(VarCommand::with_slot(name, value, slot))
    }

    pub fn error_reply() -> Self {
        Self::ErrorReply(ErrorReplyCommand::new())
    }
}

impl Into<JsonValue> for ServerCommand {
    fn into(self) -> JsonValue {
        match self {
            Self::Info(cmd) => cmd.into(),
            Self::Var(cmd) => cmd.into(),
            Self::ErrorReply(cmd) => cmd.into(),
        }
    }
}
