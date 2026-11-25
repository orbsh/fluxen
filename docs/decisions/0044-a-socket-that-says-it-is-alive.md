# 0044 — A socket that says it is alive, and what to do when it does not

Status: **Accepted** (2026-11-25). Implemented; the conflict analysis in §4 is the part to read before
changing either side's numbers.

---

## 1. The problem: a page cannot tell "idle" from "gone"

Three failures, one shape:

- A laptop sleeps with the page open. On wake the WebSocket is half-open: the browser has not noticed, no
  `close` event has fired, and every uplink lands in the void.
- A NAT or a proxy drops the mapping. Same thing, slower: the socket stays "open" for minutes.
- A dev restarts the server. The page keeps rendering a live-looking page that will never be updated again,
  and the only way out is a manual refresh.

The server side already has answers (`reflux` pings at 10 s and reclaims devices silent for 30 s), but a
client cannot see any of that: **a browser cannot send a WebSocket `ping` from JS or wasm**, and the pong is
answered by the network stack, not by the page. `gloo`'s `WebSocket` exposes `open` / `message` / `close` /
`error` and nothing else. So the client's knowledge of its own link is exhausted the moment the connection
opens — which is why a page that stops receiving cannot say that it stopped.

## 2. Decision: ask in the wire's language, and give up on our own schedule

Reserved dests, one pair, in `content`:

    __ping__    uplink      the page asking "are you there"
    __pong__    downlink    the host answering "yes"

They are not events: the transport swallows the pong before any decoder sees it, and the host answers the
ping before dispatch — no handler, no slot, no refusal counter, no log line (§3). The `__`-wrapped spelling is
what keeps a page event from colliding with them by accident.

Three rules on the client side:

- **Probe after `probe_ms` of inbound silence** — 5 s by default. Silence is the whole problem, so only an idle
  page probes: a page receiving frames is already proving the link works.
- **Declare the link dead after `dead_ms`** — 10 s by default, twice the probe. On death the client closes the
  socket *itself* rather than waiting for `close`; a half-open connection will not produce one.
- **Reconnect with exponential backoff**: 500 ms doubling to a 10 s cap, jittered downward by up to 20 % so a
  fleet recovering from one restart does not knock in lockstep. The first retry is fast enough to feel
  automatic (a dev restarts the server); the cap is the longest a page should take to notice the server is
  back, because a minute of waiting is polite to a server that is down and rude to a reader whose server came
  back ten seconds ago.

The whole policy is one page attribute, `data-transport`, holding an object — not one `data-*` per number,
because these numbers are one decision and a template that grows a name per knob stops being readable:

```text
data-transport='{"heartbeat": 5000, "dead": 10000, "retry": {"base": 500, "max": 10000, "jitter": 20}}'
data-transport='{"heartbeat": 0}'    the heartbeat is off: no probes, and no closing on our own guess
data-transport='{"retry": false}'    reconnection is off: one connection, then `closed`
```

A policy that does not parse — unknown key, bad JSON, dead window inside the probe window — is a mount error,
and the message lands in `#main` as well as the log: a blank page plus a devtools line is how a configuration
mistake gets read as a renderer bug.

`heartbeat` is the probe interval (`0` = off), `dead` the silence that counts as gone (`0` = twice the probe,
so the pair cannot disagree by accident), `retry` a switch or a schedule. Absent means the defaults above;
unknown keys, unparseable JSON, and a dead window inside the probe window are **mount errors that name
themselves** — a page quietly running a different policy than its HTML says is the failure this whole section
exists to prevent. A page with the heartbeat off is a page that would rather show a stale view than close a
socket on its own guess; a page with retry off is a single-session page, and the transport says `closed`
instead of pretending it is about to come back.

Uplinks written while the socket is down are queued and flushed on open, bounded at 256 frames — a page that
was offline for a minute must not replay a minute of typing into a fresh session.

## 3. Who retries

Retry belongs to whoever owns the socket, and there are exactly two owners:

- **Hook mode** (`window.fluxenTransport`, ADR 0040): the host wrote the transport, so the host retries. The boot
  does not wrap a hook in our backoff loop — it cannot know whether the hook's `emit` is a socket, an SSE
  stream, or a queue.
- **Default mode**: `LiveWs` owns the socket, so `LiveWs` retries. This is why ADR 0040 §3's "the framework
  never retries" still holds: the `Transport` trait has no retry, and `Ctx` has no retry; the retry is in the
  transport implementation, one layer below the framework and invisible to it.

The framework's contract is unchanged either way: a closed transport means a page that stays and a connection
that does not. What the page *renders* is the host's business — both hosts re-send on connect.

## 4. The server also has a heartbeat: does it conflict?

Mechanically, no. Two layers, disjoint:

- **Protocol-level ping/pong** (a WebSocket frame): the server's job, answered by the browser's network stack.
  It keeps `last_seen` fresh, so a client whose *main thread* is busy for seconds is still considered alive —
  which is correct, and which no page-level code could achieve.
- **Application-level `__ping__`/`__pong__`** (an `Outflow` in our own codec): the page's job, asking about the
  link in the direction the protocol pings cannot answer — "have I heard from *you* lately".

What does need reconciling is the **order of giving up**, because two clocks judging one link can disagree in a
way that produces duplicates:

- If the server gives up **first**, it reclaims a device whose page is about to reconnect, and the page has no
  way to know: it sits on a socket the server considered dead.
- If the client gives up **first by a wide margin**, the server holds the abandoned device for the whole
  window, and after the reconnect there are two devices for one page — duplicated sends, duplicated state.

So the rule: **the client gives up first, and the server's window is a few client windows long.** Today that is
client 10 s dead vs server 30 s dead / 10 s ping (`reflux.app.App(heartbeat=10.0, dead_after=30.0)`) — the
client acts after one missed pong round, the server after three of its own ticks. The server also treats the
probe as ordinary traffic (any byte refreshes `last_seen`), so a client that is probing is never reclaimed.

The one thing that must not happen is the probe becoming *app* traffic: if `__ping__` reached a handler, every
application would need to know about a frame it did not write, and if it reached the refusal counter, `/status`
would report a broken client that is working perfectly.

## 5. What it costs

- One uplink per idle window (~5 s), a few dozen bytes, plus one pong. Nothing when the page is busy.
- A frame that is not a render instruction on the wire, so both servers grow one branch: `stage` answers in the
  encoding it arrived in, `reflux` answers on the socket (`Device.channel.send`) rather than through the frame
  door, because an answer is not a document.
- A page-level global for tests: `window.__fluxenConn = {state, attempts, framesIn, framesOut, dropped,
  lastInbound}` (`lastInbound` is absolute, so a reader computes the silence against its own clock). Not API —
  it is how a browser test can see the socket's own opinion.

## 6. What this is not

- **Not a resync protocol.** The page keeps what it has; the host re-sends what the page needs on connect. A
  framework that replayed state would have to keep the state.
- **Not a replacement for the server's ping.** The protocol ping proves the client's stack is there even when
  the page is blocked; ours proves the server is there even when the page is idle. Neither can do the other's
  job.