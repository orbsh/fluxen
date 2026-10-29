#!/usr/bin/env python3
"""Cut a renderer release out of a trunk build: four files, a manifest, and a tarball.

    python3 scripts/ui_dist.py                  # build, then assemble into dist-release/
    python3 scripts/ui_dist.py --no-build       # assemble whatever examples/renderer/dist holds
    python3 scripts/ui_dist.py --tag v0.1.0 --out /tmp/rel

What ships, and what deliberately does not:

- **the four core files** — the JS glue, its wasm, `assets/main.css`, `assets/custom.css`. That is
  everything a host page needs; the hashed names are trunk's and stay the only names (a `fluxen.js`
  alias would be a second name for the same bytes, and another 2.4 MB in every release).
- **not** `assets/3dbrowser` (28 MB), `assets/splatviewer` (48 MB), `assets/monaco` (6.6 MB),
  `assets/mermaid.min.js` (2.6 MB), `assets/g2chart` (1.1 MB). Those are module/demo media, fetched
  only when a frame names them, and they are built per deployment (see the Dockerfile). Attaching
  them would make every tag ~86 MB for bytes most consumers never request.

`manifest.json` is fluxen-shaped (`entry` names the JS and wasm, `files` carries sha256 + size per
path). A consumer with its own layout — reflux keeps them under `www/static/vendor/fluxen/` and
prefixes the paths in its own `versions.json` — derives that file from this one; the prefix is the
consumer's business, not the renderer's.

Reproducible where it can be: entries sorted, uid/gid 0, empty uname/gname, and the mtime comes from
`SOURCE_DATE_EPOCH` (else 0), for both the tar and its gzip header.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
import pathlib
import subprocess
import sys
import tarfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
DIST = ROOT / "examples" / "renderer" / "dist"
OUT = ROOT / "dist-release"
CSS = ("assets/main.css", "assets/custom.css")


def sh(*args: str, cwd: pathlib.Path = ROOT) -> str:
    out = subprocess.run(args, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if out.returncode != 0:
        tail = out.stderr.decode(errors="replace").strip().splitlines()[-8:]
        raise SystemExit(f"{' '.join(args)} failed:\n  " + "\n  ".join(tail))
    return out.stdout.decode().strip()


def build() -> None:
    """`trunk build --release` — the only build path this script knows."""
    print(f"[dist] trunk build --release in examples/renderer (trunk {sh('trunk', '--version')})", flush=True)
    out = subprocess.run(["trunk", "build", "--release"], cwd=ROOT / "examples" / "renderer")
    if out.returncode != 0:
        raise SystemExit("[dist] trunk failed — see its output above")


def entry_names(dist: pathlib.Path) -> tuple[str, str]:
    """(js, wasm) as trunk emitted them.

    The two file names are found independently, not derived from a shared stem: trunk hashes
    each artifact on its own, and which naming a build produces is the TOOL's business, not
    ours (0.21.14 emitted one stem for both, 0.22.0 does not). Exactly one of each in the dist
    is what makes it a release — a stale dist holding two builds is refused.
    """
    wasms = sorted(dist.glob("ui_leptos-*_bg.wasm"))
    js = sorted(p for p in dist.glob("ui_leptos-*.js") if not p.name.endswith("_bg.wasm"))
    if len(wasms) != 1 or len(js) != 1:
        raise SystemExit(
            f"[dist] expected exactly one ui_leptos-*.js and one ui_leptos-*_bg.wasm in {dist},"
            f" found {[p.name for p in js]} + {[p.name for p in wasms]}"
            " — a stale dist with two builds is not a release"
        )
    return js[0].name, wasms[0].name


def digest(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sources(dist: pathlib.Path) -> list[tuple[str, pathlib.Path]]:
    js, wasm = entry_names(dist)
    needed = [(js, dist / js), (wasm, dist / wasm)]
    needed += [(name, dist / name) for name in CSS]
    missing = [name for name, path in needed if not path.exists()]
    if missing:
        raise SystemExit(f"[dist] the bundle is incomplete, missing: {', '.join(missing)}")
    return needed


def tag_now() -> str:
    for var in ("GITHUB_REF_NAME",):
        value = os.environ.get(var, "")
        if value.startswith("v"):
            return value
    try:
        return sh("git", "describe", "--tags", "--exact-match")
    except SystemExit:
        return "dev"


def write(out: pathlib.Path, files: list[tuple[str, pathlib.Path]], tag: str, rev: str) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    js, wasm = entry_names(DIST)
    manifest = {
        "renderer": "fluxen",
        "tag": tag,
        "rev": rev,
        "built": "trunk build --release (examples/renderer)",
        "entry": {"js": js, "wasm": wasm},
        "files": {},
    }
    for name, path in files:
        target = out / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(path.read_bytes())
        manifest["files"][name] = {"sha256": digest(target), "size": target.stat().st_size}

    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    sums = "".join(f"{meta['sha256']}  {name}\n" for name, meta in manifest["files"].items())
    (out / "SHA256SUMS").write_text(sums, encoding="utf-8")

    stamp = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    members = [(name, out / name) for name in manifest["files"]] + [("manifest.json", out / "manifest.json")]
    raw = io.BytesIO()
    with gzip.GzipFile(fileobj=raw, mode="wb", mtime=stamp) as gz:
        with tarfile.open(fileobj=gz, mode="w", format=tarfile.PAX_FORMAT) as tar:
            for name, path in sorted(members):
                info = tarfile.TarInfo(name)
                info.size = path.stat().st_size
                info.mtime = stamp
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mode = 0o644
                with path.open("rb") as handle:
                    tar.addfile(info, handle)
    archive = out / f"fluxen-ui-{tag}.tar.gz"
    archive.write_bytes(raw.getvalue())
    return manifest


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="ui_dist", description="Cut a renderer release from a trunk build.")
    ap.add_argument("--no-build", action="store_true", help="use examples/renderer/dist as it stands")
    ap.add_argument("--out", default=str(OUT), help=f"where to assemble (default: {OUT.relative_to(ROOT)})")
    ap.add_argument("--tag", default=None, help="release tag (default: the exact git tag, else `dev`)")
    ap.add_argument("--rev", default=None, help="commit the bundle was built from (default: HEAD)")
    args = ap.parse_args(argv)

    if not args.no_build:
        build()
    tag = args.tag or tag_now()
    rev = args.rev or sh("git", "rev-parse", "HEAD")
    files = sources(DIST)
    out = pathlib.Path(args.out)
    manifest = write(out, files, tag, rev)

    print(f"\n[dist] {manifest['renderer']} {tag} ({rev[:12]}) → {out}")
    width = max(len(name) for name in manifest["files"])
    for name, meta in manifest["files"].items():
        print(f"  {name:<{width}}  {meta['size']:>9,} B  {meta['sha256']}")
    archive = out / f"fluxen-ui-{tag}.tar.gz"
    print(f"  {'fluxen-ui-' + tag + '.tar.gz':<{width}}  {archive.stat().st_size:>9,} B  (4 files + manifest)")
    return 0


if __name__ == "__main__":
    sys.exit(main())