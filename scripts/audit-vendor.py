#!/usr/bin/env python3
"""Audit the vendored crate sources against their own `.cargo-checksum.json`.

Why this exists: cargo *does* verify vendored files against the manifest, and it
names the offending path — but only when it actually rebuilds the crate. With a
warm `target/`, files can be deleted from `vendor/` and `cargo check` still
reports `Finished`. The damage stays invisible until the next cold build, which
means a fresh clone or CI rather than the machine that caused it. This audit
answers cold-build integrity from a warm tree.

It verifies sha256 by default, because that is the criterion cargo itself
applies — a tree that merely has every file present can still fail a cold build.
Hashing all 677 crates costs ~5 seconds, so there is no reason to default to the
weaker question. `--presence-only` asks it anyway, for the rare case where even
that is too slow.

Exits 0 when the tree is intact, 1 when it is not, naming every fault found.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

MANIFEST = ".cargo-checksum.json"


def faults_in(crate: Path, verify_hashes: bool) -> list[str]:
    """Every way `crate` disagrees with its own manifest, as human-readable lines."""
    manifest = crate / MANIFEST
    try:
        listed = json.loads(manifest.read_text())["files"]
    except (OSError, ValueError, KeyError) as exc:
        return [f"{manifest}: unreadable manifest ({exc})"]

    faults = []
    for relative, expected in listed.items():
        path = crate / relative
        if not path.is_file():
            faults.append(f"{path}: listed in {MANIFEST} but missing")
        elif verify_hashes:
            actual = hashlib.sha256(path.read_bytes()).hexdigest()
            if actual != expected:
                faults.append(f"{path}: sha256 {actual} != recorded {expected}")
    return faults


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=Path("vendor"),
                        help="the vendor directory to audit (default: vendor)")
    parser.add_argument("--presence-only", action="store_true",
                        help="check only that each listed file exists, skipping sha256")
    args = parser.parse_args()

    if not args.root.is_dir():
        print(f"{args.root}: not a directory", file=sys.stderr)
        return 1

    crates = sorted(d for d in args.root.iterdir() if (d / MANIFEST).is_file())
    if not crates:
        print(f"{args.root}: no vendored crates found (no {MANIFEST} anywhere)", file=sys.stderr)
        return 1

    verify_hashes = not args.presence_only
    faults = [fault for crate in crates for fault in faults_in(crate, verify_hashes)]
    for fault in faults:
        print(fault, file=sys.stderr)

    if faults:
        print(f"\n{len(faults)} fault(s) across {len(crates)} vendored crates", file=sys.stderr)
        return 1
    level = "present and sha256-matching" if verify_hashes else "present"
    print(f"{len(crates)} vendored crates: every file the manifests list is {level}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
