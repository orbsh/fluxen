# 0037 — A stamp substitutes whole values

Status: **Accepted** (2026-11-19).

A `stamp` instantiates a registered definition. The definition used to be a JSON *string* with
`{{key}}` holes, spliced textually and re-parsed on every instantiation (ADR 0006). Three
complaints, all of them one complaint: a string is not a definition.

- A value that contains a quote breaks the body. The splice did not escape (it could not — it
  did not know whether the hole sat inside a string), so the producer had to know which holes
  were quoted and which were not, and getting it wrong was a parse error at runtime.
- Nothing was checkable at registration. The body is only a tree *after* a value has been
  spliced into it, so a typo in a hole's name, a typo in a node's type, and a missing brace all
  surfaced the same way: a half-rendered string that fails to parse, per instance.
- Every instance paid a text parse, and text is the one form the model cannot inspect.

## The decision

A definition is an Accrete node tree, parsed and checked once at registration. A variable is a
value that is **exactly** the string `"$name"` — the quotes are JSON's syntax, the whole value is
the variable. Substitution is whole-value, so there is no surrounding text to break.

```yaml
action: tmpl
name: label
params: [q]
data:
  type: text
  source: { default: "$q" }     # the whole value: whatever the caller supplies, as it is
```

A variable standing where children belong may take a list of nodes — `children: "$items"` is one
hole, and the list that lands there is the whole list. `params` grows by that one name.

`{"source": {"default": "共 $q 件"}}` is a **literal**, not a variable: a variable must be the
entire value. `"$$name"` is the escape for a literal that genuinely starts with `$` — one
branch, no nesting: `"$$q"` renders `$q`, and `"共 $q 件"` needs no escape at all because it is
not a variable in the first place.

## What is checked, and what is not

Registration validates everything static: every `$name` in the tree is declared in `params`,
every declared param is used, and — with the holes taken out — what remains is still a node tree
(node types, attribute vocabulary, required fields). Defining a node type as a variable is
therefore impossible, and a mistyped node fails at registration rather than at first use.

No runtime shape or type checking of the supplied value: it is dropped in as-is. A caller that
puts a node where a string was meant gets a node, and only the frame's own decode still has an
opinion. The one loud error is a **missing name**: instantiation fails and names the key it
wanted. A hole with no value has no honest default, and inventing an empty one would hide the
caller's mistake behind a blank cell.

## The rejected alternative

Keeping `{{…}}` and escaping properly — the shortest thing to write, and it stays the only
definition form that is not a tree. Quoting would become the substitute layer's business (it
would have to know JSON string contexts), every instance would still re-parse the text, and
registration would still have nothing to validate. That would fix the quoting bug and leave the
two costs that matter: a definition that cannot be checked, and a parse per instance.