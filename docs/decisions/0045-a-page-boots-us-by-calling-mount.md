# 0045 — A page boots us by calling `mount`, and by saying nothing else

Status: **Accepted** (2026-11-25). Implemented: `crates/ui_leptos/src/settings.rs` (the resolver),
`crates/ui_leptos/src/lib.rs` (`mount(element, options)` and the `#fluxen` default target),
`crates/ui_leptos/index.html`. Supersedes the `data-*` vocabulary that ADR 0021 and ADR 0044 used to carry.

---

## 1. The problem: one setting, two places to say it

The boot read its configuration from attributes on `#main` — `data-host`, `data-path`, `data-codec`,
`data-token`, `data-config`, `data-transport`. That was fine while attributes were the only door. Adding a
JavaScript entry (`mount(element, options)`) puts the same six settings in a second place, and then the
interesting question is no longer "what are the settings" but "**which of the two wins when they disagree**".

Every answer to that question costs:

- a precedence rule per field (three sources if the URL's `?token=`/`?codec=` also count),
- a conflict report, because two sources of one truth that silently disagree is exactly the failure this
  repository keeps rejecting — a page that reads its own HTML and its own script and believes both,
- a rule for what an empty attribute means, another for what an absent one means, and a test for each pair,
- documentation of all of it, in two languages.

None of that machinery exists to do anything a page wants. It exists to keep two doors from contradicting
each other. **So there is one door.**

## 2. Decision: configuration is an argument, not an attribute

    <div id="app"></div>
    <script type="module">
      import init, { mount } from '/static/vendor/fluxen/ui_leptos-<hash>.js';
      await init('/static/vendor/fluxen/ui_leptos-<hash>_bg.wasm');   // the glue wants the wasm's own path
      await mount(document.querySelector('#app'), {
        host: 'prism.d', path: '/channel', codec: 'json', token,
        config: 'options',                        // '' means: this page has no option slot
        policy: { heartbeat: 5000, dead: 10000 },  // the shape ADR 0044 §2 defines
        transport: window.mySocketHook,            // the hook of ADR 0040, by value
      });
    </script>

Two sources remain, and the order between them is not a rule but a fact: **the call over the address**. A
producer that hands out a URL can put `?token=` and `?codec=` on it (it may not be able to run this script);
the page that calls `mount` is more specific about its own boot, so its fields win — quietly, because an
override is not a disagreement. A field the call omits keeps the address's word.

What is gone: `data-host`, `data-path`, `data-codec`, `data-token`, `data-config`, `data-transport`, and with
them the whole notion of "these two must be reconciled". The resolver is 40 lines of arithmetic over plain
data with no DOM in it (`settings::Boot::resolve`), and its tests need no browser.

The hook moves with the options. `transport` is read off the same object before the rest is deserialized,
wrapped by the same `hook::wrap` that serves `window.fluxenTransport`, and refused the same way when it is the
wrong shape. `window.fluxenTransport` still works — it is not configuration, it is the host taking the socket
over, and a page that does that does not want to pass an object through a mount call.

## 3. `#fluxen`, not `#main`

One thing still has to be found without being called: a page that wants the defaults and no script. The id for
that is **ours** — `#fluxen` means "mount me with whatever the defaults are" — and it is deliberately not
`#main`:

- `#main` is the **host's** landmark (the HTML `main` region). Taking it as our magic id means a host cannot
  use the standard id for its own layout without silently booting a renderer inside it.
- With the id namespaced, there is no collision to resolve and therefore no `__fluxenAutoMount` opt-out flag:
  a page with `#fluxen` gets the default boot, a page with any other id that calls `mount` gets exactly what
  it asked for. Nothing to arbitrate, nothing to warn about.

A page with `#fluxen` can still pass the two address parameters (`?codec=`, `?token=`); anything else is said
in a `mount` call.

## 4. Why this is not "requiring JavaScript"

It is not a new dependency on scripting: the renderer IS wasm, which is a script, which drives the DOM through
JS APIs. A page that cannot run JavaScript cannot run this renderer at all, so "a page with no JS" was never a
class of consumer — it was a class of *boilerplate*, and the five lines above are assembly, not logic. What the
attributes really bought was "the host does not have to write those five lines", at the price of a second
configuration surface for every setting.

## 5. Consequences

- `reflux`'s page (`www/index.html`) currently relies on the old contract: `<div id="main" data-codec="json"
  data-path="/ws">` with no script. It becomes `<div id="main">` plus a five-line module that calls
  `mount(el, { codec: 'json', path: '/ws' })`, importing the vendored artifact by its pinned name. Its tests
  and gates keep asserting on `#main` — that id stays theirs, which is the point of §3.
- The boot's artifact is now the **library** (`cdylib`), not a binary: the same wasm carries the automatic
  `#fluxen` start and the exported `mount`. `crates/ui_leptos/src/main.rs` is gone.
- A host that calls `init` itself must hand it the wasm URL: trunk's hashed filename is only known to the page
  trunk generates, so `init()` with no argument asks for `ui_leptos_bg.wasm`, which does not exist (found the
  hard way — a 404 on the wasm looks exactly like "the module never ran"). The five lines above include it.
- `data-transport`'s vocabulary survives, but only as data: `policy` in the options is the same
  `transport::reconnect::TransportConfig` (§2), so `{"heartbeat": 0}` and `{"retry": false}` mean what ADR 0044
  §2 says they mean.
- A misspelled option key, an unusable value, or a self-contradicting policy is a **rejected mount** — the
  promise from `mount` rejects with the reason, the page's `catch` sees it, and nothing is mounted. Mount
  failures on the automatic path still land in the target element's text as well as the log (ADR 0044 §5).

## 6. Rejected

**Keep the attributes and define precedence.** This was implemented first (three sources, per-field
precedence, a warning whenever two of them disagreed) and then deleted. It works, and it is the most code this
repository has ever carried to answer a question no page asked: *which of my two configurations is the real
one*. Every field would need that rule forever, in every language the documentation is written in.

**Keep the attributes and let the call replace them wholesale.** Cheap to state, worse in practice: a page that
passes one option loses the attributes it did not touch, and the loss is silent — precisely the failure mode
warning machinery exists for.

**A `window.fluxenConfig` object read at start.** Same two-door problem with the winning source moved to JS, plus
an ordering question (the object must exist before the module runs) and a global that nothing declares.

**Auto-boot on `#main` with a `__fluxenAutoMount` opt-out.** Works, and puts a flag in the contract whose only
job is to undo the other half of the contract. `#fluxen` (§3) makes the two cases disjoint instead.

## 7. 验证（2026-11-25，真浏览器五页）

Every case below ran in a real browser against the built artifact. Five rather than one, because each exercises a
different half of the decision:

| 用例 | 通过条件 |
|---|---|
| `#fluxen` 默认路径 | 页面渲染出帧，没有任何 `mount` 调用 |
| `mount(#app, {codec:'json'})` | 页面渲染出帧，`data-*` 都不在场 |
| `policy: {heartbeat: 0}` | 静默 7 秒，零条 `__ping__` |
| 坏 codec | `mount` 的 promise 被拒，原因写明是哪一个值 |
| `transport` 按值传入 | 页面无 `__fluxenConn`，且能渲染 hook 推来的帧 |