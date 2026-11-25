//! Every shipped example is a frame the loader accepts. The examples are the documentation a reader
//! copies from, so one that no longer parses teaches the wrong wire — and nothing else catches it:
//! the loader's refusals (a `sink:` on a node that has none, a retired key, a stepper without a
//! window) are hand-written checks in `proto`, not serde errors, so a stale example simply stops
//! loading while the rest of the suite stays green. That is exactly what happened when `sink:` left
//! the button (ADR 0039 §2) and six examples kept it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn every_example_parses() {
    let dir = format!("{}/../../examples/yaml", env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no examples found in {dir}");
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let frame = stage::proto::parse_yaml_to_frame(&src)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            !frame.is_empty(),
            "{}: parsed to an empty frame",
            path.display()
        );
    }
}
