#!/usr/bin/env python3
"""Build the studio's wasm backend and web bindings inside nix/runtime."""

import argparse
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true", help="build an optimized backend")
    args = parser.parse_args()
    given = os.environ.get("CARGO_TARGET_DIR")
    target_dir = Path(given).resolve() if given else ROOT / "src/runtime/target"
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir))
    cargo = ["cargo", "build", "--locked", "-p", "xem-studio", "--target", "wasm32-unknown-unknown"]
    if args.release:
        cargo.append("--release")
    subprocess.run(cargo, cwd=ROOT / "src/runtime", env=env, check=True)
    profile = "release" if args.release else "debug"
    wasm = target_dir / "wasm32-unknown-unknown" / profile / "xem_studio.wasm"
    out = ROOT / "tools/studio/src/backend/pkg"
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["wasm-bindgen", "--target", "web", "--out-dir", str(out), str(wasm)], check=True
    )
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
