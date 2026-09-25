//! Codec wire-format contract: Json/CBOR round-trips of Message/Content,
//! plus the `OneOrMany` shape the gateway wire relies on.

use content::{Content, Influx, Message, Method};
use content::codec::{ActiveCodec, CodecType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Payload {
    n: i64,
    s: String,
}

fn sample() -> Message<Payload> {
    Message {
        sender: "stage".into(),
        created: None,
        content: vec![Content::Join(Influx {
            event: "chat".into(),
            data: Payload { n: 7, s: "hi".into() },
            method: Method::Concat,
            channel: None,
        })],
    }
}

fn codec(t: CodecType) -> ActiveCodec {
    ActiveCodec::new(t)
}

#[test]
fn json_roundtrip_preserves_message() {
    let c = codec(CodecType::Json);
    let m = sample();
    let bytes = c.encode(&m).expect("encode");
    let back: Message<Payload> = c.decode(&bytes).expect("decode");
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&m).unwrap()
    );
}

#[test]
fn cbor_roundtrip_preserves_message() {
    let c = codec(CodecType::Cbor);
    let m = sample();
    let bytes = c.encode(&m).expect("encode");
    let back: Message<Payload> = c.decode(&bytes).expect("decode");
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&m).unwrap()
    );
}

#[test]
fn method_wire_names_are_lowercase() {
    assert_eq!(
        serde_json::to_value(Method::Replace).unwrap(),
        json!("replace")
    );
    assert_eq!(
        serde_json::to_value(Method::Concat).unwrap(),
        json!("concat")
    );
    assert_eq!(
        serde_json::to_value(Method::Delete).unwrap(),
        json!("delete")
    );
    assert_eq!(serde_json::from_value::<Method>(json!("concat")).unwrap(), Method::Concat);
}

#[test]
fn method_defaults_to_replace_when_absent() {
    let v = json!({ "event": "e", "data": { "n": 1, "s": "x" } });
    let inf: Influx<Payload> = serde_json::from_value(v).unwrap();
    assert_eq!(inf.method, Method::Replace);
}

#[test]
fn content_tagged_by_action_and_accepts_single_or_array() {
    let m = sample();
    let v = serde_json::to_value(&m).unwrap();
    // OneOrMany: single element serializes as a bare object
    assert!(v["content"].is_object(), "single content collapses to object");
    assert_eq!(v["content"]["action"], json!("join"));
    // and the bare-object form deserializes back into a one-item vec
    let back: Message<Payload> = serde_json::from_value(v).unwrap();
    assert_eq!(back.content.len(), 1);
    // array form also accepted (stage emits arrays)
    let arr = json!({ "sender": "s", "content": [ { "action": "empty" } ] });
    let m2: Message<Value> = serde_json::from_value(arr).unwrap();
    assert!(matches!(m2.content[0], Content::Empty));
}

#[test]
fn cross_codec_interoperates_json_encoded_decoded_by_cbor_is_error() {
    // wrong-codec decode must fail loudly, not silently misparse
    let bytes = codec(CodecType::Json).encode(&sample()).unwrap();
    let r = codec(CodecType::Cbor).decode::<Message<Payload>>(&bytes);
    assert!(r.is_err());
}

#[test]
fn codec_type_from_str_accepts_both_names() {
    assert_eq!("json".parse::<CodecType>().unwrap(), CodecType::Json);
    assert_eq!("cbor".parse::<CodecType>().unwrap(), CodecType::Cbor);
    assert!("bincode".parse::<CodecType>().is_err());
}

#[test]
fn codec_as_type_roundtrips() {
    for t in [CodecType::Json, CodecType::Cbor] {
        assert_eq!(codec(t).as_type(), t);
    }
}
