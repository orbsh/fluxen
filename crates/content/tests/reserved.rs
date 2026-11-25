//! `is_transport_frame`: the pong is swallowed in either encoding, and nothing else is.

use content::codec::ActiveCodec;
use content::{Outflow, PING_DEST, PONG_DEST, is_transport_frame};
use serde_json::json;

fn pong(codec: ActiveCodec) -> Vec<u8> {
    codec
        .encode(&Outflow {
            dest: PONG_DEST.to_owned(),
            id: None,
            data: serde_json::Value::Null,
        })
        .expect("encode")
}

#[test]
fn the_pong_is_recognised_in_both_encodings() {
    assert!(is_transport_frame(&pong(ActiveCodec::Json)));
    assert!(is_transport_frame(&pong(ActiveCodec::Cbor)));
}

#[test]
fn a_page_frame_that_merely_mentions_the_word_is_not_swallowed() {
    // The prefilter is a byte scan, so a value containing `__pong__` must still be judged by its `dest`
    // — a page is allowed to say the word.
    let frame = ActiveCodec::Json
        .encode(&json!([{ "action": "set", "slot": "log", "data": { "type": "text", "source": { "default": "__pong__" } } }]))
        .expect("encode");
    assert!(!is_transport_frame(&frame));
}

#[test]
fn app_traffic_and_garbage_are_not_transport_frames() {
    let uplink = ActiveCodec::Cbor
        .encode(&Outflow {
            dest: PING_DEST.replace("__ping__", "probe.sent"),
            id: None,
            data: json!({ "n": 1 }),
        })
        .expect("encode");
    assert!(!is_transport_frame(&uplink));
    assert!(!is_transport_frame(b""));
    assert!(!is_transport_frame(b"not a frame at all"));
    // The marker without a decodable frame: a map-ish blob, still not ours.
    assert!(!is_transport_frame(b"__po"));
}
