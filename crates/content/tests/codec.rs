//! Codec wire-format contract: Json/CBOR round-trips of the action list,
//! plus the receive-side auto-detect that tells an action list apart from a
//! host envelope (ADR 0021 decision 6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result, clippy::panic_in_result_fn)]

use content::codec::{ActiveCodec, CodecType};
use content::{AppendOp, Content, Influx, PatchKind, PatchOp, RemoveOp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Payload {
    n: i64,
    s: String,
}

/// A frame is the action list itself — there is no envelope in this crate.
type Frame<T> = Vec<Content<T>>;

fn sample() -> Frame<Payload> {
    vec![Content::Append(AppendOp {
        event: "chat".into(),
        position: 0,
        data: Payload {
            n: 7,
            s: "hi".into(),
        },
    })]
}

fn codec(t: CodecType) -> ActiveCodec {
    ActiveCodec::new(t)
}

#[test]
fn json_roundtrip_preserves_the_action_list() {
    let c = codec(CodecType::Json);
    let m = sample();
    let bytes = c.encode(&m).expect("encode");
    let back: Frame<Payload> = c.decode(&bytes).expect("decode");
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&m).unwrap()
    );
}

#[test]
fn cbor_roundtrip_preserves_the_action_list() {
    let c = codec(CodecType::Cbor);
    let m = sample();
    let bytes = c.encode(&m).expect("encode");
    let back: Frame<Payload> = c.decode(&bytes).expect("decode");
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&m).unwrap()
    );
}

#[test]
fn action_wire_names_are_lowercase() {
    let cases = [
        (
            Content::Set(Influx {
                event: "e".into(),
                data: Payload { n: 1, s: "x".into() },
            }),
            "set",
        ),
        (
            Content::Append(AppendOp {
                event: "e".into(),
                position: 0,
                data: Payload { n: 1, s: "x".into() },
            }),
            "append",
        ),
        (
            Content::Remove(RemoveOp {
                event: "e".into(),
                id: "x".into(),
            }),
            "remove",
        ),
        (
            Content::Patch(PatchOp {
                event: "e".into(),
                id: None,
                path: "/a/b".into(),
                op: PatchKind::Replace,
                value: json!(1),
            }),
            "patch",
        ),
    ];
    for (c, name) in cases {
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["action"], json!(name), "wire name for {name}");
    }
}

#[test]
fn patch_op_wire_shape() {
    let v = serde_json::to_value(PatchKind::Append).unwrap();
    assert_eq!(v, json!("append"));
    assert_eq!(
        serde_json::from_value::<PatchKind>(json!("replace")).unwrap(),
        PatchKind::Replace
    );
    // Patch is self-describing: op is a required wire field
    let p: PatchOp = serde_json::from_value(json!({
        "event": "e", "path": "/x", "op": "append", "value": "tok"
    }))
    .unwrap();
    assert_eq!(p.id, None);
    assert_eq!(p.op, PatchKind::Append);
}

#[test]
fn influx_no_longer_carries_method_or_channel() {
    // ADR 0005: `method` was the positional-merge knob — retired with join.
    let v = json!({ "event": "e", "data": { "n": 1, "s": "x" } });
    let inf: Influx<Payload> = serde_json::from_value(v).unwrap();
    let back = serde_json::to_value(&inf).unwrap();
    assert_eq!(back, json!({ "event": "e", "data": { "n": 1, "s": "x" } }));
}

#[test]
fn a_frame_is_an_array_and_a_bare_action_is_not_a_frame() {
    let v = serde_json::to_value(sample()).unwrap();
    assert!(v.is_array(), "the frame itself is the array");
    assert_eq!(v[0]["action"], json!("append"));
    // One action on its own is not a frame: the wire is a list, always.
    assert!(serde_json::from_value::<Frame<Value>>(json!({ "action": "empty" })).is_err());
    assert_eq!(
        serde_json::from_value::<Frame<Value>>(json!([{ "action": "empty" }]))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn an_envelope_is_not_a_frame() {
    // The envelope belongs to the host (ADR 0021 decision 6): a `Message`-shaped
    // frame must fail to decode here, and the auto-detect says why.
    let envelope = json!({ "ev": "draw", "sender": "s", "content": [{ "action": "empty" }] });
    let bytes = serde_json::to_vec(&envelope).unwrap();
    assert!(codec(CodecType::Json).decode::<Frame<Value>>(&bytes).is_err());
    let err = codec(CodecType::Json)
        .decode_auto::<Frame<Value>>(&bytes)
        .expect_err("envelope refused");
    assert!(
        err.to_string().contains("envelope"),
        "the refusal names the reason: {err}"
    );
}

#[test]
fn cross_codec_strict_decode_still_errors() {
    // explicit decode() stays strict: wrong codec must fail loudly
    let bytes = codec(CodecType::Json).encode(&sample()).unwrap();
    let r = codec(CodecType::Cbor).decode::<Frame<Payload>>(&bytes);
    assert!(r.is_err());
}

#[test]
fn decode_auto_accepts_json_and_cbor_regardless_of_self() {
    // receiver-side auto-detect: frame shape decides, not the pinned codec
    let json_bytes = codec(CodecType::Json).encode(&sample()).unwrap();
    let cbor_bytes = codec(CodecType::Cbor).encode(&sample()).unwrap();
    for self_variant in [CodecType::Json, CodecType::Cbor] {
        let c = codec(self_variant);
        let a: Frame<Payload> = c.decode_auto(&json_bytes).expect("json auto");
        let b: Frame<Payload> = c.decode_auto(&cbor_bytes).expect("cbor auto");
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
    assert!(c.decode_auto::<Frame<Payload>>(b"not a frame").is_err());
    assert!(c.decode_auto::<Frame<Payload>>(b"[\"nope\"]").is_err());
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