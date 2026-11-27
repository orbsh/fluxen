//! Colours live in the token layer, once (ADR 0055).
//!
//! Two sheets ship: `core.css` (the framework) and `custom.css` (the skin). Outside the palette and the values
//! derived from it neither may write a colour literal — `oklch(0.7 0.12 182.49)` is a literal, while
//! `oklch(from var(--tone) l c h / 0.25)` and `color-mix(in oklab, var(--fg) 12%, transparent)` are derivations
//! and allowed. The colour field's bars are the stated exception: they draw the colour space itself (black to
//! white, the wheel), which is content, not chrome.
//!
//! The module that draws its own chrome (the chart's legend and panels) writes styles from JS and is covered
//! too: it may read `var(--ring)` / `var(--wash)` / `var(--surface)` and may not spell a colour out.

const CORE: &str = include_str!("../assets/core.css");
const CUSTOM: &str = include_str!("../assets/custom.css");

fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The token layer, cut off: what remains is every rule that has to live on tokens.
fn outside_the_tokens(text: &str) -> String {
    let Some(start) = text.find("@layer tokens {") else {
        panic!("the tokens layer is gone — this test has nothing to guard");
    };
    let mut depth = 0usize;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return text[start + i + 1..].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("the tokens layer never closes");
}

/// The colour field's bars: `--bar` draws the space itself, so its gradients are allowed to be literals.
fn without_the_bars(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("--bar:") {
        out.push_str(&rest[..start]);
        match rest[start..].find(';') {
            Some(end) => rest = &rest[start + end + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Balanced text after `open`, which is expected to end in `(`.
fn inside_parens(text: &str, open: usize) -> &str {
    let mut depth = 0usize;
    for (i, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open + 1..open + i];
                }
            }
            _ => {}
        }
    }
    &text[open + 1..]
}

fn literals_in(text: &str) -> Vec<String> {
    let text = without_comments(text);
    let mut found = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let colour_fn = ["oklch", "rgba", "rgb", "hsla", "hsl"]
            .iter()
            .find(|name| rest.starts_with(*name))
            .copied();
        if let Some(name) = colour_fn {
            let open = i + name.len();
            if text[open..].starts_with('(') {
                let inner = inside_parens(&text, open);
                // `from …` takes its colour from another value, and `var(…)` is a token: both are derivations.
                let derived = inner.contains("var(") || inner.contains("from ");
                if !derived {
                    found.push(format!("{name}({})", inner.trim()));
                }
                i = open + 1 + inner.len() + 1;
                continue;
            }
        }
        if rest.starts_with('#') {
            let run: String = rest[1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let is_hex =
                matches!(run.len(), 3 | 4 | 6 | 8) && run.chars().all(|c| c.is_ascii_hexdigit());
            if is_hex {
                found.push(format!("#{run}"));
                i += run.len() + 1;
                continue;
            }
        }
        // Advance by a whole character: the sheet is full of prose in comments, and a byte step can land
        // inside a multi-byte character.
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    found
}

#[test]
fn the_framework_sheet_paints_only_tokens() {
    let found = literals_in(&outside_the_tokens(CORE));
    assert!(
        found.is_empty(),
        "colour literals outside `@layer tokens` in core.css: {found:?}\n\
         Put the value in the token layer (palette, or a derivation of it) and read the token here."
    );
}

#[test]
fn the_skin_paints_only_tokens_but_its_bars_are_content() {
    let found = literals_in(&without_the_bars(CUSTOM));
    assert!(
        found.is_empty(),
        "colour literals outside `--bar` in custom.css: {found:?}\n\
         The bars draw the colour space itself; everything else has to come from a token."
    );
}

#[test]
fn the_chart_module_paints_only_tokens() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/modules/g2chart/index.js"
    );
    let Ok(source) = std::fs::read_to_string(path) else {
        panic!("the chart module is not where this test looks for it: {path}");
    };
    // Line comments are prose here (and hold measured colour values from the ADRs); the block stripper in
    // `literals_in` only knows `/* … */`.
    let source: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    // `oklchCss(…)` and the helpers build colours from numbers the reader chose — that is not chrome, it is the
    // value. Only the styles the module *writes* are guarded, and those are the ones with a colour function in
    // them; the builder's own uses are calls, not CSS text.
    let mut found: Vec<String> = literals_in(&source)
        .into_iter()
        .filter(|literal| {
            literal.starts_with("rgba") || literal.starts_with("rgb(") || literal.starts_with('#')
        })
        .collect();
    found.sort();
    found.dedup();
    assert!(
        found.is_empty(),
        "colour literals in the chart module: {found:?}\n\
         A module is not an exception: read var(--ring), var(--wash), var(--surface)."
    );
}
