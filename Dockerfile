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

# Six build inputs the repo does not carry — all gitignored (media addresses in
# assets/*/README.md, module recipes in the Cargo.toml / module headers):
#
#  * the glTF models the examples point at (`/assets/3dbrowser/*.glb`), taken
#    from the Khronos sample set at a pinned commit and sha256-checked, so a
#    moved upstream file fails the build instead of shipping silently;
#  * the 3dbrowser module (wasm + bindgen glue + index.js), built here rather
#    than shipped as an artifact;
#  * the g2chart module's deployed copy plus its runtime library g2.min.js;
#  * the editor module's deployed copy plus Monaco's AMD subset (npm tarball,
#    version + sha1 pinned);
#  * the forcegraph module's deployed copy plus 3d-force-graph (unpkg dist,
#    version + sha256 pinned — the wrapper falls back to the same URL);
#  * the truck splat scene, served from this repo's own `assets` release — the
#    scene's upstream home is a 590 MB zip, so the 29 MB file is mirrored there.
#
# `trunk build` then copies examples/renderer/assets/** into the served dist
# (index.html: `<link data-trunk rel="copy-dir" href="/assets">`), so all five
# must land before that step. The second splat scene (bonsai) is NOT fetched: a
# container serves the mesh, chart, editor and truck demos and leaves
# /assets/splatviewer/bonsai.ksplat absent.
ARG GLTF_SAMPLE_ASSETS_COMMIT=edc7c9e67c639d230715049ee31f9a96a6babbbe
# The mkdir is load-bearing: assets/3dbrowser/ is gitignored as a whole
# directory, so a clean checkout (CI's actions/checkout) does not contain it and
# `curl -o` dies with "curl: (23) client returned ERROR on write" — a write
# failure, which is not the same as a failed download.
RUN set -eu; \
    mkdir -p examples/renderer/assets/3dbrowser; \
    base="https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/${GLTF_SAMPLE_ASSETS_COMMIT}/Models"; \
    curl -fsSL "$base/MaterialsVariantsShoe/glTF-Binary/MaterialsVariantsShoe.glb" \
      -o examples/renderer/assets/3dbrowser/MaterialsVariantsShoe.glb; \
    curl -fsSL "$base/ToyCar/glTF-Binary/ToyCar.glb" \
      -o examples/renderer/assets/3dbrowser/ToyCar.glb; \
    echo "e1d7cb190382111e5a5b37b51e9a7f007f7eb2ab1b6185e0188e8d0a0d1265a7  examples/renderer/assets/3dbrowser/MaterialsVariantsShoe.glb" | sha256sum -c -; \
    echo "01a60862de55cd4b9f3acfab0b0def86451800f9c42467fcd61052c16cb9838c  examples/renderer/assets/3dbrowser/ToyCar.glb" | sha256sum -c -

RUN cd examples/modules/3dbrowser \
    && cargo build --release --target wasm32-unknown-unknown \
    && wasm-bindgen --target web --out-dir pkg \
         target/wasm32-unknown-unknown/release/browser3d.wasm \
    && cp pkg/browser3d.js pkg/browser3d_bg.wasm index.js \
         /app/examples/renderer/assets/3dbrowser/

# The chart module's runtime (ADR 0009): G2 is UMD, loaded by a <script> tag the
# wrapper injects, so it ships beside the wrapper. The wrapper itself is tracked
# source (examples/renderer/modules/g2chart/) — only its deployed copy is
# gitignored. The pinned version is recorded in that module header; it was
# recovered from the local file's sha256 (the repo had not recorded it).
ENV G2_VERSION=5.4.8
RUN mkdir -p examples/renderer/assets/g2chart \
    && cp examples/renderer/modules/g2chart/index.js \
          examples/renderer/modules/g2chart/cbor.js \
          examples/renderer/assets/g2chart/ \
    && curl -fsSL "https://unpkg.com/@antv/g2@${G2_VERSION}/dist/g2.min.js" \
         -o examples/renderer/assets/g2chart/g2.min.js \
    && echo "7e7d346cab68c002a889dc6145bfc1f9ae1391be82ce8d514fb106ef3a6e6412  examples/renderer/assets/g2chart/g2.min.js" | sha256sum -c -

# The editor module's runtime (ADR 0016): the wrapper is tracked source
# (examples/renderer/modules/editor/) — only its deployed copy is gitignored —
# and Monaco's AMD subset is unpacked from the pinned npm tarball, sha1-checked
# like every other upstream input here. min/vs is copied VERBATIM: editor.main.js
# and the *Mode chunks hard-code the worker files' content-hashed names
# (assets/editor.worker-<hash>.js etc.), so renaming or re-hashing anything
# breaks worker resolution at runtime. Same recipe as the module header's.
# The four language-service workers (css/html/json/ts = 9.12 MB of the 15.7 MB)
# are then deleted per ADR 0016 decision 5 — they are only fetched for
# IntelliSense in those four languages, the review surface needs the tokenizer
# highlight (basic-languages, no worker), and this prune is what keeps the image
# at ≈6.3 MB; editor.worker stays. The prune is asserted (exactly one worker file
# left) so a glob that stops matching — a monaco bump renaming the hashes —
# fails the build instead of silently shipping the 9 MB again.
ENV MONACO_VERSION=0.55.1
ENV MONACO_SHA1=e74c6fe5a6bf985b817d2de3eb88d56afc494a1b
RUN mkdir -p examples/renderer/assets/editor examples/renderer/assets/monaco \
    && cp examples/renderer/modules/editor/index.js \
          examples/renderer/modules/editor/cbor.js \
          examples/renderer/assets/editor/ \
    && curl -fsSL \
         "https://registry.npmjs.org/monaco-editor/-/monaco-editor-${MONACO_VERSION}.tgz" \
         -o /tmp/monaco.tgz \
    && echo "${MONACO_SHA1}  /tmp/monaco.tgz" | sha1sum -c - \
    && tar -xzf /tmp/monaco.tgz -C /tmp \
    && cp -r /tmp/package/min/vs examples/renderer/assets/monaco/vs \
    && rm -f examples/renderer/assets/monaco/vs/assets/css.worker-*.js \
             examples/renderer/assets/monaco/vs/assets/html.worker-*.js \
             examples/renderer/assets/monaco/vs/assets/json.worker-*.js \
             examples/renderer/assets/monaco/vs/assets/ts.worker-*.js \
    && test "$(ls examples/renderer/assets/monaco/vs/assets/*.worker-*.js | wc -l)" = 1 \
    && rm -rf /tmp/monaco.tgz /tmp/package

# The forcegraph module's runtime (ADR 0017): 3d-force-graph is UMD (it exposes
# the global ForceGraph3D and bundles three.js itself), so the wrapper injects it
# with a <script> tag instead of importing it — that is also why the library
# never enters the app bundle. The wrapper is tracked source
# (examples/modules/forcegraph/) and only its deployed copy is gitignored.
# Version pinned + sha256-checked like every other upstream input here; the
# wrapper falls back to this same pinned unpkg URL if the same-origin file is
# missing (see the module header).
ENV FORCE_GRAPH_VERSION=1.80.1
RUN mkdir -p examples/renderer/assets/forcegraph \
    && cp examples/modules/forcegraph/index.js \
          examples/modules/forcegraph/cbor.js \
          examples/renderer/assets/forcegraph/ \
    && curl -fsSL \
         "https://unpkg.com/3d-force-graph@${FORCE_GRAPH_VERSION}/dist/3d-force-graph.min.js" \
         -o examples/renderer/assets/forcegraph/3d-force-graph.min.js \
    && echo "13088340aad408156064fd120c3bc3a47c93a3cc45b8f132d6284b20e2223be9  examples/renderer/assets/forcegraph/3d-force-graph.min.js" | sha256sum -c -

# The truck scene, mirrored in this repo's own `assets` release (why: the scene's
# upstream home is a 590 MB multi-scene zip — see assets/splatviewer/README.md).
ARG FLUXEN_ASSETS_RELEASE=assets
RUN curl -fsSL \
      "https://github.com/orbsh/fluxen/releases/download/${FLUXEN_ASSETS_RELEASE}/truck.ksplat" \
      -o examples/renderer/assets/splatviewer/truck.ksplat \
    && echo "e8ed0a5cc03fe7d91c5cea7a53c72e0f3a8bdd805157bee24def1ab059b23654  examples/renderer/assets/splatviewer/truck.ksplat" | sha256sum -c -

# Dev gateway binary.
RUN cargo build --release -p stage

# WASM UI (trunk release build -> examples/renderer/dist).
RUN cd examples/renderer && trunk build --release

FROM debian:bookworm-slim AS runtime

COPY --from=builder /app/target/release/stage /usr/local/bin/stage
COPY --from=builder /app/examples/renderer/dist /app/dist

EXPOSE 3002

# -it expected: serve drops into the interactive console (frame monitor + /send).
ENTRYPOINT ["/usr/local/bin/stage", "serve", "--dist", "/app/dist"]
