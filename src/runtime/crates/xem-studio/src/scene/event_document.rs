//! The existing M6 document stays the single authored-event representation.
//! A scene owns its immutable original bundle plus this authored history, not a
//! compiled stream substituted for original input.
use crate::field::{FieldError, FieldGraph, draft};
use serde::{Deserialize, de};
use serde_json::Value;

fn portable(value: &Value) -> Result<(), &'static str> {
    match value {
        Value::Number(number) => {
            if number.as_u64().is_none_or(|n| n > u64::from(u32::MAX)) {
                return Err(
                    "M6 event document numeric fields must fit portable unsigned u32 ranges",
                );
            }
        }
        Value::Array(values) => {
            for value in values {
                portable(value)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                // Operand target maps use canonical numeric string keys. Check
                // them before the M6 usize-key deserializer on both hosts.
                if key == "targets"
                    && let Some(targets) = value.as_object()
                {
                    for key in targets.keys() {
                        let number = key.parse::<u32>().map_err(
                            |_| "M6 target keys must be canonical portable u32 operand indices",
                        )?;
                        if key != &number.to_string() {
                            return Err(
                                "M6 target keys must be canonical portable u32 operand indices",
                            );
                        }
                    }
                }
                portable(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn parse<E: de::Error>(value: Value) -> Result<Box<draft::Document>, E> {
    portable(&value).map_err(E::custom)?;
    serde_json::from_value(value)
        .map(Box::new)
        .map_err(E::custom)
}
pub fn action_from_json<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Box<draft::Action>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    portable(&value).map_err(de::Error::custom)?;
    serde_json::from_value(value)
        .map(Box::new)
        .map_err(de::Error::custom)
}
pub fn from_json<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Box<draft::Document>, D::Error> {
    parse(Value::deserialize(deserializer)?)
}
pub fn optional_from_json<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Box<draft::Document>>, D::Error> {
    Option::<Value>::deserialize(deserializer)?
        .map(parse)
        .transpose()
}
pub fn open(
    original: &FieldGraph,
    document: Option<Box<draft::Document>>,
) -> Result<draft::Draft, FieldError> {
    match document {
        None => draft::Draft::open(original.clone()),
        Some(document) => {
            // Compare against a canonical decode before importing untrusted
            // history. Its base cannot be a compiled graph or another source.
            if document.base != *original {
                return Err(FieldError::new(
                    "source_mismatch",
                    "M6 event document base differs from the scene's complete original event component",
                ));
            }
            draft::Draft::import(*document)
        }
    }
}
