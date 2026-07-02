use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error("cannot canonicalize non-finite JSON number")]
    NonFiniteNumber,
}

pub fn canonical_json(value: &Value) -> Result<String, CanonicalError> {
    let mut output = String::new();
    write_canonical(value, &mut output)?;
    Ok(output)
}

pub fn canonical_hash(value: &Value) -> Result<String, CanonicalError> {
    let canonical = canonical_json(value)?;
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(format!("sha256:{digest:x}"))
}

fn write_canonical(value: &Value, output: &mut String) -> Result<(), CanonicalError> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(number) => {
            if !(number.is_i64() || number.is_u64() || number.is_f64()) {
                return Err(CanonicalError::NonFiniteNumber);
            }
            output.push_str(&number.to_string());
        }
        Value::String(value) => {
            output.push_str(
                &serde_json::to_string(value).expect("string serialization is infallible"),
            );
        }
        Value::Array(values) => {
            output.push('[');
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical(item, output)?;
            }
            output.push(']');
        }
        Value::Object(object) => {
            let sorted: BTreeMap<_, _> = object.iter().collect();
            output.push('{');
            for (index, (key, item)) in sorted.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key).expect("string serialization is infallible"),
                );
                output.push(':');
                write_canonical(item, output)?;
            }
            output.push('}');
        }
    }

    Ok(())
}
