//! Host-independent studio commands, shared by native callers and wasm-bindgen.
//!
//! See tools/studio/README.md for the JSON protocol. No game data is loaded by
//! the milestone-one service.

use serde::Deserialize;
use serde_json::{Value, json};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const OPERATIONS: &[&str] = &["version", "echo"];

#[derive(Deserialize)]
#[serde(
    tag = "op",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Command {
    Version,
    Echo(Value),
}

impl Command {
    fn parse(input: &str) -> Result<Self, StudioError> {
        let value: Value = serde_json::from_str(input)
            .map_err(|error| StudioError::new("malformed_json", error.to_string()))?;
        let op = value.get("op").and_then(Value::as_str).ok_or_else(|| {
            StudioError::new(
                "invalid_command",
                "expected an object with a string `op` field",
            )
        })?;
        if !OPERATIONS.contains(&op) {
            return Err(StudioError::new(
                "unknown_operation",
                format!("unknown operation `{op}`"),
            ));
        }
        serde_json::from_value(value)
            .map_err(|error| StudioError::new("invalid_args", error.to_string()))
    }

    fn execute(self) -> CommandResult {
        match self {
            Self::Version => CommandResult::Version {
                version: env!("CARGO_PKG_VERSION"),
                operations: OPERATIONS,
            },
            Self::Echo(value) => CommandResult::Echo(value),
        }
    }
}

enum CommandResult {
    Version {
        version: &'static str,
        operations: &'static [&'static str],
    },
    Echo(Value),
}

impl CommandResult {
    fn into_json(self) -> Value {
        match self {
            Self::Version {
                version,
                operations,
            } => json!({ "version": version, "operations": operations }),
            Self::Echo(value) => value,
        }
    }
}

/// An operation's explicit failure, preserved across the JSON boundary.
#[derive(Debug)]
pub struct StudioError {
    pub kind: &'static str,
    pub message: String,
}

impl StudioError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn into_json(self) -> Value {
        json!({ "error": { "kind": self.kind, "message": self.message } })
    }
}

/// The studio service. Each call returns JSON, including when input is invalid.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Default)]
pub struct Studio;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Studio {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self
    }

    pub fn command(&self, input: &str) -> String {
        match Command::parse(input) {
            Ok(command) => command.execute().into_json(),
            Err(error) => error.into_json(),
        }
        .to_string()
    }
}
