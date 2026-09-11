# Fluxen

AI-native UI rendering library: Brick DSL + Leptos rendering + streaming merge + CBOR codec. Carved out of Fluxora (the event-bus/Gateway half became [Prism](../prism/); Fluxora itself is migrating to Leptos and keeps its name).

Design: [Fluxora 架构](../../.hermes/wiki/projects/fluxora-architecture.md) (wiki — Brick DSL, merge strategies, codec decisions all documented there).

## What moved in

- **Brick DSL** (`brick` / `brick_macro`): closed typed enum, `#[serde(tag = "type")]`, Bind system, JsonSchema validation for AI generation
- **Streaming merge** (`merge`): Replace / Concat / Delete strategies, `Vec<String>` fragment buffering
- **Codec** (`codec`): `ActiveCodec` enum dispatch (Json/CBOR), URL-param handshake pinning, `encode_ws()` helper

## Position

Rendering layer only — no event bus, no gateway, no transport. Upstream (Prism / Aura realm / any producer) sends Brick operations; Fluxen renders and merges. The thick-shell principle stays: AI generates schema-validated structured JSON (content + structure), the framework owns styling and rendering determinism.
