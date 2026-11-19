//! ADR 0031 §4 step 4b: `tiles` grew `columns` (a grid) and a `header`, and the row came with it
//! as its own node, `record`. The `table/thead/tbody/tr/th/td` family is deleted, not aliased.
//! (That step shipped under those two spellings; the frame now says `fields` and `entry`.)
//!
//! These are wire-contract checks, so every shape is decoded through the codec: what a frame may
//! say is the thing being pinned, and a retired spelling must not decode at all.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn
)]

use accrete::Accrete;
use serde_json::json;

fn roundtrip(v: serde_json::Value) -> Accrete {
    let a: Accrete = serde_json::from_value(v).expect("decode");
    let back = serde_json::to_string(&a).expect("encode");
    let b: Accrete = serde_json::from_str(&back).expect("re-decode");
    assert_eq!(a, b, "round-trip changed the node: {back}");
    b
}

/// A grid is `tiles` + `fields` + `header` + `entry` items — the four things step 4b adds, in
/// one frame, because they are only meaningful together: fields project an entry's fields, and the
/// header reads the fields' keys.
#[test]
fn a_grid_round_trips_with_its_header_and_rows() {
    let grid = roundtrip(json!({
        "type": "tiles",
        "attrs": {
            "fields": [
                {"key": "qty", "title": "", "width": "4rem"},
                {"key": "name", "title": "Name"},
                {"key": "price/retail", "title": "Retail", "width": "6rem", "align": "end"},
                {"key": "note", "selectors": ["muted", "wrap"]}
            ],
            "header": {"selector": "--goods-head", "rows": 2}
        },
        "source": {"slot": "goods"},
        "children": [
            {
                "type": "entry",
                "id": "853736",
                "children": [
                    {"type": "input", "attrs": {"selector": "qty"},
                     "source": {"default": 1, "type": "number"},
                     "sink": {"slot": "853736", "scope": "local"}},
                    {"type": "text", "attrs": {"selector": "name"},
                     "source": {"default": "Honeywell 901 Mask Headband Color Box"}},
                    {"type": "text", "attrs": {"selector": "price/retail"},
                     "source": {"default": "2.25"}},
                    {"type": "text", "attrs": {"selector": "note"},
                     "source": {"default": "Efficient filtration, headband design"}}
                ]
            }
        ]
    }));

    let Accrete::tiles(t) = &grid else {
        panic!("not tiles: {grid:?}")
    };
    let attrs = t.attrs.as_ref().expect("attrs");

    let fields = attrs.fields.as_ref().expect("fields");
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0].key, "qty");
    assert_eq!(fields[0].title.as_deref(), Some(""));
    assert_eq!(fields[0].width.as_deref(), Some("4rem"));
    // Absent width/align stay absent: the renderer's defaults, not a value the frame guessed.
    assert_eq!(fields[1].width, None);
    assert_eq!(fields[1].align, None);
    assert_eq!(
        fields[2].key, "price/retail",
        "the key is a path, not a name"
    );
    assert_eq!(fields[2].align, Some(accrete::Align::End));
    assert_eq!(
        fields[3].selectors.as_deref(),
        Some(&["muted".to_string(), "wrap".to_string()][..])
    );

    let header = attrs.header.as_ref().expect("header");
    assert_eq!(header.selector, "--goods-head");
    assert_eq!(header.rows, 2);

    // The row is a node of its own, and its fields are its children — one selector per field.
    let rows = t.children.as_ref().expect("children");
    assert_eq!(rows.len(), 1);
    let Accrete::entry(e) = &rows[0] else {
        panic!("not an entry: {:?}", rows[0])
    };
    assert_eq!(e.id.as_deref(), Some("853736"));
    assert_eq!(e.children.as_ref().map(|c| c.len()), Some(4));
}

/// `rows` is a closed set of two answers: a third would render as something nobody chose, so it is
/// refused at decode rather than at paint.
#[test]
fn a_header_of_three_rows_does_not_decode() {
    let bad = json!({
        "type": "tiles",
        "attrs": {
            "fields": [{"key": "qty"}],
            "header": {"selector": "head", "rows": 3}
        },
        "source": {"slot": "goods"}
    });
    let err = serde_json::from_value::<Accrete>(bad).expect_err("rows: 3 must not decode");
    assert!(err.to_string().contains("1 or 2"), "{err}");

    // 1 is the default and stays off the wire; 2 is the one value worth writing.
    let one = roundtrip(json!({
        "type": "tiles",
        "attrs": {"fields": [{"key": "qty"}], "header": {"selector": "head", "rows": 1}},
        "source": {"slot": "goods"}
    }));
    let wire = serde_json::to_value(&one).expect("encode");
    assert!(
        wire["attrs"]["header"].get("rows").is_none(),
        "the default row count is not written: {wire}"
    );
}

/// The whole `table` family is deleted: a frame from before ADR 0031 §4 is a frame, not a synonym.
#[test]
fn the_retired_table_names_do_not_decode() {
    for old in ["table", "thead", "tbody", "tr", "th", "td"] {
        let wire = format!(r#"{{"type":"{old}","children":[]}}"#);
        let err = serde_json::from_str::<Accrete>(&wire)
            .expect_err(&format!("`{old}` must not decode any more"));
        assert!(
            err.to_string().contains("unknown variant"),
            "`{old}`: {err}"
        );
    }
}
