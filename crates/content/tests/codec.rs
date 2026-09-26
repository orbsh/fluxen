//! Codec wire-format contract: Json/CBOR round-trips of Message/Content,
//! plus the `OneOrMany` shape the gateway wire relies on.

use content::codec::{ActiveCodec, CodecType};
use content::{Content, Influx, Message, Method};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Payload {
    n: i64,
    s: String,
}

fn sample() -> Message<Payload> {
    Message {
        ev: "draw".into(),
        sender: "stage".into(),
        created: None,
        content: vec![Content::Join(Influx {
            event: "chat".into(),
            data: Payload {
                n: 7,
                s: "hi".into(),
            },
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
    assert_eq!(
        serde_json::from_value::<Method>(json!("concat")).unwrap(),
        Method::Concat
    );
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
    assert!(
        v["content"].is_object(),
        "single content collapses to object"
    );
    assert_eq!(v["content"]["action"], json!("join"));
    // and the bare-object form deserializes back into a one-item vec
    let back: Message<Payload> = serde_json::from_value(v).unwrap();
    assert_eq!(back.content.len(), 1);
    // array form also accepted (stage emits arrays)
    let arr = json!({ "ev": "draw", "sender": "s", "content": [ { "action": "empty" } ] });
    let m2: Message<Value> = serde_json::from_value(arr).unwrap();
    assert!(matches!(m2.content[0], Content::Empty));
}

#[test]
fn ev_is_required_on_the_wire() {
    // ADR 0003: ev is a mandatory top-level field — a pre-ev frame must fail
    // to decode loudly, not be silently treated as draw.
    let no_ev = json!({ "sender": "s", "content": [{ "action": "empty" }] });
    assert!(serde_json::from_value::<Message<Value>>(no_ev).is_err());
    let with_ev = json!({ "ev": "draw", "sender": "s", "content": [{ "action": "empty" }] });
    let m: Message<Value> = serde_json::from_value(with_ev).unwrap();
    assert_eq!(m.ev, content::EV_DRAW);
}

#[test]
fn cross_codec_strict_decode_still_errors() {
    // explicit decode() stays strict: wrong codec must fail loudly
    let bytes = codec(CodecType::Json).encode(&sample()).unwrap();
    let r = codec(CodecType::Cbor).decode::<Message<Payload>>(&bytes);
    assert!(r.is_err());
}

#[test]
fn decode_auto_accepts_json_and_cbor_regardless_of_self() {
    // receiver-side auto-detect: frame shape decides, not the pinned codec
    let json_bytes = codec(CodecType::Json).encode(&sample()).unwrap();
    let cbor_bytes = codec(CodecType::Cbor).encode(&sample()).unwrap();
    for self_variant in [CodecType::Json, CodecType::Cbor] {
        let c = codec(self_variant);
        let a: Message<Payload> = c.decode_auto(&json_bytes).expect("json auto");
        let b: Message<Payload> = c.decode_auto(&cbor_bytes).expect("cbor auto");
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&sample()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&b).unwrap(),
            serde_json::to_value(&sample()).unwrap()
        );
    }
}

#[test]
fn decode_auto_rejects_garbage() {
    let c = codec(CodecType::Cbor);
    assert!(c.decode_auto::<Message<Payload>>(b"not a frame").is_err());
    assert!(c.decode_auto::<Message<Payload>>(b"{\"nope\":1}").is_err());
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
