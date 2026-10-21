//! Host-independent studio commands, shared by native callers and wasm-bindgen.
//!
//! See tools/studio/README.md for the JSON protocol. No game data is loaded by
//! this service without an explicit field.decode command.

pub mod field;

use serde::Deserialize;
use serde_json::{Value, json};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const OPERATIONS: &[&str] = &["version", "echo", "field.decode", "field.encode"];

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
    #[serde(rename = "field.decode")]
    FieldDecode(FieldDecodeArgs),
    #[serde(rename = "field.encode")]
    FieldEncode(FieldEncodeArgs),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldDecodeArgs {
    bytes: Vec<u8>,
    declared_size: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldEncodeArgs {
    graph: Box<field::FieldGraph>,
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

    fn execute(self) -> Result<CommandResult, StudioError> {
        Ok(match self {
            Self::Version => CommandResult::Version {
                version: env!("CARGO_PKG_VERSION"),
                operations: OPERATIONS,
            },
            Self::Echo(value) => CommandResult::Echo(value),
            Self::FieldDecode(args) => {
                CommandResult::FieldGraph(Box::new(field::decode(&args.bytes, args.declared_size)?))
            }
            Self::FieldEncode(args) => CommandResult::FieldBytes(field::encode(&args.graph)?),
        })
    }
}

enum CommandResult {
    Version {
        version: &'static str,
        operations: &'static [&'static str],
    },
    Echo(Value),
    FieldGraph(Box<field::FieldGraph>),
    FieldBytes(Vec<u8>),
}

impl CommandResult {
    fn into_json(self) -> Value {
        match self {
            Self::Version {
                version,
                operations,
            } => json!({ "version": version, "operations": operations }),
            Self::Echo(value) => value,
            Self::FieldGraph(graph) => json!(graph),
            Self::FieldBytes(bytes) => json!(bytes),
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

impl From<field::FieldError> for StudioError {
    fn from(error: field::FieldError) -> Self {
        Self::new(error.kind, error.message)
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
        match Command::parse(input).and_then(Command::execute) {
            Ok(result) => json!({ "ok": result.into_json() }),
            Err(error) => error.into_json(),
        }
        .to_string()
    }
}
