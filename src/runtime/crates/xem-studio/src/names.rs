//! Validate names sidecars without writing user data or guessing disc context.
use crate::StudioError;
use serde::{Deserialize, Serialize, de};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NamesContext {
    /// Loaded map metadata, merged across open discs. A missing map is unknown,
    /// not an assertion that its actor table is empty.
    #[serde(default)]
    pub actor_counts: BTreeMap<u32, usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NameTable {
    pub schema_version: u32,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub maps: BTreeMap<String, MapNames>,
    #[serde(default)]
    pub messages: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MapNames {
    #[serde(default)]
    pub actors: BTreeMap<String, ActorNames>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActorNames {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ValidatedNames {
    pub table: NameTable,
    pub variable_count: usize,
    pub actor_count: usize,
    pub event_count: usize,
    pub message_count: usize,
}

fn invalid(message: impl Into<String>) -> StudioError {
    StudioError::new("invalid_names", message)
}

fn id(key: &str, limit: usize, scope: &str) -> Result<usize, StudioError> {
    let value = key
        .parse::<usize>()
        .map_err(|error| invalid(format!("{scope} ID `{key}`: {error}")))?;
    if key != value.to_string() || value >= limit {
        return Err(invalid(format!(
            "{scope} ID `{key}` must be canonical decimal in 0..{limit}"
        )));
    }
    Ok(value)
}

fn unique_name(name: &str, seen: &mut BTreeSet<String>, scope: &str) -> Result<(), StudioError> {
    if name.is_empty() || name.trim() != name || name.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{scope}: names must be nonempty, have no surrounding whitespace or control characters"
        )));
    }
    // This is Unicode lowercasing, not a locale-dependent collation rule.
    if !seen.insert(name.to_lowercase()) {
        return Err(invalid(format!(
            "{scope}: duplicate name `{name}` (case-insensitive)"
        )));
    }
    Ok(())
}

/// Parse the original sidecar text so duplicate JSON keys are not lost by a
/// generic JSON object parser. The caller must preserve invalid files unchanged.
pub fn validate(json: &str, context: &NamesContext) -> Result<ValidatedNames, StudioError> {
    let value: UniqueJson = serde_json::from_str(json)
        .map_err(|error| StudioError::new("invalid_names_json", error.to_string()))?;
    let table: NameTable =
        serde_json::from_value(value.0).map_err(|error| invalid(error.to_string()))?;
    validate_table(table, context)
}

pub fn validate_table(
    table: NameTable,
    context: &NamesContext,
) -> Result<ValidatedNames, StudioError> {
    if table.schema_version != 1 {
        return Err(invalid("names schema_version must be 1"));
    }
    if context.actor_counts.keys().any(|map| *map > 0xfff) {
        return Err(invalid(
            "map context exceeds the original 12-bit field ID range",
        ));
    }
    let mut variables = BTreeSet::new();
    for (key, name) in &table.variables {
        id(key, 1024, "variable slot")?;
        unique_name(name, &mut variables, "variables")?;
    }
    let mut messages = BTreeSet::new();
    for (key, name) in &table.messages {
        // Message text/count is not decoded in this milestone. Only its proven
        // u16 operand domain is validated; existence in a text bank is not claimed.
        id(key, 65536, "message")?;
        unique_name(name, &mut messages, "messages")?;
    }
    let mut actor_count = 0;
    let mut event_count = 0;
    for (map_key, map_names) in &table.maps {
        let map = id(map_key, 4096, "map")? as u32;
        let count = context.actor_counts.get(&map).ok_or_else(|| {
            StudioError::new(
                "names_context_required",
                format!(
                    "map {map}: load its disc map metadata before validating actor/event names"
                ),
            )
        })?;
        let mut actors = BTreeSet::new();
        for (actor_key, actor_names) in &map_names.actors {
            id(actor_key, *count, &format!("map {map} actor"))?;
            if let Some(name) = &actor_names.name {
                unique_name(name, &mut actors, &format!("map {map} actors"))?;
                actor_count += 1;
            }
            let mut events = BTreeSet::new();
            for (event_key, name) in &actor_names.events {
                id(event_key, 32, &format!("map {map} actor {actor_key} event"))?;
                unique_name(
                    name,
                    &mut events,
                    &format!("map {map} actor {actor_key} events"),
                )?;
                event_count += 1;
            }
        }
    }
    Ok(ValidatedNames {
        variable_count: table.variables.len(),
        actor_count,
        event_count,
        message_count: table.messages.len(),
        table,
    })
}

struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::from(value)))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::from(value)))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<UniqueJson, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| UniqueJson(Value::Number(number)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::String(value.to_owned())))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::String(value)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = access.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(Value::Array(values)))
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = access.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate JSON key `{key}`")));
                    }
                    values.insert(key, access.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
