use coagent_runtime_core::canonical::{canonical_hash, canonical_json};
use serde_json::json;

#[test]
fn sorts_object_keys_deterministically() {
    let payload = json!({
        "z": 1,
        "a": 2,
        "middle": {
            "b": true,
            "a": false
        }
    });

    let canonical = canonical_json(&payload).expect("canonical json");

    assert_eq!(canonical, r#"{"a":2,"middle":{"a":false,"b":true},"z":1}"#);
}

#[test]
fn equivalent_object_key_order_produces_identical_hash() {
    let left = json!({
        "schema_version": "review_result_v1",
        "task_id": "TASK-canonical",
        "confidence": 0.5
    });
    let right = json!({
        "confidence": 0.5,
        "task_id": "TASK-canonical",
        "schema_version": "review_result_v1"
    });

    assert_eq!(
        canonical_hash(&left).expect("left hash"),
        canonical_hash(&right).expect("right hash")
    );
}

#[test]
fn different_payload_content_produces_different_hash() {
    let left = json!({ "task_id": "TASK-one" });
    let right = json!({ "task_id": "TASK-two" });

    assert_ne!(
        canonical_hash(&left).expect("left hash"),
        canonical_hash(&right).expect("right hash")
    );
}

#[test]
fn array_order_is_preserved() {
    let first = json!({ "items": ["a", "b"] });
    let second = json!({ "items": ["b", "a"] });

    assert_ne!(
        canonical_json(&first).expect("first canonical"),
        canonical_json(&second).expect("second canonical")
    );
}

#[test]
fn non_finite_numbers_do_not_enter_json_values() {
    assert!(serde_json::Number::from_f64(f64::NAN).is_none());
    assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());
    assert!(serde_json::Number::from_f64(f64::NEG_INFINITY).is_none());
}
