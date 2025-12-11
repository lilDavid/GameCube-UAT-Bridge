use std::fmt::Display;

use json::{object, JsonValue};

use crate::uat::UAT_PROTOCOL_VERSION;

#[allow(dead_code)]
#[derive(Debug)]
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
#[derive(Debug)]
pub enum ClientCommand {
    Sync(SyncCommand),
}

impl TryFrom<&JsonValue> for ClientCommand {
    type Error = ErrorReplyCommand;

    fn try_from(value: &JsonValue) -> Result<Self, Self::Error> {
        if let JsonValue::Object(obj) = value {
            match obj["cmd"].as_str() {
                Some("Sync") => Ok(Self::Sync(SyncCommand::with_slot(
                    obj["slot"].as_str().map(String::from),
                ))),
                Some(s) => Err(ErrorReplyCommand::new(s, ErrorReplyReason::UnknownCmd)),
                None => Err(ErrorReplyCommand::with_description(
                    "",
                    ErrorReplyReason::MissingArgument,
                    Some("missing cmd"),
                )),
            }
        } else {
            Err(ErrorReplyCommand::with_description(
                "",
                ErrorReplyReason::BadValue,
                Some("expected object"),
            ))
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

    pub fn with_features(
        name: Option<&str>,
        version: Option<&str>,
        features: Option<&[&str]>,
        slots: Option<&[&str]>,
    ) -> Self {
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
        let mut cmd = object! {
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
        let mut cmd = object! {
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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorReplyReason {
    UnknownCmd,
    MissingArgument,
    BadValue,
    Unknown,
}

impl Display for ErrorReplyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCmd => "unknown cmd".fmt(f),
            Self::MissingArgument => "missing argument".fmt(f),
            Self::BadValue => "bad value".fmt(f),
            Self::Unknown => "unknown".fmt(f),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ErrorReplyCommand {
    name: String,
    argument: Option<String>,
    reason: ErrorReplyReason,
    description: Option<String>,
}

impl ErrorReplyCommand {
    pub fn new(name: &str, reason: ErrorReplyReason) -> Self {
        Self::with_argument_and_description(name, None, reason, None)
    }

    pub fn with_description(
        name: &str,
        reason: ErrorReplyReason,
        description: Option<&str>,
    ) -> Self {
        Self::with_argument_and_description(name, None, reason, description)
    }

    pub fn with_argument_and_description(
        name: &str,
        argument: Option<&str>,
        reason: ErrorReplyReason,
        description: Option<&str>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            argument: argument.map(str::to_owned),
            reason,
            description: description.map(str::to_owned),
        }
    }
}

impl Into<JsonValue> for ErrorReplyCommand {
    fn into(self) -> JsonValue {
        let mut value = object! {
            name: self.name,
            reason: self.reason.to_string(),
        };
        self.argument.map(|arg| value["argument"] = arg.into());
        self.description
            .map(|desc| value["description"] = desc.into());
        value
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
    pub fn info_with_features(
        name: Option<&str>,
        version: Option<&str>,
        features: Option<&[&str]>,
        slots: Option<&[&str]>,
    ) -> Self {
        Self::Info(InfoCommand::with_features(name, version, features, slots))
    }

    pub fn var(name: &str, value: JsonValue) -> Self {
        Self::Var(VarCommand::new(name, value))
    }
    pub fn var_with_slot(name: &str, value: JsonValue, slot: Option<i32>) -> Self {
        Self::Var(VarCommand::with_slot(name, value, slot))
    }

    pub fn error_reply(name: &str, reason: ErrorReplyReason) -> Self {
        Self::ErrorReply(ErrorReplyCommand::new(name, reason))
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
