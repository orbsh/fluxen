//! The layout utilities are named after the CSS knob they turn, and the name has to agree with the value.
//!
//! `ac` / `ai` / `as` / `jc` / `ji` / `js` are the six flex knobs (`align-` and `justify-` across
//! content / items / self). The word after the dash is the CSS value — `start`, `center`, `between` — with one
//! compression allowed and only this one: `space-between` / `space-around` / `space-evenly` keep the word that
//! distinguishes them and drop the `space` nobody needs, because no other value ends in those three words.
//! So `jc-between` is right, `jc-space-between` is long, and `jc-s-between` is a shorthand for a shorthand —
//! the `-s-` says nothing a reader can use without already knowing the value it was cut from.
//!
//! This test exists because those six classes shipped misspelled (`evently`) and compressed (`-s-`) and nothing
//! noticed: no page used them, so no page broke.

const CORE: &str = include_str!("../assets/core.css");

/// The property each prefix stands for. A class whose prefix is not here is not a layout utility.
fn property_of(prefix: &str) -> Option<&'static str> {
    match prefix {
        "ac" => Some("align-content"),
        "ai" => Some("align-items"),
        "as" => Some("align-self"),
        "jc" => Some("justify-content"),
        "ji" => Some("justify-items"),
        "js" => Some("justify-self"),
        _ => None,
    }
}

/// The words a class may end in. `space-*` appears only as its tail.
const WORDS: [&str; 11] = [
    "start", "end", "center", "stretch", "baseline", "auto", "normal", "between", "around",
    "evenly", "legacy",
];

#[test]
fn every_layout_utility_names_a_knob_and_a_value() {
    let mut checked = 0usize;
    let lines: Vec<&str> = CORE.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        // A rule that shares its body with its children lists the class with a comma; both forms count.
        let raw = line.trim();
        if !raw.ends_with('{') && !raw.ends_with(',') {
            continue;
        }
        let Some(class) = raw.strip_prefix("&.") else {
            continue;
        };
        let class = class.trim_end_matches(['{', ',', ' ']);
        let Some((prefix, rest)) = class.split_once('-') else {
            continue;
        };
        let Some(property) = property_of(prefix) else {
            continue;
        };
        checked += 1;
        assert!(
            WORDS.contains(&rest),
            "`{class}`: `{rest}` is not a flex keyword — the class name has to be the value's own word, \
             with `space-` dropped for between / around / evenly and nothing else compressed"
        );
        // The declaration under it: the property the prefix claims, and the value the name claims.
        let body: Vec<&str> = lines[i + 1..]
            .iter()
            .take_while(|l| !l.trim().starts_with('}'))
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with("/*"))
            .collect();
        let declaration = body
            .iter()
            .find(|l| !l.ends_with('{') && !l.starts_with('&'))
            .copied()
            .unwrap_or("");
        assert!(
            declaration.starts_with(property),
            "`{class}` declares `{declaration}`, but `{prefix}` means `{property}`"
        );
        let expected = match rest {
            "between" | "around" | "evenly" => format!("space-{rest}"),
            "start" | "end" => format!("var(--{rest})"), // `start`/`end` differ between flex and grid
            other => other.to_string(),
        };
        assert!(
            declaration.contains(&expected),
            "`{class}` declares `{declaration}`, but its name promises `{expected}`"
        );
    }
    assert!(
        checked >= 28,
        "only {checked} layout utilities found — did the sheet change shape?"
    );
}
