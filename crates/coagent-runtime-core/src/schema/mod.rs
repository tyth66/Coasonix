use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use thiserror::Error;

#[derive(Debug)]
pub struct SchemaRegistry {
    schema_path: PathBuf,
    root_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationResult {
    pub expected_schema: String,
    pub valid: bool,
    pub errors: Vec<SchemaValidationError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationError {
    pub path: String,
    pub message: String,
}

impl SchemaValidationResult {
    pub fn to_payload(&self, task_id: &str, request_id: Option<&str>) -> Value {
        let mut payload = Map::new();
        payload.insert(
            "schema_version".to_string(),
            Value::String("schema_validation_result_v1".to_string()),
        );
        payload.insert("task_id".to_string(), Value::String(task_id.to_string()));
        if let Some(request_id) = request_id {
            payload.insert(
                "request_id".to_string(),
                Value::String(request_id.to_string()),
            );
        }
        payload.insert(
            "expected_schema".to_string(),
            Value::String(self.expected_schema.clone()),
        );
        payload.insert("valid".to_string(), Value::Bool(self.valid));
        payload.insert(
            "errors".to_string(),
            Value::Array(
                self.errors
                    .iter()
                    .map(|error| {
                        let mut item = Map::new();
                        item.insert("path".to_string(), Value::String(error.path.clone()));
                        item.insert("message".to_string(), Value::String(error.message.clone()));
                        Value::Object(item)
                    })
                    .collect(),
            ),
        );
        Value::Object(payload)
    }
}

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("failed to read schema registry {path}: {source}")]
    ReadSchema {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid schema registry JSON in {path}: {source}")]
    ParseSchema {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("schema registry is missing $defs/{0}")]
    MissingDefinition(String),
    #[error("invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

impl SchemaRegistry {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, SchemaError> {
        let path = path.as_ref().to_path_buf();
        let schema_text = fs::read_to_string(&path).map_err(|source| SchemaError::ReadSchema {
            path: path.clone(),
            source,
        })?;
        let root_schema: Value =
            serde_json::from_str(&schema_text).map_err(|source| SchemaError::ParseSchema {
                path: path.clone(),
                source,
            })?;

        Ok(Self {
            schema_path: path,
            root_schema,
        })
    }

    pub fn load_from_str(
        schema_path: impl Into<PathBuf>,
        schema_text: &str,
    ) -> Result<Self, SchemaError> {
        let path = schema_path.into();
        let root_schema: Value =
            serde_json::from_str(schema_text).map_err(|source| SchemaError::ParseSchema {
                path: path.clone(),
                source,
            })?;

        Ok(Self {
            schema_path: path,
            root_schema,
        })
    }

    pub fn schema_path(&self) -> &Path {
        &self.schema_path
    }

    pub fn validate(&self, expected_schema: &str, payload: &Value) -> SchemaValidationResult {
        let wrapper = match self.wrapper_schema(expected_schema) {
            Ok(wrapper) => wrapper,
            Err(error) => {
                return SchemaValidationResult {
                    expected_schema: expected_schema.to_string(),
                    valid: false,
                    errors: vec![SchemaValidationError {
                        path: String::new(),
                        message: error.to_string(),
                    }],
                };
            }
        };

        let validator = match jsonschema::validator_for(&wrapper) {
            Ok(validator) => validator,
            Err(error) => {
                return SchemaValidationResult {
                    expected_schema: expected_schema.to_string(),
                    valid: false,
                    errors: vec![SchemaValidationError {
                        path: String::new(),
                        message: format!("invalid schema wrapper: {error}"),
                    }],
                };
            }
        };

        let errors: Vec<_> = validator
            .iter_errors(payload)
            .map(|error| SchemaValidationError {
                path: error.instance_path().to_string(),
                message: error.to_string(),
            })
            .collect();

        SchemaValidationResult {
            expected_schema: expected_schema.to_string(),
            valid: errors.is_empty(),
            errors,
        }
    }

    fn wrapper_schema(&self, expected_schema: &str) -> Result<Value, SchemaError> {
        let defs = self
            .root_schema
            .get("$defs")
            .and_then(Value::as_object)
            .ok_or_else(|| SchemaError::MissingDefinition(expected_schema.to_string()))?;

        if !defs.contains_key(expected_schema) {
            return Err(SchemaError::MissingDefinition(expected_schema.to_string()));
        }

        Ok(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$defs": defs,
            "allOf": [
                { "$ref": format!("#/$defs/{expected_schema}") }
            ]
        }))
    }
}

pub fn parse_json_no_duplicate_keys(input: &str) -> Result<Value, SchemaError> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = NoDuplicateValueSeed.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

struct NoDuplicateValueSeed;

impl<'de> DeserializeSeed<'de> for NoDuplicateValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NoDuplicateValueVisitor)
    }
}

struct NoDuplicateValueVisitor;

impl<'de> Visitor<'de> for NoDuplicateValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("any valid JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("non-finite number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(NoDuplicateValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = HashSet::new();
        let mut object = Map::new();

        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!("duplicate key `{key}`")));
            }
            let value = map.next_value_seed(NoDuplicateValueSeed)?;
            object.insert(key, value);
        }

        Ok(Value::Object(object))
    }
}
