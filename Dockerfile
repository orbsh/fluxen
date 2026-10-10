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

# Prebuilt wasm-bindgen CLI, pinned to the module's wasm-bindgen dependency
# (examples/modules/3dbrowser/Cargo.lock): the JS glue and the wasm it is
# generated for must come from the same version.
ENV WASM_BINDGEN_VERSION=0.2.129
RUN curl -fsSL \
      "https://github.com/rustwasm/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
      | tar -xz --strip-components=1 -C /usr/local/bin \
          "wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl/wasm-bindgen"

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY examples/modules ./examples/modules

# Three build inputs the repo does not carry — all gitignored (media addresses
# in assets/3dbrowser/README.md, module recipes in the Cargo.toml / module
# headers):
#
#  * the glTF models the examples point at (`/assets/3dbrowser/*.glb`), taken
#    from the Khronos sample set at a pinned commit and sha256-checked, so a
#    moved upstream file fails the build instead of shipping silently;
#  * the 3dbrowser module (wasm + bindgen glue + index.js), built here rather
#    than shipped as an artifact;
#  * the g2chart module's deployed copy plus its runtime library g2.min.js.
#
# `trunk build` then copies crates/ui_leptos/assets/** into the served dist
# (index.html: `<link data-trunk rel="copy-dir" href="/assets">`), so all three
# must land before that step. The splat scenes are deliberately NOT fetched:
# their only upstream source is a 590 MB zip, so a container serves the mesh and
# chart examples and leaves /assets/splatviewer/*.ksplat absent.
ARG GLTF_SAMPLE_ASSETS_COMMIT=edc7c9e67c639d230715049ee31f9a96a6babbbe
RUN set -eu; \
    base="https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/${GLTF_SAMPLE_ASSETS_COMMIT}/Models"; \
    curl -fsSL "$base/MaterialsVariantsShoe/glTF-Binary/MaterialsVariantsShoe.glb" \
      -o crates/ui_leptos/assets/3dbrowser/MaterialsVariantsShoe.glb; \
    curl -fsSL "$base/ToyCar/glTF-Binary/ToyCar.glb" \
      -o crates/ui_leptos/assets/3dbrowser/ToyCar.glb; \
    echo "e1d7cb190382111e5a5b37b51e9a7f007f7eb2ab1b6185e0188e8d0a0d1265a7  crates/ui_leptos/assets/3dbrowser/MaterialsVariantsShoe.glb" | sha256sum -c -; \
    echo "01a60862de55cd4b9f3acfab0b0def86451800f9c42467fcd61052c16cb9838c  crates/ui_leptos/assets/3dbrowser/ToyCar.glb" | sha256sum -c -

RUN cd examples/modules/3dbrowser \
    && cargo build --release --target wasm32-unknown-unknown \
    && wasm-bindgen --target web --out-dir pkg \
         target/wasm32-unknown-unknown/release/browser3d.wasm \
    && cp pkg/browser3d.js pkg/browser3d_bg.wasm index.js \
         /app/crates/ui_leptos/assets/3dbrowser/

# The chart module's runtime (ADR 0009): G2 is UMD, loaded by a <script> tag the
# wrapper injects, so it ships beside the wrapper. The wrapper itself is tracked
# source (crates/ui_leptos/modules/g2chart/) — only its deployed copy is
# gitignored. The pinned version is recorded in that module header; it was
# recovered from the local file's sha256 (the repo had not recorded it).
ENV G2_VERSION=5.4.8
RUN mkdir -p crates/ui_leptos/assets/g2chart \
    && cp crates/ui_leptos/modules/g2chart/index.js \
          crates/ui_leptos/modules/g2chart/cbor.js \
          crates/ui_leptos/assets/g2chart/ \
    && curl -fsSL "https://unpkg.com/@antv/g2@${G2_VERSION}/dist/g2.min.js" \
         -o crates/ui_leptos/assets/g2chart/g2.min.js \
    && echo "7e7d346cab68c002a889dc6145bfc1f9ae1391be82ce8d514fb106ef3a6e6412  crates/ui_leptos/assets/g2chart/g2.min.js" | sha256sum -c -

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
