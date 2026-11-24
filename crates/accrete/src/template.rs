//! A registered stamp: the definition is a **structure**, not text (ADR 0037).
//!
//! The old form was `name → JSON text with {{key}} holes`: nothing was validated at registration, and instantiation
//! did one round of text substitution then parsed. One quoted phrase broke the JSON, whether a hole was a variable was all context, and there was nothing checkable at registration.
//!
//! Now it lands in one step: the tree is parsed, typed and its holes counted at registration (every `$name` must be
//! in `params`, and every name in `params` must appear in the tree); instantiation only swaps holes for values — whole-value
//! replacement, no text assembly. A value is a variable only when it **is** the string `"$name"` (the quotes are just JSON
//! syntax; the whole value is the variable), and `$$name` escapes "truly starting with `$`". So `"共 $q 件"` is a literal and never a variable.

use super::{Accrete, AccreteOps};
use std::collections::HashMap;
use serde_json::{Map, Value};

/// A stamp definition: the parsed, validated tree + the names it takes.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    params: Vec<String>,
    body: Value,
}

/// The registry: name → definition. Parsing happens exactly once, here (at registration); afterwards every instance clones from this tree.
#[derive(Debug, Clone, Default)]
pub struct Templates(HashMap<String, Template>);

impl Templates {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.0.get(name)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Register a definition: `params` are the names it takes, `body` is this tree.
    ///
    /// Everything checkable at registration is checked: every `$name` in the tree must be in `params` (otherwise a typo found only at
    /// instantiation becomes a silent empty value); every name in `params` must appear in the tree (declared and unused is a
    /// parameter nobody ever passes); and **after the holes are removed** every remaining piece must still be a valid Accrete node —
    /// node type, attribute vocabulary and required fields are settled here, and parsing happens only this once. The holes themselves
    /// are not checked: the whole value goes in, so anything can go in (which is the other face of "no shape check").
    ///
    /// The old `{{key}}` text form is refused outright: silently interpreting it as the new form is the worse ending — the frame author thinks they are
    /// using holes but actually gets a literal.
    pub fn register(&mut self, name: &str, params: &[&str], body: &Value) -> Result<(), String> {
        let declared: Vec<String> = params.iter().map(|p| (*p).to_string()).collect();
        let template = Template::new(declared, body)
            .map_err(|e| format!("template {name:?} rejected: {e}"))?;
        self.0.insert(name.to_string(), template);
        Ok(())
    }
}

impl Template {
    fn new(params: Vec<String>, body: &Value) -> Result<Self, String> {
        if let Value::String(_) = body {
            return Err(OLD_TEXT_FORM.into());
        }
        // ① After the holes are removed, is this tree still the tree of the frame language.
        let Some(skeleton) = skeleton(body) else {
            return Err("body is one hole — a definition is a node tree it can be spliced into".into());
        };
        serde_json::from_value::<Accrete>(skeleton)
            .map_err(|e| format!("body is not a node tree: {e}"))?;
        // ② The holes on the tree and the declared names must match one to one.
        let mut used = Vec::new();
        collect(body, &mut used);
        if let Some(missing) = used.iter().find(|u| !params.contains(u)) {
            return Err(format!("`${missing}` is used but not declared in params"));
        }
        if let Some(unused) = params.iter().find(|p| !used.contains(p)) {
            return Err(format!("`{unused}` is declared but never used"));
        }
        Ok(Self {
            params,
            body: body.clone(),
        })
    }

    /// Instantiate: swap every hole for the same-named value in `data`, whole-value replacement.
    ///
    /// Values get no shape/type check — a value is exactly what is put into that position (`$items` standing in `children`
    /// receives a run of nodes). The only thing that errors is **a missing name**: the hole has no value to swap in, and a template should not
    /// invent an empty value of its own.
    pub fn instance(&self, data: &Map<String, Value>) -> Result<Accrete, String> {
        let mut body = self.body.clone();
        fill(&mut body, data)?;
        serde_json::from_value(body).map_err(|e| format!("instance is not a node tree: {e}"))
    }

    /// The names it takes (validated at registration: exactly the holes on the tree).
    pub fn params(&self) -> &[String] {
        &self.params
    }
}

/// The wording of the text-form refusal: say **what to do**, not merely "this one is not allowed".
const OLD_TEXT_FORM: &str = "a template body is a node tree, not a `{{key}}` JSON string — \
                             write the tree and put `\"$name\"` (whole value) where a value goes";

/// Whether a string is a variable, an escaped literal, or a literal.
///
/// - `"$name"` → a variable (the whole value is the variable; the name is an identifier);
/// - `"$$name"` → the literal `"$name"` (text that truly starts with `$` has only this spelling);
/// - anything else → a literal, including a string like `"共 $q 件"` with a hole mid-sentence — a variable must fill the whole value.
fn variable(s: &str) -> Option<&str> {
    let name = s.strip_prefix('$')?;
    if name.starts_with('$') {
        return None;
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    chars
        .all(|c| c.is_alphanumeric() || c == '_')
        .then_some(name)
}

/// Removing the holes from the tree: the key (or array element) the hole sits in disappears entirely, everything else stays as it was.
///
/// What is parsed at registration is exactly this "tree without holes" — a hole standing somewhere means something is missing there, and "something
/// missing" is still a valid node; the hole itself is not checked (the whole value goes in, the caller decides the shape). If a hole occupies the
/// definition's root, there is no shell left to remove: `None`.
fn skeleton(v: &Value) -> Option<Value> {
    match v {
        Value::String(s) => variable(s).is_none().then(|| v.clone()),
        Value::Array(a) => Some(Value::Array(a.iter().filter_map(skeleton).collect())),
        Value::Object(m) => Some(Value::Object(
            m.iter()
                .filter_map(|(k, x)| skeleton(x).map(|x| (k.clone(), x)))
                .collect(),
        )),
        _ => Some(v.clone()),
    }
}

/// The holes on the tree, deduplicated in order of appearance.
fn collect(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if let Some(name) = variable(s)
                && !out.iter().any(|x| x == name)
            {
                out.push(name.to_string());
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect(x, out)),
        _ => {}
    }
}

/// Whole-value replacement: the value the hole sits at is swapped, not one byte of the surrounding string is touched (there is no "surrounding").
fn fill(v: &mut Value, data: &Map<String, Value>) -> Result<(), String> {
    match v {
        Value::String(s) => {
            if let Some(name) = variable(s) {
                match data.get(name) {
                    Some(x) => *v = x.clone(),
                    None => return Err(format!("missing value for `${name}`")),
                }
            } else if s.starts_with("$$") {
                // One `$` is the escape, not two: `$$name` is the literal `$name`.
                *s = s[1..].to_string();
            }
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                fill(x, data)?;
            }
        }
        Value::Object(m) => {
            for x in m.values_mut() {
                fill(x, data)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl Accrete {
    /// Depth-first expansion of this node's and its descendants' `stamp`, in place.
    /// Returns warning strings (unregistered name, missing value, instance is no longer a node) — accrete has zero logging dependencies,
    /// the caller decides to log a warn (aligned with ADR 0005's miss-warn-drop convention).
    /// A failed node keeps its `stamp` shape, and the render side has a fallback placeholder.
    pub fn expand(&mut self, env: &Templates) -> Vec<String> {
        let mut warns = Vec::new();
        self.expand_into(env, &mut warns);
        warns
    }

    fn expand_into(&mut self, env: &Templates, warns: &mut Vec<String>) {
        if let Accrete::stamp(r) = self {
            match env.get(&r.name) {
                Some(t) => match t.instance(&r.data) {
                    Ok(x) => *self = x,
                    Err(e) => {
                        warns.push(format!("stamp {:?}: {e}", r.name));
                        return;
                    }
                },
                None => {
                    warns.push(format!("stamp {:?} not registered", r.name));
                    return;
                }
            }
        }
        // The expansion result may itself still carry stamps (a template inside a template), so keep descending uniformly.
        if let Some(cs) = self.borrow_children_mut() {
            for c in cs {
                c.expand_into(env, warns);
            }
        }
    }
}