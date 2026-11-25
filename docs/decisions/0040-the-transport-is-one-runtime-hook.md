# 0040 — The transport is one runtime hook, not a second artifact

Status: **Accepted** (2026-11-25). Not implemented yet.

---

## 1. The problem: a host that needs a transport we do not ship

`Transport` is two methods (`emit`, `on`) over raw bytes, and the trait is 31 lines. But the only way to use
something other than our WebSocket is to write a **Rust** boot: implement the trait and copy `mount()`'s assembly
(`#main` attributes, the codec, `Ctx::new`, `mount_to`, `window.fluxen.send`) — 169 lines, every host, every time.

The transports people actually ask for are the ones a browser `WebSocket` cannot express: POST + SSE, a socket
shared with the rest of the page, auth headers, retry and backoff policies the host already owns, an existing
client library, HTTP/3. Every one of those lives naturally in the host's **JavaScript**.

So the question: how does a JS host take over the transport without forking the boot?

## 2. The tempting wrong answer: a second artifact

Compile `ui-core` into a "core wasm" that exports a `#[wasm_bindgen]` surface and let JS boot it. Rejected, for
four reasons that all outlive the first release:

- **It does not actually move the boot out of Rust.** `Ctx::new` must run inside the Leptos executor (`mount_to`'s
  closure; calling it earlier panics with "before a global executor was initialized"). So the exported entry point
  *is* a boot: it creates the reactive root, mounts it, publishes the page handle. JS would own the transport, not
  the boot — the same thing the hook gives, plus an ABI.
- **An ABI is a version contract.** Every core change that touches the exported surface (or the byte-passing shape)
  breaks every JS boot written against it, and the two artifacts have to be released in step.
- **The codec would have two homes.** `content::codec` encodes and decodes in Rust; a JS boot either receives the
  exported codec (another ABI) or reimplements it (two implementations of one wire, drifting).
- **Two artifacts, two test matrices.** The same source built twice, the same gates run twice, two release flows —
  and the artifact is not small: the whole bundle is 2.86 MB of wasm today, and the boot is only 185 lines of it,
  so the second artifact is the second 2.86 MB.

And the only thing a second artifact buys over the hook is third-party boot writers — a consumer nobody has.

## 3. Decision: one optional hook, read once, before mount

The artifact stays one. The boot looks for a global before it builds anything:

```
window.fluxenTransport = {
  emit(bytes: Uint8Array): void        // one uplink frame, already encoded
  subscribe(push: (bytes: Uint8Array) => void): void   // the boot hands you the sink; you push every downlink frame
  close?(): void
}
```

Present → the boot wraps it as a `Transport` and mounts. Absent → `WsTransport::new(url, codec)` exactly as today,
zero JS, zero extra crossings.

Five rules, all of them deliberate:

- **Read once, at mount.** The transport is chosen before `Ctx::new`, because `Ctx::new` consumes it. There is no
  "swap the transport later": replacing a transport under a live reactive root means replaying state we do not keep.
- **Bytestream only.** The hook moves bytes. `ActiveCodec` still lives in Rust and still comes from
  `#main[data-codec]` / `?codec=` (default CBOR); the boot encodes uplink and decodes downlink. A hook that also
  chose the codec would make "wrong bytes, blank page" the default failure.
- **Present but malformed is a mount error**, named in the console. Never a silent fallback to our WebSocket: a page
  whose JS believes it owns the socket while wasm opens its own is the worst of both worlds, and it fails invisibly.
- **No reconnect inside the framework.** The hook owns the socket, so it owns retry, backoff and resumption — the
  framework never retries either (the WS transport has no retry today). What the framework does on a closed
  transport is what it does on a closed socket: the page stays, the connection does not.
- **`window.fluxen.send` is published in both modes.** Page code does not know which transport is running; the
  handle is the same leg into `Ctx::send`.

## 4. What it costs

- **One crossing per frame in hook mode.** Downlink: JS → wasm. Uplink: wasm → JS. The stock mode keeps zero, which
  is why this is a hook and not a rewrite: the default path pays nothing for the option existing.
- **A public contract.** `window.fluxenTransport` becomes documented API with a test, and the field is read in the
  boot, which is a place that used to be entirely ours.
- **A second mode in the gates.** Every gate that mounts a page runs against both transports; the JS one can be a
  tiny in-page loopback, so the cost is one harness, not a second suite.

## 5. What this is not

- **Not "bring your own server".** The host still speaks this document's wire: the same action list down, the same
  `Outflow` up. The hook changes who carries the bytes, not what they mean.
- **Not a retry/backoff framework.** See §3.
- **Not a change for the hosts we already have.** Prism and reflux mount the stock boot with no hook: the code path
  they exercise is byte-for-byte the one that exists today.
- **Not the "core + own boot" row.** That row is a host that links `ui-core` and writes its own boot in Rust; this is
  a host that keeps our boot and replaces only the transport. Two rows, one of them new — the table in
  `external-integration` gains it rather than pretending one covers both.