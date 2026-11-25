//! The wire's JSON Schema, as the consumers read it.
//!
//! `schemars` emits `$ref` + `$defs` for every named struct, with one exception: a struct that refuses
//! unknown keys (`deny_unknown_fields`). Draft-07 ignores siblings of `$ref`, so the tag cannot sit beside
//! the reference, and schemars inlines the whole definition into the enum's arm instead. That is a valid
//! schema and a broken contract: the arms are how a consumer names its types — reflux builds one Python
//! class and one Rust builder per arm's `$ref` — and an inlined arm names nothing.
//!
//! So an inlined arm is put back where it belongs: its body becomes `$defs/<Type>` (minus the tag, which
//! stays on the arm), and the arm keeps the shape every other arm has — the tag's `const` beside a `$ref`.
//! `additionalProperties: false` travels with the body, so the refusal is still on the wire.
//!
//! The work happens on a `Value`, whose maps are sorted: the export has never carried an object-key order
//! (schemars' `Schema` is a `Value` too), and the snapshot's consumers read it as a document, not as a
//! stream — so a re-homed arm costs its neighbours nothing.

use serde_json::{Map, Value, json};

/// The exported schema: `schemars`'s view of [`accrete::Accrete`], arms re-homed.
#[must_use]
pub fn exported() -> Value {
    let mut doc = serde_json::to_value(schemars::schema_for!(accrete::Accrete))
        .expect("a schema is JSON by construction");
    normalise(&mut doc);
    doc
}

/// Move every `$ref`-less arm's body into `$defs`, named after its tag.
fn normalise(doc: &mut Value) {
    let arms = doc
        .get_mut("oneOf")
        .and_then(Value::as_array_mut)
        .expect("Accrete is an enum: its schema has arms");
    let mut homes: Vec<(String, Value)> = Vec::new();
    for arm in arms.iter_mut() {
        // Only an inlined arm needs this; a `$ref` arm already names its type.
        if arm.get("$ref").is_some() {
            continue;
        }
        let Some(tag) = arm
            .pointer("/properties/type/const")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let mut body = arm.take();
        if let Some(props) = body.get_mut("properties").and_then(Value::as_object_mut) {
            props.remove("type");
        }
        if let Some(required) = body.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|v| v.as_str() != Some("type"));
            if required.is_empty() {
                body.as_object_mut().map(|o| o.remove("required"));
            }
        }
        let name = type_name(&tag);
        *arm = json!({
            "type": "object",
            "properties": { "type": { "type": "string", "const": tag } },
            "$ref": format!("#/$defs/{name}"),
            "required": ["type"],
        });
        homes.push((name, body));
    }
    if homes.is_empty() {
        return;
    }
    let defs: &mut Map<String, Value> = doc
        .get_mut("$defs")
        .and_then(Value::as_object_mut)
        .expect("a re-homed arm needs a home");
    for (name, body) in homes {
        defs.insert(name, body);
    }
}

/// A tag's type name: `input` → `Input`, `force-graph` → `ForceGraph`.
fn type_name(tag: &str) -> String {
    tag.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::exported;

    #[test]
    fn every_arm_names_a_type_and_a_refusal_keeps_its_fence() {
        let doc = exported();
        let arms = doc["oneOf"].as_array().expect("arms");
        assert!(arms.iter().all(|a| a.get("$ref").is_some()), "an arm must name its type");
        let defs = doc["$defs"].as_object().expect("defs");
        for arm in arms {
            let r = arm["$ref"].as_str().expect("$ref");
            let name = r.rsplit('/').next().expect("a path segment");
            assert!(defs.contains_key(name), "{name} is referenced but not defined");
        }
        // `input` is the type that made this necessary: it refuses unknown keys, so schemars inlined it.
        let input = &defs["Input"];
        assert_eq!(input["additionalProperties"], serde_json::json!(false));
        assert!(input["properties"].get("type").is_none(), "the tag belongs to the arm, not the body");
        assert_ne!(input["required"], serde_json::json!(["type"]));
    }
}
