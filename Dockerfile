# fluxen dev image: stage serve (mirror + UI + console REPL) with trunk-built dist.
# Run interactively — the console REPL is part of the product:
#   docker run -it --rm -p 3002:3002 ghcr.io/orbsh/fluxen
# amd64 only — trunk/wasm tooling runs natively; cross-arch would need QEMU.

FROM rust:1.98-slim AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown

# Prebuilt trunk (pinned to the version used in dev).
ENV TRUNK_VERSION=v0.21.14
RUN curl -fsSL \
      "https://github.com/trunk-rs/trunk/releases/download/${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
      | tar -xz -C /usr/local/bin

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Dev gateway binary.
RUN cargo build --release -p stage

# WASM UI (trunk release build -> crates/ui_leptos/dist).
RUN cd crates/ui_leptos && trunk build --release

FROM debian:bookworm-slim AS runtime

COPY --from=builder /app/target/release/stage /usr/local/bin/stage
COPY --from=builder /app/crates/ui_leptos/dist /app/dist

EXPOSE 3002

# -it expected: serve drops into the interactive console (frame monitor + /send).
ENTRYPOINT ["/usr/local/bin/stage", "serve", "--dist", "/app/dist"]
