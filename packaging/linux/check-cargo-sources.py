#!/usr/bin/env python3
"""CI: Flathub cargo-sources.json must cover monica-gtk/Cargo.lock."""
from __future__ import annotations

import json
import sys
import tomllib
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def git_rev(source: str) -> str | None:
    if "#" in source:
        return source.rsplit("#", 1)[-1]
    if "rev=" in source:
        return source.split("rev=", 1)[1].split("&", 1)[0].split("#", 1)[0]
    return None


def main() -> int:
    root = repo_root()
    sources_path = root / "packaging" / "flatpak" / "cargo-sources.json"
    lock_path = root / "monica-gtk" / "Cargo.lock"
    if not sources_path.is_file():
        print(f"missing {sources_path}", file=sys.stderr)
        return 1
    if not lock_path.is_file():
        print(f"missing {lock_path}", file=sys.stderr)
        return 1

    sources = json.loads(sources_path.read_text(encoding="utf-8"))
    if not isinstance(sources, list) or not sources:
        print("cargo-sources.json must be a non-empty JSON array", file=sys.stderr)
        return 1

    blob = json.dumps(sources)
    lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    errors: list[str] = []
    crates_io = 0
    git_pkgs = 0
    for package in lock.get("package", []):
        source = package.get("source")
        if not source:
            continue
        name = package["name"]
        version = package["version"]
        if source.startswith("registry+"):
            crates_io += 1
            needle = f"{name}/{name}-{version}.crate"
            if needle not in blob:
                errors.append(f"crates.io missing from cargo-sources: {name} {version}")
        elif source.startswith("git+"):
            git_pkgs += 1
            rev = git_rev(source)
            if not rev:
                errors.append(f"git package {name} has no rev in Cargo.lock")
            elif rev not in blob:
                errors.append(f"git rev missing from cargo-sources: {name} {rev}")

    if crates_io < 1:
        errors.append("Cargo.lock has no crates.io packages")
    if git_pkgs < 1:
        errors.append("Cargo.lock has no git packages (expected Monica-Pass/Mdbx)")
    if "static.crates.io/crates/" not in blob:
        errors.append("cargo-sources.json has no crates.io archives")
    if '"type": "git"' not in blob and '"type": "archive"' not in blob:
        errors.append("cargo-sources.json has no git/archive entries")

    if errors:
        print("check-cargo-sources: FAIL")
        for item in errors:
            print(f"  - {item}")
        return 1

    print(
        f"check-cargo-sources: ok ({len(sources)} entries, "
        f"{crates_io} crates.io packages, {git_pkgs} git packages from Cargo.lock)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
